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
    Button, Checkbox, Combo, ComboItem, Control, EditCtxAction, InputEvent, Invalidations, Key,
    ScrollBars, TextBox, Widget,
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
    /// 입력란 우클릭 메뉴에서 고른 클립보드 행동 — OS 클립보드는 호스트(창) 몫.
    Edit(EditCtxAction),
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
    /// 상태 메시지(접속 실패/성공) — 워드랩 · 넘치면 세로 스크롤(사용자 09-14).
    status_scroll: i32,
    status_bars: ScrollBars,
    /// 페인트가 잰 상태 메시지 콘텐츠 높이(px) — 이벤트 경로(폰트 없음)가 스크롤 범위에 쓴다.
    status_content_h: std::cell::Cell<i32>,
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
            status_scroll: 0,
            status_bars: ScrollBars::new(),
            status_content_h: std::cell::Cell::new(0),
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

    /// 폼의 접속 명칭(프로필 이름) — 접속 성공 시 '한 번 이상 접속' 목록에 올린다.
    pub(crate) fn profile_name(&self) -> String {
        self.name.text().trim().to_string()
    }

    #[allow(dead_code)]
    pub(crate) fn is_connected(&self) -> bool {
        self.connected
    }

    pub(crate) fn state_ref(&self) -> &ConnState {
        &self.state
    }

    pub(crate) fn set_state(&mut self, s: ConnState) {
        self.connected = matches!(s, ConnState::Connected(_));
        self.state = s;
        self.status_scroll = 0;
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
        // 라벨 16 + 3 — 입력란 포커스 링(바깥 2px)이 위 라벨과 겹치던 문제(사용자 09-14).
        let label_h = self.s(19.0);
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
            // Port = 5자리("65535")가 들어가는 58(사용자 09-14 · 43은 4자리도 잘림) · Host도 같은 14px 축소 · PANEL_W 292.
            let port_w = self.s(58.0);
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
        // 버튼 3개 한 줄 — 동일 너비 · 동일 간격(나머지 px는 양끝에 나눠 중앙 정렬 · 사용자 09-14).
        let bw = (w - gap * 2) / 3;
        let bh = self.s(28.0);
        let x0 = x + (w - (bw * 3 + gap * 2)) / 2;
        self.test_btn.set_bounds(Rect::new(x0, y, bw, bh), &mut inv);
        self.connect_btn
            .set_bounds(Rect::new(x0 + bw + gap, y, bw, bh), &mut inv);
        self.save_btn
            .set_bounds(Rect::new(x0 + (bw + gap) * 2, y, bw, bh), &mut inv);
    }

    /// 상태줄 자리(버튼 아래).
    /// 상태 메시지 영역 — 버튼 아래부터 패널 바닥까지(워드랩 · 넘치면 스크롤).
    fn status_rect(&self) -> Rect {
        let b = self.test_btn.bounds();
        let pad = self.s(10.0);
        let y = b.bottom() + self.s(10.0);
        Rect::new(
            self.bounds.x + pad,
            y,
            self.bounds.w - pad * 2,
            (self.bounds.bottom() - pad - y).max(self.s(24.0)),
        )
    }

    /// 상태 메시지 스크롤 — 휠/썸 드래그. 소비했으면 true.
    fn status_scroll_event(&mut self, ev: &InputEvent) -> bool {
        let sr = self.status_rect();
        let ch = self.status_content_h.get();
        if ch <= sr.h {
            return false;
        }
        let (_, ny, consumed) =
            self.status_bars
                .on_event(ev, sr, sr.w, ch, 0, self.status_scroll, self.scale);
        let moved = ny != self.status_scroll;
        self.status_scroll = ny.clamp(0, (ch - sr.h).max(0));
        moved || consumed
    }

    /// 상태 메시지 스크롤바 틱 — 다시 그려야 하면 true.
    pub(crate) fn tick_status(&mut self, now_ms: u64) -> bool {
        self.status_bars.tick(now_ms)
    }

    pub(crate) fn status_bars_visible(&self) -> bool {
        self.status_bars.is_visible()
    }

    // ── 포커스

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if on && self.field_focus.is_none() {
            self.field_focus = Some(Field::Name);
        }
        self.sync_focus();
        if !on {
            // 패널이 포커스를 잃으면 버튼·콤보·체크박스 링도 전부 꺼진다(포커스 규칙 — CLAUDE.md §3).
            self.own_focus(None);
        }
    }

    /// ★ 포커스 규칙(사용자 09-14 · CLAUDE.md §3): 한 창엔 포커스 링이 **정확히 하나 이하**.
    /// nexa-ctl 버튼·콤보·체크박스는 MouseDown에서 스스로 `focused = true`가 되고 스스로 끄지 않는다 —
    /// 그래서 누를 때마다 여기서 한 번 정리한다: `hit`(방금 눌린 컨트롤)만 켜고 나머지는 끈다. 텍스트박스가 눌렸으면 전부 끈다.
    fn own_focus(&mut self, hit: Option<usize>) {
        fn set<C: Control>(c: &mut C, on: bool) {
            if c.is_focused() != on {
                c.set_focused(on);
            }
        }
        set(&mut self.dialect, hit == Some(0));
        set(&mut self.save_pw, hit == Some(1));
        set(&mut self.test_btn, hit == Some(2));
        set(&mut self.connect_btn, hit == Some(3));
        set(&mut self.save_btn, hit == Some(4));
    }

    /// 눌린 지점이 어느 비텍스트 컨트롤인가(`own_focus`의 index).
    fn ctl_hit(&self, p: Point) -> Option<usize> {
        [
            self.dialect.bounds(),
            self.save_pw.bounds(),
            self.test_btn.bounds(),
            self.connect_btn.bounds(),
            self.save_btn.bounds(),
        ]
        .iter()
        .position(|b| b.contains(p))
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

    /// 버튼 hover 페이드 틱 — 밝기가 변했으면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.test_btn.tick(now_ms);
        let b = self.connect_btn.tick(now_ms);
        let c = self.save_btn.tick(now_ms);
        let d = self.dialect.tick_hover(now_ms);
        // 입력란 hover 페이드(회색 · Slow).
        let mut e = false;
        for f in FIELDS {
            e |= self.textbox(f).tick(now_ms);
        }
        let f = self.tick_status(now_ms);
        a || b || c || d || e || f
    }

    pub(crate) fn animating(&self) -> bool {
        self.test_btn.is_animating()
            || self.connect_btn.is_animating()
            || self.save_btn.is_animating()
            || self.dialect.hover_animating()
            || FIELDS.iter().any(|&f| self.textbox_ref(f).is_animating())
            || self.status_bars_visible()
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

    /// 지금 입력된 비밀번호(저장 여부와 무관 · 세션 보관용).
    pub(crate) fn password_text(&self) -> String {
        self.password.text()
    }

    /// 종류 콤보 영역(우클릭 메뉴 판정).
    pub(crate) fn dialect_bounds(&self) -> Rect {
        self.dialect.bounds()
    }

    /// 종류 콤보의 선택 항목 라벨.
    pub(crate) fn dialect_selected_label(&self) -> String {
        let c = self.dialect.core();
        c.items()
            .get(c.selected_index())
            .map(|it| it.label.clone())
            .unwrap_or_default()
    }

    /// 종류 콤보의 전체 항목 라벨(줄마다 하나).
    pub(crate) fn dialect_labels(&self) -> String {
        self.dialect
            .core()
            .items()
            .iter()
            .map(|it| it.label.as_str())
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    /// 어느 입력란의 우클릭 편집 메뉴가 열려 있는가(모달).
    pub(crate) fn edit_menu_open(&self) -> bool {
        self.field_focus
            .is_some_and(|f| self.textbox_ref(f).popup_open())
    }

    fn textbox_ref(&self, f: Field) -> &TextBox {
        match f {
            Field::Host => &self.host,
            Field::Port => &self.port,
            Field::Database => &self.database,
            Field::User => &self.user,
            Field::Password => &self.password,
            Field::Name => &self.name,
        }
    }

    /// 입력란 팝업(우클릭 편집 메뉴)만 — 창이 **맨 마지막**에 부른다(다른 컨트롤이 메뉴 위에 그려지던 버그 · 사용자 09-14).
    pub(crate) fn paint_popups(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        for tb in [
            &self.host,
            &self.port,
            &self.database,
            &self.user,
            &self.password,
            &self.name,
        ] {
            tb.paint_popup(dc, th);
        }
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
        // 상태 메시지 스크롤(휠 · 썸 드래그) — 넘칠 때만 소비.
        if matches!(
            ev,
            InputEvent::Wheel { .. }
                | InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        ) && self.status_scroll_event(ev)
        {
            inv.push(self.bounds);
            return None;
        }
        // 열린 편집 메뉴(입력란 우클릭)도 모달 — 그 입력란만 받는다 · 고른 행동은 호스트에.
        if let Some(f) = self
            .field_focus
            .filter(|&f| self.textbox_ref(f).popup_open())
        {
            let tb = self.textbox(f);
            tb.on_event(ev, inv);
            if let Some(act) = tb.take_edit_ctx() {
                return Some(PanelAction::Edit(act));
            }
            return None;
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
            } else if self.ctl_hit(p).is_some() {
                self.field_focus = None;
                self.sync_focus();
            }
        }
        // 마우스 사건은 포커스 입력란(드래그 선택 · 우클릭 메뉴)에도 — 버튼·콤보·체크박스보다 먼저.
        if matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
                | InputEvent::RightDown { .. }
        ) {
            if let Some(f) = self.field_focus {
                let tb = self.textbox(f);
                tb.on_event(ev, inv);
                if let Some(act) = tb.take_edit_ctx() {
                    return Some(PanelAction::Edit(act));
                }
            }
        }
        if matches!(ev, InputEvent::RightDown { .. }) {
            return None;
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
            // 포커스 링은 하나만 — 방금 눌린 컨트롤만 켠다(텍스트박스면 전부 끈다).
            if let InputEvent::MouseDown { x, y, .. } = *ev {
                let hit = self.ctl_hit(Point { x, y });
                self.own_focus(hit);
            }
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
        if matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        ) {
            // 마우스는 위에서 이미 입력란에 전달했다(두 번 보내면 드래그가 꼬인다).
            return None;
        }
        if let Some(f) = self.field_focus {
            let tb = self.textbox(f);
            tb.on_event(ev, inv);
            if let Some(act) = tb.take_edit_ctx() {
                return Some(PanelAction::Edit(act));
            }
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
        // 라벨 16 + 3 — 입력란 포커스 링(바깥 2px)이 위 라벨과 겹치던 문제(사용자 09-14).
        let label_h = self.s(19.0);
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
        // 워드랩 + 세로 스크롤(오버레이 막대 · 사용자 09-14).
        let dot = self.s(8.0);
        let tx = sr.x + dot + self.s(6.0);
        let line_h = dc.text_height() + self.s(2.0);
        let lines = wrap_words(dc, &text, (sr.right() - tx).max(1));
        let content_h = line_h * lines.len() as i32;
        self.status_content_h.set(content_h);
        let scroll = self.status_scroll.clamp(0, (content_h - sr.h).max(0));
        dc.fill_ellipse(
            Rect::new(sr.x, sr.y + self.s(4.0) - scroll, dot, dot).intersection(&sr),
            color,
        );
        let mut y = sr.y - scroll;
        for line in &lines {
            if y + line_h >= sr.y && y < sr.bottom() {
                dc.text(tx, y, sr, line, th.text);
            }
            y += line_h;
        }
        self.status_bars
            .paint(dc, th, sr, sr.w, content_h.max(sr.h), 0, scroll, self.scale);
        // 콤보는 팝업을 스스로 그린다 — 열린 것이 최상위가 되게 마지막에.
        self.dialect.paint(dc, th);
        // 입력란 팝업은 여기서 그리지 않는다 — 창이 맨 마지막에 `paint_popups`로(최상위).
    }
}

/// 단어 단위 워드랩 — `max_w` 안에 맞게 줄을 나눈다(한 단어가 넘치면 글자 단위로 자른다 · 줄바꿈 문자는 존중).
fn wrap_words(dc: &mut dyn DrawCtx, text: &str, max_w: i32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let cand = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if dc.text_width(&cand) <= max_w {
                line = cand;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            // 한 단어가 폭을 넘으면 글자 단위로.
            let mut piece = String::new();
            for ch in word.chars() {
                let mut try_p = piece.clone();
                try_p.push(ch);
                if dc.text_width(&try_p) <= max_w || piece.is_empty() {
                    piece = try_p;
                } else {
                    out.push(std::mem::take(&mut piece));
                    piece.push(ch);
                }
            }
            line = piece;
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}
