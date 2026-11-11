//! ★ AI 좌측 패널(docs/110 §14-2 · 사용자 11-11 재설계 "공간을 아껴 — 작은 아이콘 + 툴팁 · 선택은 큰 팝업으로"): 한 패널이
//! 활동 막대의 **Claude 탭과 Codex 탭을 각각** 받는다(제공자 = 탭 · 호스트가 [`PanelState::provider`]로 넣는다 · 세션 목록도 그 제공자만).
//! 위 = 제목 + 아이콘 버튼(새 대화 · 프로필 · API/CLI 칩(Claude) · Auto 스위치 · 툴팁) · 칩 두 개(프로필 · 모델+노력도 → [`Picker`] 큰
//! 팝업) · 세션 검색 + 목록(열기/이어하기 · 이름 바꾸기 · 삭제 · 우클릭 메뉴) · 아래 = 사용량 한 줄. 동작은 [`PanelAction`]으로 돌려준다.

use super::picker::{Extra, PickEvent, PickItem, Picker};
use super::{Mode, Provider, Usage, EFFORTS};
use nexa_ctl::controls::ctxmenu::MenuIcon;
use nexa_ctl::controls::{ContextMenu, CtxItem};
use nexa_ctl::draw::{draw_tooltip_in, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{Color, IconImage, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SessionRow {
    pub id: String,
    pub title: String,
    pub mode: Mode,
    pub model: String,
    /// 저장된 턴 수(사용자 글 수).
    pub turns: usize,
}

/// 프로필 선택 팝업 한 행(이름 · 설명 = 모델 · 키 상태).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ProfileRow {
    pub name: String,
    pub desc: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelState {
    /// 이 패널이 지금 보여 주는 탭(활동 막대 Claude/Codex).
    pub provider: Provider,
    pub mode: Mode,
    pub profiles: Vec<ProfileRow>,
    pub profile: String,
    pub model: String,
    /// 모델 선택지(id · 설명) — 프로필이 조회한 목록 또는 제공자 내장 표 · 현재 모델 포함.
    pub models: Vec<(String, String)>,
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
    SetMode(Mode),
    SetProfile(String),
    SetModel(String),
    SetEffort(String),
    SetAuto(bool),
    NewChat,
    ManageProfiles,
    /// 세션 1클릭 = 미리보기 탭 · 더블클릭 = 고정 탭(사용자 11-12 · 편집기 탭과 같은 기준).
    OpenSession(String),
    OpenSessionPinned(String),
    RenameSession(String, String),
    DeleteSession(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickKind {
    Profile,
    Model,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    NewChat,
    Profiles,
    ModeChip,
    Auto,
    ProfileChip,
    ModelChip,
    Session(usize),
    None,
}

impl Hit {
    /// 툴팁 글(아이콘 버튼·칩만).
    fn tip(self, st: &PanelState) -> Option<String> {
        Some(match self {
            Hit::NewChat => t(Msg::AiNewChat).to_string(),
            Hit::Profiles => t(Msg::AiProfiles).to_string(),
            Hit::ModeChip => tf(
                if st.provider == Provider::Codex {
                    Msg::AiTipModeCodex
                } else {
                    Msg::AiTipMode
                },
                &[t(if st.mode == Mode::Api {
                    Msg::AiModeApi
                } else {
                    Msg::AiModeCli
                })],
            ),
            Hit::Auto => t(Msg::AiTipAuto).to_string(),
            Hit::ProfileChip => tf(Msg::AiTipProfile, &[&st.profile]),
            Hit::ModelChip => tf(Msg::AiTipModel, &[&st.model, &st.effort]),
            _ => return None,
        })
    }
}

/// 작은 아이콘 버튼(16px 틴트 아이콘 · hover 오버레이 · 눌림) — 아이콘·색별 틴트 캐시.
struct IconBtn {
    icon: fn() -> MenuIcon,
    tint: RefCell<Option<(Color, Rc<IconImage>)>>,
}

impl IconBtn {
    fn new(icon: fn() -> MenuIcon) -> IconBtn {
        IconBtn {
            icon,
            tint: RefCell::new(None),
        }
    }
    fn tinted(&self, fg: Color) -> Rc<IconImage> {
        if let Some((c, img)) = self.tint.borrow().as_ref() {
            if *c == fg {
                return img.clone();
            }
        }
        let (r, g, b) = fg.rgb();
        let ic = (self.icon)();
        let img = Rc::new(IconImage::from_alpha_tinted(
            ic.w,
            ic.h,
            &ic.alpha,
            (r, g, b),
        ));
        *self.tint.borrow_mut() = Some((fg, img.clone()));
        img
    }
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
    /// 세션 행 더블클릭 판정(id · 시각) · 간격(ms · 설정 `ui.dblclick_ms`).
    last_session_click: Option<(String, std::time::Instant)>,
    dblclick_ms: u128,
    /// 그릴 때 잡은 자리.
    r_new: Rect,
    r_profiles: Rect,
    r_mode: Rect,
    r_auto: Rect,
    r_profile: Rect,
    r_model: Rect,
    list_rect: Rect,
    row_h: i32,
    lh: i32,
    scroll_y: i32,
    bars: ScrollBars,
    picker: Option<(PickKind, Picker)>,
    menu: ContextMenu,
    menu_session: Option<String>,
    /// 세션 검색 상자(제목 부분 일치).
    filter: TextBox,
    filtered: Vec<usize>,
    /// 이름 바꾸기 중인 세션 id + 입력 상자.
    rename: Option<(String, TextBox)>,
    ib_new: IconBtn,
    ib_profiles: IconBtn,
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
            last_session_click: None,
            dblclick_ms: 400,
            r_new: Rect::default(),
            r_profiles: Rect::default(),
            r_mode: Rect::default(),
            r_auto: Rect::default(),
            r_profile: Rect::default(),
            r_model: Rect::default(),
            list_rect: Rect::default(),
            row_h: 24,
            lh: 16,
            scroll_y: 0,
            bars: ScrollBars::new(),
            picker: None,
            menu: ContextMenu::new(),
            menu_session: None,
            filter,
            filtered: Vec::new(),
            rename: None,
            ib_new: IconBtn::new(crate::toolicons::mi_add),
            ib_profiles: IconBtn::new(crate::toolicons::mi_key),
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }
    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.picker = None;
            self.menu.close();
            self.rename = None;
            self.hover = Hit::None;
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
    pub(crate) fn set_dblclick_ms(&mut self, ms: u128) {
        self.dblclick_ms = ms.max(1);
    }

    pub(crate) fn take_actions(&mut self) -> Vec<PanelAction> {
        std::mem::take(&mut self.actions)
    }
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open() || self.picker.is_some()
    }
    pub(crate) fn menu_bounds(&self) -> Rect {
        if let Some((_, p)) = &self.picker {
            return p.bounds();
        }
        self.menu.bounds()
    }
    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
        self.picker = None;
    }
    pub(crate) fn set_state(&mut self, st: PanelState) {
        if st != self.st {
            // 열린 팝업의 꼬리 행(노력도 · Auto)은 새 값으로.
            if let Some((PickKind::Model, p)) = &mut self.picker {
                p.set_extra(0, effort_extra(&st.effort));
                p.set_extra(
                    1,
                    Extra::Toggle {
                        label: t(Msg::AiAuto).to_string(),
                        on: st.auto,
                    },
                );
            }
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
        if self.r_new.contains(p) {
            Hit::NewChat
        } else if self.r_profiles.contains(p) {
            Hit::Profiles
        } else if self.r_mode.contains(p) {
            Hit::ModeChip
        } else if self.r_auto.contains(p) {
            Hit::Auto
        } else if self.r_profile.contains(p) {
            Hit::ProfileChip
        } else if self.r_model.contains(p) {
            Hit::ModelChip
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

    fn host_rect(&self, anchor: Rect) -> Rect {
        Rect::new(0, 0, self.clamp_w.max(anchor.right()), i32::MAX / 4)
    }

    fn open_picker(&mut self, kind: PickKind, anchor: Rect) {
        let (title, items, selected, extras) = match kind {
            PickKind::Profile => {
                let items: Vec<PickItem> = self
                    .st
                    .profiles
                    .iter()
                    .map(|p| PickItem {
                        id: p.name.clone(),
                        title: p.name.clone(),
                        desc: p.desc.clone(),
                    })
                    .collect();
                if items.is_empty() {
                    // 프로필이 없으면 관리 화면으로.
                    self.actions.push(PanelAction::ManageProfiles);
                    return;
                }
                let sel = items.iter().position(|i| i.id == self.st.profile);
                (t(Msg::AiPickProfile), items, sel, Vec::new())
            }
            PickKind::Model => {
                let items: Vec<PickItem> = self
                    .st
                    .models
                    .iter()
                    .map(|(id, desc)| PickItem {
                        id: id.clone(),
                        title: id.clone(),
                        desc: desc.clone(),
                    })
                    .collect();
                let sel = items.iter().position(|i| i.id == self.st.model);
                (
                    t(Msg::AiPickModel),
                    items,
                    sel,
                    vec![
                        effort_extra(&self.st.effort),
                        Extra::Toggle {
                            label: t(Msg::AiAuto).to_string(),
                            on: self.st.auto,
                        },
                    ],
                )
            }
        };
        let host = self.host_rect(anchor);
        let p = Picker::new(
            title, items, selected, extras, anchor, host, self.lh, self.scale,
        );
        self.picker = Some((kind, p));
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
        // 큰 선택 팝업(바깥 클릭 = 닫고 그대로 진행).
        if let Some((kind, p)) = &mut self.picker {
            let kind = *kind;
            match p.on_event(ev) {
                PickEvent::None => return true,
                PickEvent::Redraw => return true,
                PickEvent::Pick(id) => {
                    self.picker = None;
                    self.actions.push(match kind {
                        PickKind::Profile => PanelAction::SetProfile(id),
                        PickKind::Model => PanelAction::SetModel(id),
                    });
                    return true;
                }
                PickEvent::Effort(i) => {
                    if let Some(e) = EFFORTS.get(i) {
                        self.actions.push(PanelAction::SetEffort((*e).to_string()));
                    }
                    return true;
                }
                PickEvent::Toggle(on) => {
                    self.actions.push(PanelAction::SetAuto(on));
                    return true;
                }
                PickEvent::Close => {
                    self.picker = None;
                    if !matches!(ev, InputEvent::MouseDown { .. }) {
                        return true;
                    }
                    // 바깥 클릭은 아래로 이어진다.
                }
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
                    Hit::NewChat => self.actions.push(PanelAction::NewChat),
                    Hit::Profiles => self.actions.push(PanelAction::ManageProfiles),
                    Hit::ModeChip => self.actions.push(PanelAction::SetMode(match self.st.mode {
                        Mode::Api => Mode::Cli,
                        Mode::Cli => Mode::Api,
                    })),
                    Hit::Auto => self.actions.push(PanelAction::SetAuto(!self.st.auto)),
                    Hit::ProfileChip => self.open_picker(PickKind::Profile, self.r_profile),
                    Hit::ModelChip => self.open_picker(PickKind::Model, self.r_model),
                    Hit::Session(i) => {
                        if let Some(&si) = self.filtered.get(i) {
                            let id = self.st.sessions[si].id.clone();
                            let now = std::time::Instant::now();
                            let dbl = matches!(
                                &self.last_session_click,
                                Some((pid, t0)) if *pid == id
                                    && now.duration_since(*t0).as_millis() < self.dblclick_ms
                            );
                            self.last_session_click =
                                if dbl { None } else { Some((id.clone(), now)) };
                            self.actions.push(if dbl {
                                PanelAction::OpenSessionPinned(id)
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
            _ => false,
        }
    }

    /// 아이콘 버튼 하나(정사각 · hover/눌림 오버레이 · 16px 틴트 아이콘).
    fn paint_icon_btn(&self, dc: &mut dyn DrawCtx, th: &Theme, r: Rect, ib: &IconBtn, hit: Hit) {
        let rad = self.s(4.0);
        if self.hover == hit {
            dc.fill_round_rect_alpha(r, rad, th.text, 0.08);
        }
        if self.pressed == hit {
            dc.fill_round_rect_alpha(r, rad, th.text, 0.12);
        }
        let img = ib.tinted(th.text);
        let icon = self.s(16.0);
        let dst = Rect::new(r.x + (r.w - icon) / 2, r.y + (r.h - icon) / 2, icon, icon);
        dc.image_scaled(dst, &img, r);
    }

    /// 칩(값 + 셰브론 · 누르면 큰 팝업).
    fn paint_chip(&self, dc: &mut dyn DrawCtx, th: &Theme, r: Rect, value: &str, hit: Hit) {
        let rad = self.s(4.0);
        dc.fill_round_rect(r, rad, th.field_bg);
        dc.stroke_round_rect(
            r,
            rad,
            if self.hover == hit {
                th.accent
            } else {
                th.border
            },
            1.0,
        );
        let cw = self.s(10.0);
        let inner = Rect::new(r.x + self.s(6.0), r.y, r.w - cw - self.s(16.0), r.h);
        let shown = nexa_ctl::draw::ellipsize_middle(dc, value, inner.w.max(10));
        let ty = dc.text_center_y(r.y, r.h);
        dc.text(inner.x, ty, inner, &shown, th.text);
        nexa_ctl::controls::draw_chevron_down(
            dc,
            Rect::new(r.right() - cw - self.s(5.0), r.y + (r.h - cw) / 2, cw, cw),
            th.text_dim,
        );
    }

    fn paint_switch(&self, dc: &mut dyn DrawCtx, th: &Theme, r: Rect, on: bool, hit: Hit) {
        let tw = self.s(30.0);
        let thh = self.s(16.0);
        let tr = Rect::new(r.x + (r.w - tw) / 2, r.y + (r.h - thh) / 2, tw, thh);
        dc.fill_round_rect(tr, thh / 2, if on { th.accent } else { th.border });
        let kn = thh - self.s(4.0);
        let kx = if on {
            tr.right() - kn - self.s(2.0)
        } else {
            tr.x + self.s(2.0)
        };
        dc.fill_ellipse(
            Rect::new(kx, tr.y + self.s(2.0), kn, kn),
            nexa_ctl::Color(0x00FF_FFFF),
        );
        if self.hover == hit {
            dc.fill_round_rect_alpha(r, self.s(4.0), th.text, 0.05);
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
        self.lh = lh;
        let row_h = lh + self.s(10.0);
        self.row_h = row_h;
        let codex = self.st.provider == Provider::Codex;
        // ── 머리: 제목(제공자) + 오른쪽 아이콘 버튼들(오른쪽부터 Auto · API/CLI · 프로필 · 새 대화)
        let gap = self.s(4.0);
        let mut bx = x + w;
        self.r_auto = Rect::new(bx - self.s(34.0), y, self.s(34.0), row_h);
        bx = self.r_auto.x - gap;
        // [API|CLI] 칩은 두 제공자 모두(Codex CLI = codex 자체 로그인 · 사용자 11-11 "Codex도 CLI 기능").
        {
            let label = t(if self.st.mode == Mode::Api {
                Msg::AiModeApi
            } else {
                Msg::AiModeCli
            });
            let mw = dc.text_width(label) + self.s(14.0);
            self.r_mode = Rect::new(bx - mw, y, mw, row_h);
            bx = self.r_mode.x - gap;
        }
        self.r_profiles = Rect::new(bx - row_h, y, row_h, row_h);
        bx = self.r_profiles.x - gap;
        self.r_new = Rect::new(bx - row_h, y, row_h, row_h);
        {
            let ty = dc.text_center_y(y, row_h);
            dc.select_font(FontSlot::Base, true);
            let title = self.st.provider.title();
            let title_w = (self.r_new.x - gap - x).max(10);
            let shown = nexa_ctl::draw::ellipsize_middle(dc, title, title_w);
            dc.text(x, ty, clip, &shown, th.text);
            dc.select_font(FontSlot::Base, false);
            if self.st.busy {
                let tw = dc.text_width(&shown);
                dc.fill_ellipse(
                    Rect::new(
                        x + tw + self.s(6.0),
                        ty + lh / 2 - self.s(3.0),
                        self.s(6.0),
                        self.s(6.0),
                    ),
                    th.accent,
                );
            }
            self.paint_icon_btn(dc, th, self.r_new, &self.ib_new, Hit::NewChat);
            self.paint_icon_btn(dc, th, self.r_profiles, &self.ib_profiles, Hit::Profiles);
            {
                let r = self.r_mode;
                let rad = self.s(4.0);
                let api = self.st.mode == Mode::Api;
                dc.fill_round_rect(r, rad, if api { th.panel_bg_alt } else { th.accent });
                dc.stroke_round_rect(
                    r,
                    rad,
                    if self.hover == Hit::ModeChip {
                        th.accent
                    } else {
                        th.border
                    },
                    1.0,
                );
                let label = t(if api { Msg::AiModeApi } else { Msg::AiModeCli });
                let tw = dc.text_width(label);
                dc.text(
                    r.x + (r.w - tw) / 2,
                    ty,
                    r,
                    label,
                    if api {
                        th.text
                    } else {
                        nexa_ctl::Color(0x00FF_FFFF)
                    },
                );
            }
            self.paint_switch(dc, th, self.r_auto, self.st.auto, Hit::Auto);
        }
        y += row_h + self.s(6.0);
        // ── 칩 행: [프로필 ▾][모델 · 노력도 ▾] (CLI 모드는 프로필 칩 없음 — CLI 자체 로그인)
        let show_profile = self.st.mode == Mode::Api;
        if show_profile {
            let half = (w - gap) / 2;
            self.r_profile = Rect::new(x, y, half, row_h);
            self.r_model = Rect::new(x + half + gap, y, w - half - gap, row_h);
            let pv = if self.st.profile.is_empty() {
                t(Msg::AiNoProfileShort).to_string()
            } else {
                self.st.profile.clone()
            };
            self.paint_chip(dc, th, self.r_profile, &pv, Hit::ProfileChip);
        } else {
            self.r_profile = Rect::default();
            self.r_model = Rect::new(x, y, w, row_h);
        }
        // 모델 칩 글: `claude-` 접두는 떼고 · 자리가 모자라면 노력도를 생략(협업 V1 관찰 "claude-…edium" · 11-11).
        let short = self
            .st
            .model
            .strip_prefix("claude-")
            .unwrap_or(&self.st.model);
        let inner_w = self.r_model.w - self.s(10.0) - self.s(16.0);
        let full = format!("{short} · {}", self.st.effort);
        let mv = if dc.text_width(&full) <= inner_w {
            full
        } else {
            short.to_string()
        };
        self.paint_chip(dc, th, self.r_model, &mv, Hit::ModelChip);
        y += row_h + self.s(8.0);
        // ── 아래 사용량 줄(먼저 자리를 잡아 목록 높이를 정한다)
        // 바닥 = 사용량 1줄 + CLI 모드면 경로·로그인 상태를 **줄마다**(협업 V1 관찰 "한 줄 가운데 생략" · 11-11).
        let mut foot_lines = 1;
        if self.st.mode == Mode::Cli {
            foot_lines += self
                .st
                .cli_note
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count() as i32;
        }
        let foot_h = foot_lines * lh + self.s(10.0);
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
        let list_bottom = b.bottom() - foot_h - pad;
        self.list_rect = Rect::new(b.x, y, b.w - 1, (list_bottom - y).max(0));
        self.clamp_scroll();
        let lr = self.list_rect;
        if self.filtered.is_empty() {
            let ty = dc.text_center_y(lr.y, row_h);
            dc.text(lr.x + pad, ty, lr, t(Msg::AiNoSessions), th.text_dim);
        } else {
            let first = (self.scroll_y / row_h) as usize;
            let mut ry = lr.y - self.scroll_y % row_h;
            for i in first..self.filtered.len() {
                if ry >= lr.bottom() {
                    break;
                }
                let s = &self.st.sessions[self.filtered[i]];
                let r = Rect::new(lr.x, ry, lr.w, row_h);
                let rc = Rect::new(
                    r.x,
                    r.y.max(lr.y),
                    r.w,
                    (r.bottom().min(lr.bottom()) - r.y.max(lr.y)).max(0),
                );
                let cur = s.id == self.st.current;
                if cur {
                    dc.fill_rect(
                        rc,
                        if self.focused {
                            th.sel_bg
                        } else {
                            th.sel_bg_inactive
                        },
                    );
                } else if self.hover == Hit::Session(i) {
                    dc.fill_rect_alpha(rc, th.text, 0.06);
                }
                let ty = dc.text_center_y(ry, row_h);
                let tag = match s.mode {
                    Mode::Api => "API",
                    Mode::Cli => "CLI",
                };
                let tag_w = if codex {
                    0
                } else {
                    dc.text(lr.x + pad, ty, rc, tag, th.text_dim);
                    dc.text_width("API") + self.s(8.0)
                };
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
                    dc.text(lr.x + pad + tag_w, ty, rc, &shown, th.text);
                    let turns = s.turns.to_string();
                    let tw = dc.text_width(&turns);
                    dc.text(lr.right() - pad - tw, ty, rc, &turns, th.text_dim);
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
        // ── 바닥: 사용량(작게 · 흐리게) + CLI 경로
        let mut fy = list_bottom + self.s(6.0);
        dc.fill_rect(Rect::new(x, fy - self.s(4.0), w, 1), th.border);
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
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &line, w);
            dc.text(x, fy, clip, &shown, th.text_dim);
            fy += lh;
            if foot_lines > 1 {
                for l in self.st.cli_note.lines().filter(|l| !l.trim().is_empty()) {
                    let shown = nexa_ctl::draw::ellipsize_middle(dc, l, w);
                    dc.text(x, fy, clip, &shown, th.text_dim);
                    fy += lh;
                }
            }
        }
    }

    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        if let Some((_, p)) = &self.picker {
            p.paint(dc, th);
        }
        if self.menu.is_open() {
            self.menu.paint(dc, th);
        }
        self.filter.paint_popup(dc, th);
        // 툴팁(아이콘 버튼·칩 · 도구줄 선례처럼 hover 즉시 · 팝업이 열려 있으면 없음).
        if self.picker.is_none() && !self.menu.is_open() && self.pressed == Hit::None {
            if let Some(tip) = self.hover.tip(&self.st) {
                let r = match self.hover {
                    Hit::NewChat => self.r_new,
                    Hit::Profiles => self.r_profiles,
                    Hit::ModeChip => self.r_mode,
                    Hit::Auto => self.r_auto,
                    Hit::ProfileChip => self.r_profile,
                    _ => self.r_model,
                };
                let clamp = (self.bounds.x, self.clamp_w.max(self.bounds.right()));
                draw_tooltip_in(dc, th, r, clamp, &tip, self.scale);
            }
        }
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
        s.push_str(&format!(
            "models={}\n",
            st.models
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
        s.push_str(&format!(
            "profiles={}\n",
            st.profiles
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
        if let Some((k, _)) = &self.picker {
            s.push_str(&format!("picker={k:?}\n"));
        }
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

/// 노력도 꼬리 행(현재 = EFFORTS 위치 · 모르는 값이면 medium 자리).
fn effort_extra(effort: &str) -> Extra {
    let idx = EFFORTS.iter().position(|e| *e == effort).unwrap_or(1);
    Extra::Effort {
        label: tf(Msg::AiEffortLabel, &[effort]),
        idx,
        n: EFFORTS.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_hit_rows_and_pickers() {
        let mut p = AiPanel::new();
        p.set_visible(true);
        p.set_bounds(Rect::new(0, 0, 240, 600), 1.0);
        p.set_clamp_width(800);
        p.set_state(PanelState {
            models: vec![
                ("claude-opus-5-5".into(), "best".into()),
                ("claude-haiku-5-5".into(), "fast".into()),
            ],
            model: "claude-haiku-5-5".into(),
            effort: "high".into(),
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
        // 모델 칩 → 큰 팝업(현재 = haiku · 노력도 high = 셋째 점) → 첫 행 선택.
        p.r_model = Rect::new(8, 40, 220, 24);
        p.on_event(&InputEvent::MouseDown {
            x: 20,
            y: 50,
            shift: false,
            primary: false,
        });
        p.on_event(&InputEvent::MouseUp { x: 20, y: 50 });
        assert!(p.menu_open() && p.dump_text().contains("picker=Model"));
        let pr = p.menu_bounds();
        p.on_event(&InputEvent::MouseDown {
            x: pr.x + 20,
            y: pr.y + 26 + 10,
            shift: false,
            primary: false,
        });
        assert_eq!(
            p.take_actions(),
            vec![PanelAction::SetModel("claude-opus-5-5".into())]
        );
        assert!(!p.menu_open());
        // 프로필이 없으면 프로필 칩 = 관리 화면으로.
        p.r_profile = Rect::new(8, 40, 100, 24);
        p.on_event(&InputEvent::MouseDown {
            x: 20,
            y: 50,
            shift: false,
            primary: false,
        });
        p.on_event(&InputEvent::MouseUp { x: 20, y: 50 });
        assert_eq!(p.take_actions(), vec![PanelAction::ManageProfiles]);
        // 툴팁 글.
        assert!(Hit::ModelChip
            .tip(&p.st)
            .is_some_and(|s| s.contains("claude-haiku-5-5")));
        assert!(Hit::Session(0).tip(&p.st).is_none());
    }
}
