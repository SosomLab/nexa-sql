//! 접속 패널(기본 UI · [docs/22 §1](../../../docs/22-driver-extensions.md) DBeaver 폼 + Golden 로그인 리스트의 축소판).
//!
//! 사용자 09-14: *"UI는 기본만 · DB 종류·접속 정보 설정 → 연결 버튼 → 접속 상태 확인"*.
//! 핵심 로직은 전부 공용 층 — 스펙 조립 = [`ConnectSpec::from_parts`] · 테스트 = `nsql_run::test_connection` ·
//! 저장 = `nsql_vault` — CLI `nsql conn add/test`와 **같은 함수**를 부른다(패널은 값을 모아 [`PanelAction`]으로 내보낼 뿐).
//!
//! 구성(세로 폼): 프로필 콤보(Golden 로그인 리스트 자리) · DB 종류 · 호스트 · 포트 · 데이터베이스/서비스(파일 기반 방언은 파일/DSN) ·
//! 사용자 · 비밀번호(마스킹) · 비밀번호 저장 · 프로필 이름 · [Test] [Connect/Disconnect] [Save] · 상태줄(● 색 + 문구).

use nexa_ctl::controls::{ButtonTone, ComboControl};
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
        /// 목록에서 불러온 프로필의 이름을 바꿔 저장한 경우 = 옛 이름(호스트가 옛 항목을 지운다 · 사용자 09-16
        /// "New가 아니면 새 프로필이 아니라 이름 변경"). New(빈 폼)면 None.
        rename_from: Option<String>,
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
    /// 기본 스키마(`?schema=` · Oracle CURRENT_SCHEMA · PG search_path · 그 외 숨김 · 사용자 09-18).
    Schema,
    User,
    Password,
    Name,
}

const FIELDS: [Field; 7] = [
    Field::Name,
    Field::Host,
    Field::Port,
    Field::Database,
    Field::Schema,
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
    /// Port 입력란 폭(논리 px · 설정 `conn.port_w` · 기본 72 — 09-19 mac에서 58은 "1522"가 잘렸다).
    port_w: f32,
    focused: bool,
    dialect: Combo,
    host: TextBox,
    port: TextBox,
    database: TextBox,
    schema: TextBox,
    user: TextBox,
    password: TextBox,
    name: TextBox,
    /// 목록에서 불러온 프로필 이름(New/clear면 None) — 저장 시 이름 변경 판정용(09-16).
    loaded_name: Option<String>,
    /// 접속 유형(docs/56 §4 · 목록 우클릭 메뉴로 지정 · 폼은 값을 보존만 한다).
    env: Option<nsql_script::ConnEnv>,
    save_pw: Checkbox,
    test_btn: Button,
    connect_btn: Button,
    save_btn: Button,
    field_focus: Option<Field>,
    /// 동작(Test/Connect/Save)을 눌렀을 때 비어 있던 필수 칸 — 경고 띠 · 채우면 즉시 해제(22 §10).
    warn: Vec<Field>,
    /// 방언 콤보가 바꾼 기본 포트(사용자가 손대지 않은 포트 칸만 따라간다).
    auto_port: Option<u16>,
    pub(crate) state: ConnState,
    connected: bool,
    /// 저장본 스냅숏(불러오기·New·Save 성공 때 갱신) — 지금 값과 다르면 "바뀜"(T-131).
    snap: FormSnap,
    /// 마지막 `sync_dirty`의 바뀐 칸(그리기는 &self라 여기서 읽는다).
    dirty_now: Vec<Dirty>,
}

/// 폼 값의 스냅숏(저장본 · 09-19 T-131) — 칸별 비교로 "바뀜"을 판정한다(공백은 양끝만 정규화).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct FormSnap {
    pub name: String,
    pub dialect: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub schema: String,
    pub user: String,
    pub password: String,
    pub save_pw: bool,
}

/// 바뀐 칸(순수 판정 · MC/DC: 칸마다 독립).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dirty {
    Name,
    Dialect,
    Host,
    Port,
    Database,
    Schema,
    User,
    Password,
    SavePw,
}

/// 어느 동작의 필수 검사인가(22 §10): Save는 Password 대신 프로필 이름.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormAction {
    Connect,
    Save,
}

/// 폼의 칸 ↔ 원장의 로그인 항목.
fn field_of(f: nsql_core::LoginField) -> Field {
    use nsql_core::LoginField as L;
    match f {
        L::Host => Field::Host,
        L::Port => Field::Port,
        L::Database => Field::Database,
        L::Schema => Field::Schema,
        L::User => Field::User,
        L::Password => Field::Password,
    }
}

/// 이 방언·동작에서 필수인 폼 칸(라벨 `*`의 근거 · 원장 = `Dialect::login_required`).
fn required_fields(dialect: Dialect, action: FormAction) -> Vec<Field> {
    let mut out: Vec<Field> = dialect
        .login_required()
        .iter()
        .copied()
        .map(field_of)
        .filter(|f| action == FormAction::Connect || *f != Field::Password)
        .collect();
    if action == FormAction::Save {
        out.insert(0, Field::Name);
    }
    out
}

/// 빠진 필수 칸(순수 판정 · MC/DC: 방언 × 동작 × 칸별 빈 값) — 폼 순서대로.
fn missing_fields(dialect: Dialect, action: FormAction, now: &FormSnap) -> Vec<Field> {
    let empty = |f: Field| match f {
        Field::Name => now.name.trim().is_empty(),
        Field::Host => now.host.trim().is_empty(),
        Field::Port => now.port.trim().is_empty(),
        Field::Database => now.database.trim().is_empty(),
        Field::Schema => now.schema.trim().is_empty(),
        Field::User => now.user.trim().is_empty(),
        Field::Password => now.password.is_empty(),
    };
    let req = required_fields(dialect, action);
    let mut out: Vec<Field> = Vec::new();
    for f in [Field::Name]
        .into_iter()
        .chain(FIELDS.into_iter().filter(|f| *f != Field::Name))
    {
        if req.contains(&f) && empty(f) && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

pub(crate) fn dirty_fields(base: &FormSnap, now: &FormSnap) -> Vec<Dirty> {
    let ne = |a: &str, b: &str| a.trim() != b.trim();
    let mut out = Vec::new();
    if ne(&base.name, &now.name) {
        out.push(Dirty::Name);
    }
    if ne(&base.dialect, &now.dialect) {
        out.push(Dirty::Dialect);
    }
    if ne(&base.host, &now.host) {
        out.push(Dirty::Host);
    }
    if ne(&base.port, &now.port) {
        out.push(Dirty::Port);
    }
    if ne(&base.database, &now.database) {
        out.push(Dirty::Database);
    }
    if ne(&base.schema, &now.schema) {
        out.push(Dirty::Schema);
    }
    if ne(&base.user, &now.user) {
        out.push(Dirty::User);
    }
    // 비밀번호는 공백도 값이다.
    if base.password != now.password {
        out.push(Dirty::Password);
    }
    if base.save_pw != now.save_pw {
        out.push(Dirty::SavePw);
    }
    out
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
            port_w: 72.0,
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
            schema: TextBox::new(t(Msg::PhSchema)),
            user: TextBox::new(t(Msg::PhUser)),
            password: TextBox::new(t(Msg::PhPassword)),
            name: TextBox::new(t(Msg::PhProfileName)),
            loaded_name: None,
            env: None,
            save_pw: Checkbox::new(t(Msg::LblSavePassword), true),
            test_btn: Button::new(t(Msg::BtnTest)),
            connect_btn: Button::new(t(Msg::BtnConnect)),
            save_btn: Button::new(t(Msg::BtnSave)),
            field_focus: None,
            warn: Vec::new(),
            auto_port: None,
            snap: FormSnap::default(),
            dirty_now: Vec::new(),
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
        rebuild(&mut self.database, &db_ph, false);
        rebuild(&mut self.schema, t(Msg::PhSchema), false);
        rebuild(&mut self.user, t(Msg::PhUser), false);
        rebuild(&mut self.password, t(Msg::PhPassword), true);
        rebuild(&mut self.name, t(Msg::PhProfileName), false);
        self.layout_inner();
    }

    fn selected_dialect(&self) -> Dialect {
        Dialect::from_name(&self.dialect.selected_value()).unwrap_or(Dialect::Sqlite)
    }

    fn database_placeholder(&self) -> String {
        let d = self.selected_dialect();
        let base = match d {
            Dialect::Sqlite => t(Msg::PhSqliteFile),
            Dialect::Odbc => t(Msg::PhDsn),
            Dialect::Oracle => t(Msg::PhService),
            _ => t(Msg::PhDatabase),
        };
        // 선택 칸은 "(optional)"을 뒤에(22 §10 · 필수 칸은 `*`).
        if d.login_required()
            .contains(&nsql_core::LoginField::Database)
        {
            base.to_string()
        } else {
            format!("{base} {}", t(Msg::PhOptional))
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
        self.loaded_name = Some(name.to_string());
        self.env = spec.env;
        if let Some(d) = spec.dialect {
            self.dialect.select_value(&d.to_string());
            self.auto_port = d.default_port();
        }
        self.host.set_text(spec.host.as_deref().unwrap_or(""));
        self.port
            .set_text(&spec.port.map(|p| p.to_string()).unwrap_or_default());
        self.database
            .set_text(spec.database.as_deref().unwrap_or(""));
        self.schema.set_text(spec.schema.as_deref().unwrap_or(""));
        self.user.set_text(spec.user.as_deref().unwrap_or(""));
        self.password
            .set_text(spec.password.as_deref().unwrap_or(""));
        self.save_pw.set_checked(spec.password.is_some());
        self.name.set_text(name);
        let d = self.selected_dialect();
        self.apply_dialect(d);
        self.mark_saved();
    }

    /// 폼 → 스펙(GUI·CLI 공용 검증). 실패 문구는 상태줄로.
    fn build_spec(&self) -> Result<ConnectSpec, String> {
        let port = self.port.text().trim().parse::<u16>().ok();
        let mut spec = ConnectSpec::from_parts(
            self.selected_dialect(),
            &self.host.text(),
            port,
            &self.database.text(),
            &self.user.text(),
            &self.password.text(),
        )?;
        let sc = self.schema.text();
        if self.schema_applies() && !sc.trim().is_empty() {
            spec.schema = Some(sc.trim().to_string());
        }
        // 접속 유형은 폼에 칸이 없다(목록 우클릭 메뉴) — 불러온 값을 그대로 실어 저장·접속에서 잃지 않게 한다.
        spec.env = self.env;
        Ok(spec)
    }

    /// 목록 메뉴에서 유형을 바꿨다 — 그 프로필이 폼에 올라와 있으면 폼의 값도 맞춘다.
    pub(crate) fn set_env_if_loaded(&mut self, name: &str, env: Option<nsql_script::ConnEnv>) {
        if self.loaded_name.as_deref() == Some(name) {
            self.env = env;
        }
    }

    /// 기본 스키마 칸이 뜻이 있는 방언(Oracle · PostgreSQL) — SQL Server는 Database가 그 자리 · 파일 방언은 없음.
    fn schema_applies(&self) -> bool {
        matches!(self.selected_dialect(), Dialect::Oracle | Dialect::Postgres)
    }

    /// 새 프로필 — 폼 비우기(DB 종류는 유지).
    pub(crate) fn clear(&mut self) {
        self.loaded_name = None;
        self.env = None;
        for tb in [
            &mut self.host,
            &mut self.port,
            &mut self.database,
            &mut self.schema,
            &mut self.user,
            &mut self.password,
            &mut self.name,
        ] {
            tb.set_text("");
        }
        self.save_pw.set_checked(false);
        let d = self.selected_dialect();
        self.apply_dialect(d);
        self.mark_saved();
    }

    /// 지금 값의 스냅숏.
    pub(crate) fn snapshot(&self) -> FormSnap {
        FormSnap {
            name: self.name.text(),
            dialect: self.dialect.selected_value(),
            host: self.host.text(),
            port: self.port.text(),
            database: self.database.text(),
            schema: if self.schema_applies() {
                self.schema.text()
            } else {
                String::new()
            },
            user: self.user.text(),
            password: self.password.text(),
            save_pw: self.save_pw.is_checked(),
        }
    }

    /// 지금 값을 저장본으로(불러오기 · New · Save 성공).
    pub(crate) fn mark_saved(&mut self) {
        self.snap = self.snapshot();
    }

    /// 저장본과 다른 칸.
    pub(crate) fn dirty(&self) -> Vec<Dirty> {
        dirty_fields(&self.snap, &self.snapshot())
    }

    pub(crate) fn is_dirty(&self) -> bool {
        !self.dirty().is_empty()
    }

    /// 자체 캡처용(`NSQL_STARTUP_CMD conn.edit:<프로필>`): 서비스/DB 칸과 사용자 칸에 글자를 덧붙여 "바뀜" 표식을 띄운다.
    pub(crate) fn capture_touch(&mut self) {
        let d = format!("{}X", self.database.text());
        self.database.set_text(&d);
        let u = format!("{}X", self.user.text());
        self.user.set_text(&u);
    }

    /// 그리기 직전(창이 부른다): 바뀐 칸을 컨트롤 표시(입력란 띠 · Save 색/라벨)에 반영한다.
    pub(crate) fn sync_dirty(&mut self) {
        let dirty = self.dirty();
        let is = |d: Dirty| dirty.contains(&d);
        self.host.set_modified(is(Dirty::Host));
        self.port.set_modified(is(Dirty::Port));
        self.database.set_modified(is(Dirty::Database));
        self.schema.set_modified(is(Dirty::Schema));
        self.user.set_modified(is(Dirty::User));
        self.password.set_modified(is(Dirty::Password));
        self.name.set_modified(is(Dirty::Name));
        // 경고 띠(빠진 필수 칸) — 글자를 넣으면 즉시 해제 · 경고 > 바뀜.
        let snap = self.snapshot();
        self.warn.retain(|f| match f {
            Field::Name => snap.name.trim().is_empty(),
            Field::Host => snap.host.trim().is_empty(),
            Field::Port => snap.port.trim().is_empty(),
            Field::Database => snap.database.trim().is_empty(),
            Field::Schema => snap.schema.trim().is_empty(),
            Field::User => snap.user.trim().is_empty(),
            Field::Password => snap.password.is_empty(),
        });
        for f in FIELDS {
            let on = self.warn.contains(&f);
            self.textbox(f).set_warning(on);
        }
        let n = dirty.len();
        self.save_btn.set_tone(if n > 0 {
            ButtonTone::Accent
        } else {
            ButtonTone::Default
        });
        // 라벨은 늘 "Save" — 바뀜 표식은 그릴 때 같은 크기의 원으로 붙인다(글리프 `•`는 글꼴마다 크기가 달랐다).
        self.save_btn.set_label(t(Msg::BtnSave).to_string());
        self.dirty_now = dirty;
    }

    /// 목록에서 불러온 프로필 이름(New면 None).
    pub(crate) fn loaded_name(&self) -> Option<String> {
        self.loaded_name.clone()
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
            &mut self.schema,
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
            let port_w = self.s(self.port_w);
            self.host
                .set_bounds(Rect::new(x, y, w - port_w - gap, field_h), &mut inv);
            self.port
                .set_bounds(Rect::new(x + w - port_w, y, port_w, field_h), &mut inv);
            y += field_h + gap;
        }
        place(&mut self.database, &mut inv, &mut y, field_h);
        if self.schema_applies() {
            place(&mut self.schema, &mut inv, &mut y, field_h);
        } else {
            self.schema.set_bounds(off, &mut inv);
        }
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
            Field::Schema => &mut self.schema,
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
            .filter(|f| !(matches!(f, Field::Schema) && !self.schema_applies()))
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

    /// Port 입력란 폭 지정(설정 주입).
    pub(crate) fn set_port_w(&mut self, w: f32) {
        self.port_w = w.max(24.0);
    }

    /// 이 프로필이 테스트 중이면 Test/Connect 버튼을 잠근다(끝나면 해제 · 사용자 09-14).
    pub(crate) fn set_testing_lock(&mut self, locked: bool) {
        self.test_btn.set_enabled(!locked);
        self.connect_btn.set_enabled(!locked);
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
            Field::Schema => &self.schema,
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
            &self.schema,
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

    /// 필수 칸 검사(22 §10): 빠진 칸이 있으면 경고 띠 + 상태줄 `Required: …` + 첫 칸 포커스 → false(동작 안 함).
    fn check_required(&mut self, action: FormAction) -> bool {
        let missing = missing_fields(self.selected_dialect(), action, &self.snapshot());
        if missing.is_empty() {
            self.warn.clear();
            return true;
        }
        let names: Vec<String> = missing.iter().map(|f| self.field_label(*f)).collect();
        self.set_state(ConnState::Failed(tf(
            Msg::StRequiredFields,
            &[&names.join(", ")],
        )));
        self.field_focus = Some(missing[0]);
        self.focused = true;
        self.sync_focus();
        self.warn = missing;
        false
    }

    /// 칸 라벨(방언에 따라 Database 칸 이름이 다르다).
    fn field_label(&self, f: Field) -> String {
        match f {
            Field::Name => t(Msg::LblProfileName).to_string(),
            Field::Host => t(Msg::LblHost).to_string(),
            Field::Port => t(Msg::LblPort).to_string(),
            Field::Database => match self.selected_dialect() {
                Dialect::Sqlite => t(Msg::LblFile),
                Dialect::Odbc => t(Msg::LblDsn),
                Dialect::Oracle => t(Msg::LblService),
                _ => t(Msg::LblDatabase),
            }
            .to_string(),
            Field::Schema => t(Msg::LblSchema).to_string(),
            Field::User => t(Msg::LblUser).to_string(),
            Field::Password => t(Msg::LblPassword).to_string(),
        }
    }

    fn act_test(&mut self) -> Option<PanelAction> {
        if !self.check_required(FormAction::Connect) {
            return None;
        }
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

    /// 저장(잃는 순간 지킴이 "변경 저장" · T-131).
    pub(crate) fn save_action(&mut self) -> Option<PanelAction> {
        self.act_save()
    }

    /// 변경 버리기 — 저장본(스냅숏)으로 되돌린다.
    pub(crate) fn discard_changes(&mut self) {
        let snap = self.snap.clone();
        self.dialect.select_value(&snap.dialect);
        self.host.set_text(&snap.host);
        self.port.set_text(&snap.port);
        self.database.set_text(&snap.database);
        self.schema.set_text(&snap.schema);
        self.user.set_text(&snap.user);
        self.password.set_text(&snap.password);
        self.name.set_text(&snap.name);
        self.save_pw.set_checked(snap.save_pw);
        let d = self.selected_dialect();
        self.apply_dialect(d);
        self.snap = snap;
    }

    fn act_connect(&mut self) -> Option<PanelAction> {
        if self.connected {
            return Some(PanelAction::Disconnect);
        }
        if !self.check_required(FormAction::Connect) {
            return None;
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
        if !self.check_required(FormAction::Save) {
            return None;
        }
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
                // 불러온 프로필의 이름을 바꿨으면 "이름 변경"(옛 항목 제거) — 새로 만들지 않는다.
                let rename_from = self.loaded_name.clone().filter(|old| old != &name);
                Some(PanelAction::Save {
                    name,
                    spec,
                    rename_from,
                })
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
        // 바뀐 칸(T-131): 입력란 왼쪽 띠(`sync_dirty`가 켰다) + 라벨 끝 ` •` · 콤보/체크는 왼쪽에 같은 띠를 여기서 그린다.
        let dirty = &self.dirty_now;
        let is = |d: Dirty| dirty.contains(&d);
        let stripe = |dc: &mut dyn DrawCtx, r: Rect| {
            if r.w > 0 {
                let m = self.s(4.0);
                dc.fill_rect(
                    Rect::new(r.x + 1, r.y + m, self.s(2.0).max(1), (r.h - m * 2).max(1)),
                    th.accent,
                );
            }
        };
        // 필수 칸 = 라벨 뒤 `*`(Azure Data Studio식 · 22 §10) — 방언을 바꾸면 따라 바뀐다 · 빠진 칸은 경고색.
        let req = required_fields(self.selected_dialect(), FormAction::Connect);
        let req_name = true; // 프로필 이름은 Save에 필수.
        let warn = self.warn.clone();
        let label = |dc: &mut dyn DrawCtx, r: Rect, text: &str, changed: bool, field: Field| {
            if r.w == 0 {
                return;
            }
            let y = r.y - label_h + self.s(1.0);
            let missing = warn.contains(&field);
            dc.text(r.x, y, b, text, if missing { th.warn } else { th.text_dim });
            let mut x = r.x + dc.text_width(text);
            // ★ 표식 = **같은 크기의 원**(필수 = 빨강 · 바뀜 = 파랑) · 글자 줄의 세로 중앙(사용자 09-19 — 종전에는 글리프
            //   `*`·`•`라 글꼴마다 크기·높이가 달랐다).
            let cy = y + dc.text_height() / 2 + self.s(1.0); // 글자 상자 중앙보다 1px 아래 = 눈에 보이는 글자 중앙
            let required = if field == Field::Name {
                req_name
            } else {
                req.contains(&field)
            };
            if required {
                x += self.s(MARK_GAP);
                mark_dot(
                    dc,
                    x,
                    cy,
                    self.s(MARK_D),
                    if missing { th.warn } else { th.danger },
                );
                x += self.s(MARK_D);
            }
            if changed {
                x += self.s(MARK_GAP);
                mark_dot(dc, x, cy, self.s(MARK_D), th.accent);
            }
        };
        label(
            dc,
            self.name.bounds(),
            t(Msg::LblProfileName),
            is(Dirty::Name),
            Field::Name,
        );
        // DB 종류 콤보 = 필수이나 늘 값이 있다(`*` 없이).
        {
            let r = self.dialect.bounds();
            if r.w > 0 {
                let y = r.y - label_h + self.s(1.0);
                let text = t(Msg::LblDbType);
                dc.text(r.x, y, b, text, th.text_dim);
                if is(Dirty::Dialect) {
                    let w = dc.text_width(text);
                    let cy = y + dc.text_height() / 2 + self.s(1.0);
                    mark_dot(
                        dc,
                        r.x + w + self.s(MARK_GAP),
                        cy,
                        self.s(MARK_D),
                        th.accent,
                    );
                }
            }
        }
        label(
            dc,
            self.host.bounds(),
            t(Msg::LblHost),
            is(Dirty::Host),
            Field::Host,
        );
        label(
            dc,
            self.port.bounds(),
            t(Msg::LblPort),
            is(Dirty::Port),
            Field::Port,
        );
        let db_label = match self.selected_dialect() {
            Dialect::Sqlite => t(Msg::LblFile),
            Dialect::Odbc => t(Msg::LblDsn),
            Dialect::Oracle => t(Msg::LblService),
            _ => t(Msg::LblDatabase),
        };
        label(
            dc,
            self.database.bounds(),
            db_label,
            is(Dirty::Database),
            Field::Database,
        );
        if self.schema_applies() {
            label(
                dc,
                self.schema.bounds(),
                t(Msg::LblSchema),
                is(Dirty::Schema),
                Field::Schema,
            );
        }
        label(
            dc,
            self.user.bounds(),
            t(Msg::LblUser),
            is(Dirty::User),
            Field::User,
        );
        label(
            dc,
            self.password.bounds(),
            t(Msg::LblPassword),
            is(Dirty::Password),
            Field::Password,
        );
        for tb in [
            &self.host,
            &self.port,
            &self.database,
            &self.schema,
            &self.user,
            &self.password,
            &self.name,
        ] {
            if tb.bounds().w > 0 {
                tb.paint(dc, th);
            }
        }
        self.save_pw.paint(dc, th);
        if is(Dirty::SavePw) {
            stripe(dc, self.save_pw.bounds());
        }
        if is(Dirty::Dialect) {
            stripe(dc, self.dialect.bounds());
        }
        self.test_btn.paint(dc, th);
        self.connect_btn.paint(dc, th);
        // Save = 바뀐 칸이 있으면 강조 + `Save •`(`sync_dirty` · 툴바 Commit 배지와 같은 문법).
        let n = dirty.len();
        self.save_btn.paint(dc, th);
        if n > 0 {
            // 가운데 정렬된 라벨의 오른쪽 끝 + 간격 · 버튼의 세로 중앙 · 색 = 강조 버튼의 글자색(흰색).
            let sb = self.save_btn.bounds();
            let tw = dc.text_width(t(Msg::BtnSave));
            let x = sb.x + (sb.w + tw) / 2 + self.s(MARK_GAP);
            mark_dot(
                dc,
                x,
                sb.y + sb.h / 2,
                self.s(MARK_D),
                nexa_ctl::Color(0x00FF_FFFF),
            );
        }
        // 상태줄: ● + 문구(패널 폭 안에서 잘림)
        let sr = self.status_rect();
        let (color, text) = match &self.state {
            // 바뀐 채 아무 결과도 없으면 "저장 필요"를 상태로(결과가 오면 결과가 우선 · 상태는 한 번에 하나).
            ConnState::Idle if n > 0 => (th.accent, tf(Msg::StUnsaved, &[&n.to_string()])),
            ConnState::Idle => (th.text_dim, t(Msg::StIdle).to_string()),
            ConnState::Connecting => (th.warn, t(Msg::StConnectingShort).to_string()),
            ConnState::Testing => (th.warn, t(Msg::StTesting).to_string()),
            ConnState::Connected(d) => (th.ok, tf(Msg::StConnectedShort, &[d])),
            ConnState::TestOk(d) => (th.ok, d.clone()),
            ConnState::Failed(e) => (th.danger, e.clone()),
        };
        // 워드랩 + 세로 스크롤(오버레이 막대 · 사용자 09-14).
        let dot = self.s(MARK_D);
        let tx = sr.x + dot + self.s(6.0);
        let line_h = dc.text_height() + self.s(2.0);
        let lines = wrap_words(dc, &text, (sr.right() - tx).max(1));
        let content_h = line_h * lines.len() as i32;
        self.status_content_h.set(content_h);
        let scroll = self.status_scroll.clamp(0, (content_h - sr.h).max(0));
        // 첫 줄 글자의 세로 중앙에(스크롤되면 같이 올라간다 · 영역 밖은 그리지 않는다).
        let dot_y = sr.y - scroll + (dc.text_height() - dot) / 2 + self.s(1.0);
        if dot_y >= sr.y && dot_y + dot <= sr.bottom() {
            dc.fill_ellipse(Rect::new(sr.x, dot_y, dot, dot), color);
        }
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

/// 폼 표식(필수 · 바뀜 · 상태)의 **원 지름**과 앞 간격(논리 px) — 전부 같은 크기(사용자 09-19).
const MARK_D: f32 = 6.0;
const MARK_GAP: f32 = 4.0;

/// 표식 원 — 왼쪽 끝 `x` · 세로 중심 `cy` · 지름 `d`.
fn mark_dot(dc: &mut dyn DrawCtx, x: i32, cy: i32, d: i32, color: nexa_ctl::Color) {
    dc.fill_ellipse(Rect::new(x, cy - d / 2, d, d), color);
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

#[cfg(test)]
mod dirty_tests {
    use super::*;

    fn base() -> FormSnap {
        FormSnap {
            name: "A".into(),
            dialect: "oracle".into(),
            host: "h".into(),
            port: "1521".into(),
            database: "db".into(),
            schema: "".into(),
            user: "u".into(),
            password: "p".into(),
            save_pw: true,
        }
    }

    /// MC/DC — 칸 하나만 바꾸면 그 칸만 나온다(9칸 독립) · 같으면 빈 목록.
    #[test]
    fn each_field_independently() {
        let b = base();
        assert!(dirty_fields(&b, &b).is_empty());
        type Mut = fn(&mut FormSnap);
        let cases: [(Mut, Dirty); 9] = [
            (|s| s.name = "B".into(), Dirty::Name),
            (|s| s.dialect = "pg".into(), Dirty::Dialect),
            (|s| s.host = "x".into(), Dirty::Host),
            (|s| s.port = "1".into(), Dirty::Port),
            (|s| s.database = "y".into(), Dirty::Database),
            (|s| s.schema = "S".into(), Dirty::Schema),
            (|s| s.user = "v".into(), Dirty::User),
            (|s| s.password = "q".into(), Dirty::Password),
            (|s| s.save_pw = false, Dirty::SavePw),
        ];
        for (f, want) in cases {
            let mut n = base();
            f(&mut n);
            assert_eq!(dirty_fields(&b, &n), vec![want]);
        }
    }

    /// 필수 칸 판정 MC/DC(22 §10): 방언 × 동작 × 칸별 빈 값 — 각 조건이 결과를 독립적으로 바꾼다.
    #[test]
    fn required_fields_by_dialect_and_action() {
        let full = base();
        // 다 채웠으면 어느 방언·동작이든 빠진 것 없음.
        for d in [
            Dialect::Oracle,
            Dialect::Postgres,
            Dialect::Mssql,
            Dialect::Sqlite,
        ] {
            assert!(
                missing_fields(d, FormAction::Connect, &full).is_empty(),
                "{d:?}"
            );
        }
        // Oracle: Database(서비스) 필수 · PG/MSSQL: 선택.
        let mut no_db = base();
        no_db.database.clear();
        assert_eq!(
            missing_fields(Dialect::Oracle, FormAction::Connect, &no_db),
            vec![Field::Database]
        );
        assert!(missing_fields(Dialect::Postgres, FormAction::Connect, &no_db).is_empty());
        assert!(missing_fields(Dialect::Mssql, FormAction::Connect, &no_db).is_empty());
        // SQLite: 파일(Database)만 필수 — 호스트·사용자·비밀번호 없어도 됨.
        let file_only = FormSnap {
            database: "/tmp/a.db".into(),
            ..Default::default()
        };
        assert!(missing_fields(Dialect::Sqlite, FormAction::Connect, &file_only).is_empty());
        assert_eq!(
            missing_fields(Dialect::Sqlite, FormAction::Connect, &FormSnap::default()),
            vec![Field::Database]
        );
        // Password: Connect엔 필수 · Save엔 아님 · Save는 이름이 필수.
        let mut no_pw = base();
        no_pw.password.clear();
        assert_eq!(
            missing_fields(Dialect::Oracle, FormAction::Connect, &no_pw),
            vec![Field::Password]
        );
        assert!(missing_fields(Dialect::Oracle, FormAction::Save, &no_pw).is_empty());
        let mut no_name = base();
        no_name.name.clear();
        assert!(missing_fields(Dialect::Oracle, FormAction::Connect, &no_name).is_empty());
        assert_eq!(
            missing_fields(Dialect::Oracle, FormAction::Save, &no_name),
            vec![Field::Name]
        );
        // Port·Schema는 늘 선택 · 여러 칸이면 폼 순서(Name · Host · … · Password).
        let mut sparse = base();
        sparse.port.clear();
        sparse.schema.clear();
        sparse.host = " ".into();
        sparse.user.clear();
        assert_eq!(
            missing_fields(Dialect::Postgres, FormAction::Connect, &sparse),
            vec![Field::Host, Field::User]
        );
    }

    /// 양끝 공백은 바뀜이 아니다 — 비밀번호만 공백도 값.
    #[test]
    fn trims_except_password() {
        let b = base();
        let mut n = base();
        n.host = " h ".into();
        n.name = "A ".into();
        assert!(dirty_fields(&b, &n).is_empty());
        n.password = "p ".into();
        assert_eq!(dirty_fields(&b, &n), vec![Dirty::Password]);
    }

    /// 여러 칸이면 폼 순서대로 전부.
    #[test]
    fn multiple_in_form_order() {
        let b = base();
        let mut n = base();
        n.user = "w".into();
        n.host = "z".into();
        assert_eq!(dirty_fields(&b, &n), vec![Dirty::Host, Dirty::User]);
    }
}
