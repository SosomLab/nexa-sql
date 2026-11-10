//! ★ Claude 좌측 패널(docs/110 §14-2 · 사용자 11-11 "좌측 Claude 탭 · 상단 API/CLI 선택"): 모드 분할 버튼 · 프로필/모델/노력도
//! 드롭 · Auto 스위치 · [새 대화][프로필 관리] · **세션 목록**(사용자 11-11 "이전 기록을 활용하는 세션 관리" — 검색 · 열기/이어하기 ·
//! 이름 바꾸기 · 삭제 · 우클릭 메뉴) · 사용량 한 줄. 상태는 호스트가 [`PanelState`]로 넣고 동작은 [`PanelAction`]으로 돌려준다.

use super::{Mode, Provider, Usage, EFFORTS};
use nexa_ctl::controls::{ContextMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SessionRow {
    pub id: String,
    pub title: String,
    pub mode: Mode,
    pub model: String,
    /// 저장된 턴 수(사용자 글 수).
    pub turns: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelState {
    pub provider: Provider,
    pub mode: Mode,
    pub profiles: Vec<String>,
    pub profile: String,
    pub model: String,
    /// 모델 선택지(프로필이 조회한 목록 또는 제공자 내장 표 · 현재 모델 포함).
    pub models: Vec<String>,
    pub effort: String,
    pub auto: bool,
    pub sessions: Vec<SessionRow>,
    pub current: String,
    pub usage: Usage,
    pub busy: bool,
    /// CLI 실행 파일 상태 글(찾음/못 찾음).
    pub cli_note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PanelAction {
    SetProvider(Provider),
    SetMode(Mode),
    SetProfile(String),
    SetModel(String),
    SetEffort(String),
    SetAuto(bool),
    NewChat,
    ManageProfiles,
    OpenSession(String),
    RenameSession(String, String),
    DeleteSession(String),
    /// 채팅 탭을 앞으로.
    ShowChat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropKind {
    Profile,
    Model,
    Effort,
}

struct Drop {
    kind: DropKind,
    items: Vec<String>,
    rect: Rect,
    hover: Option<usize>,
    row_h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    ProvClaude,
    ProvCodex,
    ModeApi,
    ModeCli,
    Profile,
    Model,
    Effort,
    Auto,
    NewChat,
    Profiles,
    Session(usize),
    None,
}

const PAD: f32 = 8.0;

pub(crate) struct AiPanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    clamp_w: i32,
    focused: bool,
    st: PanelState,
    actions: Vec<PanelAction>,
    hover: Hit,
    pressed: Hit,
    /// 그릴 때 잡은 자리.
    r_prov_claude: Rect,
    r_prov_codex: Rect,
    r_mode_api: Rect,
    r_mode_cli: Rect,
    r_profile: Rect,
    r_model: Rect,
    r_effort: Rect,
    r_auto: Rect,
    r_new: Rect,
    r_profiles: Rect,
    list_rect: Rect,
    row_h: i32,
    scroll_y: i32,
    bars: ScrollBars,
    drop: Option<Drop>,
    menu: ContextMenu,
    menu_session: Option<String>,
    /// 세션 검색 상자(제목 부분 일치).
    filter: TextBox,
    filtered: Vec<usize>,
    /// 이름 바꾸기 중인 세션 id + 입력 상자.
    rename: Option<(String, TextBox)>,
}

impl AiPanel {
    pub(crate) fn new() -> Self {
        let mut filter = TextBox::new(t(Msg::AiSearchSessions));
        filter.set_focus_ring(false);
        AiPanel {
            visible: false,
            bounds: Rect::default(),
            scale: 1.0,
            clamp_w: 0,
            focused: false,
            st: PanelState::default(),
            actions: Vec::new(),
            hover: Hit::None,
            pressed: Hit::None,
            r_prov_claude: Rect::default(),
            r_prov_codex: Rect::default(),
            r_mode_api: Rect::default(),
            r_mode_cli: Rect::default(),
            r_profile: Rect::default(),
            r_model: Rect::default(),
            r_effort: Rect::default(),
            r_auto: Rect::default(),
            r_new: Rect::default(),
            r_profiles: Rect::default(),
            list_rect: Rect::default(),
            row_h: 24,
            scroll_y: 0,
            bars: ScrollBars::new(),
            drop: None,
            menu: ContextMenu::new(),
            menu_session: None,
            filter,
            filtered: Vec::new(),
            rename: None,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }
    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.drop = None;
            self.menu.close();
            self.rename = None;
        }
    }
    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }
    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        if (scale - self.scale).abs() > f32::EPSILON {
            self.filter.set_scale(scale);
            if let Some((_, tb)) = &mut self.rename {
                tb.set_scale(scale);
            }
        }
        self.scale = scale;
    }
    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
    }
    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if !on {
            self.filter.set_focused(false);
        }
    }
    pub(crate) fn wants_ime(&self) -> bool {
        self.filter.is_focused() || self.rename.as_ref().is_some_and(|(_, tb)| tb.is_focused())
    }
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if let Some((_, tb)) = &mut self.rename {
            if tb.is_focused() {
                return Some(tb);
            }
        }
        self.filter.is_focused().then_some(&mut self.filter)
    }
    pub(crate) fn take_actions(&mut self) -> Vec<PanelAction> {
        std::mem::take(&mut self.actions)
    }
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open() || self.drop.is_some()
    }
    pub(crate) fn menu_bounds(&self) -> Rect {
        if let Some(d) = &self.drop {
            return d.rect;
        }
        self.menu.bounds()
    }
    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
        self.drop = None;
    }
    pub(crate) fn set_state(&mut self, st: PanelState) {
        if st != self.st {
            self.st = st;
            self.refilter();
        }
    }

    fn refilter(&mut self) {
        let q = self.filter.text().to_lowercase();
        self.filtered = self
            .st
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| q.is_empty() || s.title.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.clamp_scroll();
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn content_h(&self) -> i32 {
        self.filtered.len() as i32 * self.row_h
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    fn hit(&self, p: Point) -> Hit {
        if self.r_prov_claude.contains(p) {
            Hit::ProvClaude
        } else if self.r_prov_codex.contains(p) {
            Hit::ProvCodex
        } else if self.r_mode_api.contains(p) {
            Hit::ModeApi
        } else if self.r_mode_cli.contains(p) {
            Hit::ModeCli
        } else if self.r_profile.contains(p) {
            Hit::Profile
        } else if self.r_model.contains(p) {
            Hit::Model
        } else if self.r_effort.contains(p) {
            Hit::Effort
        } else if self.r_auto.contains(p) {
            Hit::Auto
        } else if self.r_new.contains(p) {
            Hit::NewChat
        } else if self.r_profiles.contains(p) {
            Hit::Profiles
        } else if self.list_rect.contains(p) {
            let i = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h.max(1)) as usize;
            if i < self.filtered.len() {
                Hit::Session(i)
            } else {
                Hit::None
            }
        } else {
            Hit::None
        }
    }

    fn open_drop(&mut self, kind: DropKind, anchor: Rect) {
        let items: Vec<String> = match kind {
            DropKind::Profile => self.st.profiles.clone(),
            DropKind::Model => self.st.models.clone(),
            DropKind::Effort => EFFORTS.iter().map(|m| (*m).to_string()).collect(),
        };
        if items.is_empty() {
            return;
        }
        let row_h = self.row_h;
        let h = row_h * items.len() as i32 + self.s(8.0);
        let host = Rect::new(0, 0, self.clamp_w.max(anchor.right()), i32::MAX / 4);
        let at = nexa_ctl::geom::place_popup(
            Point {
                x: anchor.x,
                y: anchor.bottom(),
            },
            (anchor.w, h),
            host,
        );
        self.drop = Some(Drop {
            kind,
            items,
            rect: Rect::new(at.x, at.y, anchor.w, h),
            hover: None,
            row_h,
        });
    }

    fn pick_drop(&mut self, i: usize) {
        let Some(d) = self.drop.take() else { return };
        let Some(v) = d.items.get(i).cloned() else {
            return;
        };
        self.actions.push(match d.kind {
            DropKind::Profile => PanelAction::SetProfile(v),
            DropKind::Model => PanelAction::SetModel(v),
            DropKind::Effort => PanelAction::SetEffort(v),
        });
    }

    fn open_menu(&mut self, p: Point, row: usize) {
        let Some(&si) = self.filtered.get(row) else {
            return;
        };
        let Some(sess) = self.st.sessions.get(si) else {
            return;
        };
        self.menu_session = Some(sess.id.clone());
        let items = vec![
            CtxItem::item("open", t(Msg::AiSessOpen)),
            CtxItem::item("rename", t(Msg::AiSessRename)),
            CtxItem::Separator,
            CtxItem::item("delete", t(Msg::AiSessDelete)),
        ];
        let text_w = (self.row_h * 8).max(160);
        let host = Rect::new(0, 0, self.clamp_w, i32::MAX / 4);
        crate::menu_icons::open_at(&mut self.menu, p.x, p.y, items, host, text_w);
    }

    fn menu_pick(&mut self, id: &str) {
        let Some(sid) = self.menu_session.take() else {
            return;
        };
        match id {
            "open" => self.actions.push(PanelAction::OpenSession(sid)),
            "delete" => self.actions.push(PanelAction::DeleteSession(sid)),
            "rename" => {
                let title = self
                    .st
                    .sessions
                    .iter()
                    .find(|s| s.id == sid)
                    .map(|s| s.title.clone())
                    .unwrap_or_default();
                let mut tb = TextBox::new("");
                tb.set_scale(self.scale);
                tb.set_text(&title);
                tb.set_focused(true);
                self.filter.set_focused(false);
                self.rename = Some((sid, tb));
            }
            _ => {}
        }
    }

    fn finish_rename(&mut self, commit: bool) {
        if let Some((id, tb)) = self.rename.take() {
            let title = tb.text().trim().to_string();
            if commit && !title.is_empty() {
                self.actions.push(PanelAction::RenameSession(id, title));
            }
        }
    }

    /// 사건 — 다시 그릴 일이 있으면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        // 우클릭 메뉴가 먼저.
        if self.menu.is_open() {
            let outside = self.menu.is_outside_click(ev);
            let consumed = self.menu.on_event(ev) && !outside;
            if let Some(id) = self.menu.take_picked() {
                self.menu_pick(&id);
                return true;
            }
            if consumed {
                return true;
            }
            if matches!(ev, InputEvent::MouseMove { .. }) {
                return false;
            }
        }
        // 드롭 목록이 열려 있으면 그것이 먼저(바깥 클릭 = 닫고 그대로 진행).
        if let Some(d) = &mut self.drop {
            match *ev {
                InputEvent::MouseMove { x, y } => {
                    let p = Point { x, y };
                    let h = d
                        .rect
                        .contains(p)
                        .then(|| ((y - d.rect.y - 4) / d.row_h.max(1)) as usize)
                        .filter(|&i| i < d.items.len());
                    let changed = h != d.hover;
                    d.hover = h;
                    return changed;
                }
                InputEvent::MouseDown { x, y, .. } => {
                    let p = Point { x, y };
                    if d.rect.contains(p) {
                        let i = ((y - d.rect.y - 4) / d.row_h.max(1)) as usize;
                        self.pick_drop(i);
                        return true;
                    }
                    self.drop = None;
                    // 바깥 클릭은 아래로 이어진다.
                }
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                } => {
                    self.drop = None;
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Enter, ..
                } => {
                    if let Some(i) = d.hover {
                        self.pick_drop(i);
                    }
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Down, ..
                } => {
                    d.hover = Some(d.hover.map_or(0, |h| (h + 1).min(d.items.len() - 1)));
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Up, ..
                } => {
                    d.hover = Some(d.hover.map_or(0, |h| h.saturating_sub(1)));
                    return true;
                }
                _ => return true,
            }
        }
        // 이름 바꾸기 상자.
        if let Some((_, tb)) = &mut self.rename {
            match *ev {
                InputEvent::Key {
                    key: CtlKey::Enter, ..
                } => {
                    self.finish_rename(true);
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                } => {
                    self.finish_rename(false);
                    return true;
                }
                InputEvent::MouseDown { x, y, .. } => {
                    if !tb.bounds().contains(Point { x, y }) {
                        self.finish_rename(true);
                    } else {
                        tb.on_event(ev, &mut inv);
                        return true;
                    }
                }
                InputEvent::Key { .. } | InputEvent::Char { .. } | InputEvent::SelectAll => {
                    tb.on_event(ev, &mut inv);
                    return true;
                }
                _ => {
                    tb.on_event(ev, &mut inv);
                }
            }
        }
        // 검색 상자(포커스일 때 키 · 안의 마우스).
        let in_filter = match *ev {
            InputEvent::MouseDown { x, y, .. } | InputEvent::MouseMove { x, y } => {
                self.filter.bounds().contains(Point { x, y })
            }
            _ => false,
        };
        if in_filter
            || (self.filter.is_focused()
                && matches!(
                    ev,
                    InputEvent::Key { .. } | InputEvent::Char { .. } | InputEvent::SelectAll
                ))
        {
            if let InputEvent::MouseDown { .. } = ev {
                self.filter.set_focused(true);
            }
            if let InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } = ev
            {
                self.filter.set_text("");
                self.refilter();
                return true;
            }
            self.filter.on_event(ev, &mut inv);
            if self.filter.take_changed().is_some() {
                self.refilter();
            }
            if !matches!(ev, InputEvent::MouseMove { .. }) {
                return true;
            }
        }
        // 목록 스크롤.
        let (_, ny, consumed) = self.bars.on_event(
            ev,
            self.list_rect,
            self.list_rect.w,
            self.content_h().max(self.list_rect.h),
            0,
            self.scroll_y,
            self.scale,
        );
        if ny != self.scroll_y {
            self.scroll_y = ny;
            self.clamp_scroll();
        }
        if consumed {
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let h = self.hit(Point { x, y });
                let changed = h != self.hover;
                self.hover = h;
                changed
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                self.filter.set_focused(false);
                self.pressed = self.hit(p);
                true
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                let h = self.hit(p);
                let pressed = std::mem::replace(&mut self.pressed, Hit::None);
                if h != pressed {
                    return true;
                }
                match h {
                    Hit::ProvClaude => self
                        .actions
                        .push(PanelAction::SetProvider(Provider::Claude)),
                    Hit::ProvCodex => self.actions.push(PanelAction::SetProvider(Provider::Codex)),
                    Hit::ModeApi => self.actions.push(PanelAction::SetMode(Mode::Api)),
                    Hit::ModeCli => self.actions.push(PanelAction::SetMode(Mode::Cli)),
                    Hit::Profile => self.open_drop(DropKind::Profile, self.r_profile),
                    Hit::Model => self.open_drop(DropKind::Model, self.r_model),
                    Hit::Effort => self.open_drop(DropKind::Effort, self.r_effort),
                    Hit::Auto => self.actions.push(PanelAction::SetAuto(!self.st.auto)),
                    Hit::NewChat => self.actions.push(PanelAction::NewChat),
                    Hit::Profiles => self.actions.push(PanelAction::ManageProfiles),
                    Hit::Session(i) => {
                        if let Some(&si) = self.filtered.get(i) {
                            let id = self.st.sessions[si].id.clone();
                            self.actions.push(if id == self.st.current {
                                PanelAction::ShowChat
                            } else {
                                PanelAction::OpenSession(id)
                            });
                        }
                    }
                    Hit::None => {}
                }
                true
            }
            InputEvent::RightDown { x, y } => {
                let p = Point { x, y };
                if let Hit::Session(i) = self.hit(p) {
                    self.open_menu(p, i);
                }
                true
            }
            InputEvent::Key {
                key: CtlKey::Delete,
                ..
            } => {
                // 선택 개념이 없으니 hover 행 삭제는 메뉴로만 — 키는 무시.
                false
            }
            _ => false,
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let b = self.bounds;
        if b.w <= 0 {
            return;
        }
        let pad = self.s(PAD);
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        let clip = b;
        let x = b.x + pad;
        let w = (b.w - pad * 2 - 1).max(self.s(40.0));
        let mut y = b.y + pad;
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let row_h = lh + self.s(10.0);
        self.row_h = row_h;
        // ── 제목 + 제공자 분할 버튼(Claude | Codex) + Claude면 모드 분할 버튼(API | CLI)
        dc.select_font(FontSlot::Base, true);
        dc.text(x, y, clip, t(Msg::MnAiPanel), th.text);
        dc.select_font(FontSlot::Base, false);
        y += lh + self.s(8.0);
        let half = w / 2;
        self.r_prov_claude = Rect::new(x, y, half, row_h);
        self.r_prov_codex = Rect::new(x + half, y, w - half, row_h);
        let codex = self.st.provider == Provider::Codex;
        if codex {
            self.r_mode_api = Rect::default();
            self.r_mode_cli = Rect::default();
        } else {
            self.r_mode_api = Rect::new(x, y + row_h + self.s(4.0), half, row_h);
            self.r_mode_cli = Rect::new(x + half, y + row_h + self.s(4.0), w - half, row_h);
        }
        let mut segs = vec![
            (
                self.r_prov_claude,
                !codex,
                t(Msg::AiProviderClaude),
                Hit::ProvClaude,
            ),
            (
                self.r_prov_codex,
                codex,
                t(Msg::AiProviderCodex),
                Hit::ProvCodex,
            ),
        ];
        if !codex {
            segs.push((
                self.r_mode_api,
                self.st.mode == Mode::Api,
                t(Msg::AiModeApi),
                Hit::ModeApi,
            ));
            segs.push((
                self.r_mode_cli,
                self.st.mode == Mode::Cli,
                t(Msg::AiModeCli),
                Hit::ModeCli,
            ));
        }
        for (r, on, label, hit) in segs {
            let hot = self.hover == hit;
            if on {
                dc.fill_round_rect(r, self.s(4.0), th.accent);
            } else {
                dc.fill_round_rect(r, self.s(4.0), th.panel_bg_alt);
                if hot {
                    dc.fill_rect_alpha(r, th.text, 0.08);
                }
            }
            dc.stroke_round_rect(r, self.s(4.0), th.border, 1.0);
            let tw = dc.text_width(label);
            let ty = dc.text_center_y(r.y, r.h);
            dc.text(
                r.x + (r.w - tw) / 2,
                ty,
                r,
                label,
                if on {
                    nexa_ctl::Color(0x00FF_FFFF)
                } else {
                    th.text
                },
            );
        }
        y += row_h + self.s(8.0);
        if !codex {
            y += row_h + self.s(4.0);
        }
        // ── 프로필 / 모델 / 노력도 드롭 행
        let label_w = self.s(64.0);
        let mut rows: Vec<(Msg, String, Hit)> = Vec::new();
        if self.st.mode == Mode::Api || codex {
            rows.push((
                Msg::AiProfile,
                if self.st.profile.is_empty() {
                    t(Msg::AiNoProfile).to_string()
                } else {
                    self.st.profile.clone()
                },
                Hit::Profile,
            ));
        }
        rows.push((Msg::AiModel, self.st.model.clone(), Hit::Model));
        rows.push((Msg::AiEffort, self.st.effort.clone(), Hit::Effort));
        self.r_profile = Rect::default();
        for (label, value, hit) in rows {
            let r = Rect::new(x + label_w, y, w - label_w, row_h);
            match hit {
                Hit::Profile => self.r_profile = r,
                Hit::Model => self.r_model = r,
                _ => self.r_effort = r,
            }
            let ty = dc.text_center_y(y, row_h);
            dc.text(x, ty, clip, t(label), th.text_dim);
            dc.fill_round_rect(r, self.s(4.0), th.field_bg);
            dc.stroke_round_rect(
                r,
                self.s(4.0),
                if self.hover == hit {
                    th.accent
                } else {
                    th.border
                },
                1.0,
            );
            let inner = Rect::new(r.x + self.s(6.0), r.y, r.w - self.s(24.0), r.h);
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &value, inner.w);
            dc.text(inner.x, ty, inner, &shown, th.text);
            let cw = self.s(10.0);
            nexa_ctl::controls::draw_chevron_down(
                dc,
                Rect::new(r.right() - cw - self.s(6.0), r.y + (r.h - cw) / 2, cw, cw),
                th.text_dim,
            );
            y += row_h + self.s(4.0);
        }
        // ── Auto 스위치 행
        self.r_auto = Rect::new(x, y, w, row_h);
        {
            let ty = dc.text_center_y(y, row_h);
            dc.text(x, ty, clip, t(Msg::AiAuto), th.text_dim);
            let tw = self.s(30.0);
            let thh = self.s(16.0);
            let tr = Rect::new(x + w - tw, y + (row_h - thh) / 2, tw, thh);
            dc.fill_round_rect(
                tr,
                thh / 2,
                if self.st.auto { th.accent } else { th.border },
            );
            let kn = thh - self.s(4.0);
            let kx = if self.st.auto {
                tr.right() - kn - self.s(2.0)
            } else {
                tr.x + self.s(2.0)
            };
            dc.fill_ellipse(
                Rect::new(kx, tr.y + self.s(2.0), kn, kn),
                nexa_ctl::Color(0x00FF_FFFF),
            );
            if self.hover == Hit::Auto {
                dc.fill_rect_alpha(self.r_auto, th.text, 0.05);
            }
        }
        y += row_h + self.s(6.0);
        // ── 버튼 [새 대화] [프로필 관리]
        let bw = (w - self.s(6.0)) / 2;
        self.r_new = Rect::new(x, y, bw, row_h);
        self.r_profiles = Rect::new(x + bw + self.s(6.0), y, w - bw - self.s(6.0), row_h);
        for (r, label, primary, hit) in [
            (self.r_new, t(Msg::AiNewChat), true, Hit::NewChat),
            (self.r_profiles, t(Msg::AiProfiles), false, Hit::Profiles),
        ] {
            let hot = self.hover == hit;
            let down = self.pressed == hit;
            let st = nexa_ctl::tokens::State::of(false, hot, down, true);
            if primary {
                dc.fill_round_rect(r, self.s(4.0), th.accent);
            } else {
                dc.fill_round_rect(r, self.s(4.0), th.panel_bg_alt);
                dc.stroke_round_rect(r, self.s(4.0), if hot { th.accent } else { th.border }, 1.0);
            }
            dc.state_layer(r, th.text, st);
            let tw = dc.text_width(label);
            let ty = dc.text_center_y(r.y, r.h);
            dc.text(
                r.x + (r.w - tw) / 2,
                ty,
                r,
                label,
                if primary {
                    nexa_ctl::Color(0x00FF_FFFF)
                } else {
                    th.text
                },
            );
        }
        y += row_h + self.s(8.0);
        // ── 사용량
        {
            let u = self.st.usage;
            let line = tf(
                Msg::AiUsageLine,
                &[
                    &u.input.to_string(),
                    &u.output.to_string(),
                    &u.cache_read.to_string(),
                    &format!("{:.4}", u.cost_usd),
                ],
            );
            dc.text(x, y, clip, &line, th.text_dim);
            y += lh + self.s(4.0);
            if self.st.mode == Mode::Cli && !self.st.cli_note.is_empty() {
                dc.text(x, y, clip, &self.st.cli_note, th.text_dim);
                y += lh + self.s(4.0);
            }
        }
        dc.fill_rect(Rect::new(x, y, w, 1), th.border);
        y += self.s(6.0);
        // ── 세션: 제목 + 검색 상자
        dc.select_font(FontSlot::Base, true);
        dc.text(
            x,
            y,
            clip,
            &format!("{} ({})", t(Msg::AiSessions), self.st.sessions.len()),
            th.text,
        );
        dc.select_font(FontSlot::Base, false);
        y += lh + self.s(4.0);
        let fr = Rect::new(x, y, w, row_h);
        let mut inv = Invalidations::default();
        self.filter.set_bounds(fr, &mut inv);
        self.filter.paint(dc, th);
        y += row_h + self.s(4.0);
        self.list_rect = Rect::new(b.x, y, b.w - 1, (b.bottom() - y - pad).max(0));
        self.clamp_scroll();
        let lr = self.list_rect;
        if self.filtered.is_empty() {
            let ty = dc.text_center_y(lr.y, row_h);
            dc.text(lr.x + pad, ty, lr, t(Msg::AiNoSessions), th.text_dim);
            return;
        }
        let first = (self.scroll_y / row_h) as usize;
        let mut ry = lr.y - self.scroll_y % row_h;
        for i in first..self.filtered.len() {
            if ry >= lr.bottom() {
                break;
            }
            let s = &self.st.sessions[self.filtered[i]];
            let r = Rect::new(lr.x, ry, lr.w, row_h);
            let cur = s.id == self.st.current;
            if cur {
                dc.fill_rect(
                    r,
                    if self.focused {
                        th.sel_bg
                    } else {
                        th.sel_bg_inactive
                    },
                );
            } else if self.hover == Hit::Session(i) {
                dc.fill_rect_alpha(r, th.text, 0.06);
            }
            let ty = dc.text_center_y(ry, row_h);
            let tag = match s.mode {
                Mode::Api => "API",
                Mode::Cli => "CLI",
            };
            dc.text(lr.x + pad, ty, lr, tag, th.text_dim);
            let tag_w = dc.text_width("API") + self.s(8.0);
            let is_renaming = self.rename.as_ref().is_some_and(|(id, _)| *id == s.id);
            if is_renaming {
                let s2 = self.s(2.0);
                let s4 = self.s(4.0);
                if let Some((_, tb)) = &mut self.rename {
                    let tr = Rect::new(
                        lr.x + pad + tag_w,
                        ry + s2,
                        lr.w - pad * 2 - tag_w,
                        row_h - s4,
                    );
                    tb.set_bounds(tr, &mut inv);
                    tb.paint(dc, th);
                }
            } else {
                let title_w = lr.w - pad * 2 - tag_w - self.s(30.0);
                let shown = nexa_ctl::draw::ellipsize_middle(dc, &s.title, title_w.max(10));
                dc.text(lr.x + pad + tag_w, ty, lr, &shown, th.text);
                let turns = s.turns.to_string();
                let tw = dc.text_width(&turns);
                dc.text(lr.right() - pad - tw, ty, lr, &turns, th.text_dim);
            }
            ry += row_h;
        }
        self.bars.paint(
            dc,
            th,
            lr,
            lr.w,
            self.content_h().max(lr.h),
            0,
            self.scroll_y,
            self.scale,
        );
    }

    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        if let Some(d) = &self.drop {
            dc.shadow(
                d.rect,
                th.text,
                nexa_ctl::tokens::Elevation::Mid,
                self.s(4.0),
            );
            dc.fill_round_rect(d.rect, self.s(4.0), th.panel_bg);
            dc.stroke_round_rect(d.rect, self.s(4.0), th.border, 1.0);
            dc.select_font(FontSlot::Base, false);
            let mut y = d.rect.y + self.s(4.0);
            let current = match d.kind {
                DropKind::Profile => &self.st.profile,
                DropKind::Model => &self.st.model,
                DropKind::Effort => &self.st.effort,
            };
            for (i, it) in d.items.iter().enumerate() {
                let r = Rect::new(d.rect.x + 1, y, d.rect.w - 2, d.row_h);
                if d.hover == Some(i) {
                    dc.fill_rect(r, th.sel_bg);
                }
                let ty = dc.text_center_y(y, d.row_h);
                dc.text(
                    r.x + self.s(8.0),
                    ty,
                    r,
                    it,
                    if it == current { th.accent } else { th.text },
                );
                y += d.row_h;
            }
        }
        if self.menu.is_open() {
            self.menu.paint(dc, th);
        }
        self.filter.paint_popup(dc, th);
    }

    /// 헤드리스 덤프(V1).
    pub(crate) fn dump_text(&self) -> String {
        let st = &self.st;
        let mut s = format!(
            "visible={} provider={} mode={} profile={} model={} effort={} auto={} busy={} sessions={} current={}\n",
            self.visible,
            st.provider.as_str(),
            st.mode.as_str(),
            st.profile,
            st.model,
            st.effort,
            st.auto,
            st.busy,
            st.sessions.len(),
            st.current
        );
        for r in &st.sessions {
            s.push_str(&format!(
                "  {} {} [{}] turns={}\n",
                r.id,
                r.title,
                r.mode.as_str(),
                r.turns
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_and_hit_rows() {
        let mut p = AiPanel::new();
        p.set_visible(true);
        p.set_bounds(Rect::new(0, 0, 240, 600), 1.0);
        p.set_state(PanelState {
            sessions: vec![
                SessionRow {
                    id: "a".into(),
                    title: "orders report".into(),
                    ..Default::default()
                },
                SessionRow {
                    id: "b".into(),
                    title: "index tuning".into(),
                    ..Default::default()
                },
            ],
            current: "a".into(),
            ..Default::default()
        });
        assert_eq!(p.filtered.len(), 2);
        p.filter.set_text("tun");
        p.refilter();
        assert_eq!(p.filtered, vec![1]);
        p.list_rect = Rect::new(0, 300, 240, 300);
        p.row_h = 24;
        assert_eq!(p.hit(Point { x: 10, y: 310 }), Hit::Session(0));
        assert_eq!(p.hit(Point { x: 10, y: 340 }), Hit::None);
        p.on_event(&InputEvent::MouseDown {
            x: 10,
            y: 310,
            shift: false,
            primary: false,
        });
        p.on_event(&InputEvent::MouseUp { x: 10, y: 310 });
        assert_eq!(p.take_actions(), vec![PanelAction::OpenSession("b".into())]);
        assert!(p.dump_text().contains("sessions=2"));
    }
}
