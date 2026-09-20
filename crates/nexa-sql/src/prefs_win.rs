//! 환경 설정 창(T-39 · 사용자 09-15) — VS Code/DBeaver식: **검색** · 왼쪽 **트리**(그룹 ▸ 카테고리 = `CATEGORY_TREE`) ·
//! 오른쪽 **설정 카드 목록**(스크롤) — 종류별 컨트롤: Bool = 스위치 · Choice/Lang = 콤보 · Int/Text = 입력란(즉시 검증) ·
//! 색(`ui.*_color`) = 입력란 + [선택…](색 설정 창) · 단축키(`key.*`) = 입력란 + [캡처…](단축키 창).
//! 기본값과 다른 카드는 왼쪽 강조 막대 + [초기화]. 값은 바꾸는 즉시 호스트가 저장·반영(`PrefsAction::Changed`).
//! 이 창은 레지스트리 스냅샷(`refresh`)만 가진다 — 설정 파일·적용은 호스트 몫.

use nexa_ctl::controls::{PositionDropdown, Switch};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::HudPos;
use nexa_ctl::{
    Button, Combo, ComboControl, ComboItem, Control, EditCtxAction, InputEvent, Invalidations,
    Key as CtlKey, LabelSide, ScrollBars, TextBox, TreeControl, TreeModel, TreeNode, TreeView,
    Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Lang, Msg};
use nsql_settings::{Entry, SettingKind, Settings, CATEGORY_TREE};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrefsAction {
    None,
    Paint,
    /// 값 변경(호스트가 정규화·저장·반영).
    Changed {
        key: String,
        value: String,
    },
    /// 기본값으로.
    Reset(String),
    /// 색 창 열기(어느 색 키를 고르는가).
    OpenColors(String),
    OpenKeys,
    /// settings.json으로 편집(호스트가 내보내고 열고 감시한다).
    EditJson,
}

const PAD: f32 = 12.0;
const TREE_W: f32 = 230.0;
/// 스플리터 손잡이 폭(논리 px) — 왼쪽(검색+트리)과 카드 사이 · 드래그로 폭 조절 · hover 시 서서히 진해짐(clip과 같은 부품 `IntentFade`).
const SPLIT_W: f32 = 6.0;
const LEFT_MIN: f32 = 160.0;
const RIGHT_MIN: f32 = 320.0;
const SEARCH_H: f32 = 30.0;
const CTL_H: f32 = 28.0;
const CARD_GAP: f32 = 10.0;

enum CardCtl {
    Bool(Switch),
    Choice(Box<Combo>),
    Text(Box<TextBox>),
    /// 3×3 위치 — 이미지 드롭다운(선택 타일 + ▾ · 팝업 = 그리드 · 사용자 09-19). 값은 긴 이름(`bottom_left`) ↔ 컨트롤 코드(`bl`).
    Pos(Box<PositionDropdown>),
}

struct Card {
    entry: &'static Entry,
    value: String,
    modified: bool,
    ctl: CardCtl,
    reset: Button,
    /// 색·단축키 보조 버튼(선택…/캡처…).
    aux: Option<Button>,
    /// 마지막 페인트에서 정해진 카드 사각형(히트 테스트).
    /// 종속 조건 불충족으로 잠김(부모 설정을 먼저 바꿔야 한다 · `DEPENDS`).
    locked: bool,
    rect: Rect,
    /// 검색 모드에서 카테고리 표시.
    show_cat: bool,
    error: Option<String>,
    /// 기본값 글자가 컨트롤 줄에 안 들어가면(긴 URL) 설명 아래 한 줄로(사용자 09-17).
    default_below: bool,
}

/// 스냅샷 한 줄.
#[derive(Clone)]
struct Snap {
    entry: &'static Entry,
    value: String,
    modified: bool,
}

pub(crate) struct PrefsWin {
    window: Option<Rc<Window>>,
    /// 창 기하 기억(`wingeom::Memo`): 기록 위치·크기(같은 모니터일 때만 씀) · 마지막 닫힌 (위치, 크기).
    memo: crate::wingeom::Memo,
    last: Option<((i32, i32), (f64, f64))>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    /// 주 조합키(Ctrl · macOS ⌘) — 클립보드 단축키.
    primary: bool,
    search: TextBox,
    tree: TreeView,
    /// 숨긴 분류(끈/미설치 확장) · 그로부터 만든 보이는 트리(선택 index의 기준).
    hidden: Vec<Msg>,
    vtree: Vec<(Msg, Vec<Msg>)>,
    advanced: Switch,
    json_btn: Button,
    close_btn: Button,
    cards: Vec<Card>,
    /// 카드 영역·스크롤.
    list: Rect,
    scroll: i32,
    content_h: i32,
    bars: ScrollBars,
    /// 스냅샷(전체 · 비노출 포함) — `refresh`로 갱신.
    snap: Vec<Snap>,
    /// 현재 선택(그룹 index, 카테고리 index) · 검색어.
    sel: (usize, Option<usize>),
    last_tree_row: usize,
    query: String,
    /// 왼쪽 열 폭(논리 px) · 드래그 중(시작 x, 시작 폭) · 손잡이 hover 페이드 · 손잡이 사각형.
    left_w: f32,
    split_drag: Option<(i32, f32)>,
    split_fade: IntentFade,
    split_rect: Rect,
}

fn is_color_key(k: &str) -> bool {
    k.ends_with("_color")
}

fn is_key_key(k: &str) -> bool {
    k.starts_with("key.")
}

impl PrefsWin {
    /// 보이는 트리(그룹 ▸ 분류) — `CATEGORY_TREE`에서 숨긴 분류(끈/미설치 확장)를 뺀 것.
    fn visible_tree(hidden: &[Msg]) -> Vec<(Msg, Vec<Msg>)> {
        CATEGORY_TREE
            .iter()
            .map(|(g, cats)| {
                let kept: Vec<Msg> = cats
                    .iter()
                    .copied()
                    .filter(|c| !hidden.contains(c))
                    .collect();
                (*g, kept)
            })
            .filter(|(_, cats)| !cats.is_empty())
            .collect()
    }

    fn build_model(tree: &[(Msg, Vec<Msg>)]) -> TreeModel {
        let mut roots: Vec<TreeNode> = Vec::new();
        for (g, cats) in tree {
            let kids: Vec<TreeNode> = cats.iter().map(|c| TreeNode::leaf(t(*c))).collect();
            roots.push(TreeNode::branch(t(*g), kids));
        }
        let mut model = TreeModel::new(roots);
        for gi in 0..tree.len() {
            model.set_expanded(&[gi], true);
        }
        model
    }

    /// 확장 분류 숨김 갱신(호스트가 확장 켜기/끄기/설치/제거 때) — 트리를 다시 만들고 선택을 보정한다.
    pub(crate) fn set_hidden_categories(&mut self, hidden: Vec<Msg>) {
        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        self.vtree = Self::visible_tree(&self.hidden);
        self.tree = TreeView::new(Self::build_model(&self.vtree));
        if self.sel.0 >= self.vtree.len() {
            self.sel = (0, None);
        } else if let Some(ci) = self.sel.1 {
            if ci >= self.vtree[self.sel.0].1.len() {
                self.sel = (self.sel.0, None);
            }
        }
        if self.window.is_some() {
            self.rebuild_cards();
            self.layout();
            self.redraw();
        }
    }

    pub(crate) fn new() -> Self {
        let vtree = Self::visible_tree(&[]);
        let model = Self::build_model(&vtree);
        PrefsWin {
            hidden: Vec::new(),
            vtree,
            window: None,
            memo: crate::wingeom::Memo::default(),

            last: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            search: TextBox::new(t(Msg::PhSearchSettings)),
            tree: TreeView::new(model),
            advanced: Switch::new(t(Msg::LblAdvanced), false).with_label_side(LabelSide::Left),
            json_btn: Button::new(t(Msg::BtnEditJson)),
            close_btn: Button::new(t(Msg::BtnClose)),
            cards: Vec::new(),
            list: Rect::default(),
            scroll: 0,
            content_h: 0,
            bars: ScrollBars::new(),
            snap: Vec::new(),
            sel: (0, Some(0)),
            last_tree_row: 1,
            query: String::new(),
            left_w: TREE_W,
            split_drag: None,
            split_fade: IntentFade::with_speed(FadeSpeed::Fast),
            split_rect: Rect::default(),
        }
    }

    /// 레지스트리 스냅샷 갱신(열 때 · 값이 바뀔 때). 카드는 값만 갱신(입력 중인 상자는 건드리지 않음).
    pub(crate) fn refresh(&mut self, s: &Settings) {
        // ★ 실행 속도 향상이 켜져 있으면 강제 대상 카드는 **강제값**을 보여 준다(저장값은 그대로 · 카드는 잠김 · 사용자 09-17
        //   "설정된 값이 아니라 성능 향상 값으로").
        let boost = s.boost_on();
        self.snap = s
            .list()
            .into_iter()
            .map(|(e, v, m)| Snap {
                entry: e,
                value: if boost {
                    nsql_settings::perf::boost_value(e.key)
                        .unwrap_or(v)
                        .to_string()
                } else {
                    v.to_string()
                },
                modified: m,
            })
            .collect();
        if self.cards.is_empty() {
            self.rebuild_cards();
            return;
        }
        for c in &mut self.cards {
            let Some(sn) = self.snap.iter().find(|x| x.entry.key == c.entry.key) else {
                continue;
            };
            c.modified = sn.modified;
            if c.value != sn.value {
                c.value = sn.value.clone();
                c.error = None;
                match &mut c.ctl {
                    CardCtl::Bool(sw) => sw.set_on(sn.value == "on"),
                    CardCtl::Choice(cb) => cb.select_value(&sn.value),
                    CardCtl::Pos(pd) => pd.select_value(HudPos::parse(&sn.value).code()),
                    CardCtl::Text(tb) => {
                        if !tb.is_focused() {
                            tb.set_text(&sn.value);
                            let mut inv = Invalidations::default();
                            tb.select_range(0, 0, &mut inv);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn set_error(&mut self, key: &str, e: String) {
        if let Some(c) = self.cards.iter_mut().find(|c| c.entry.key == key) {
            c.error = Some(e);
        }
    }

    /// 지금 선택/검색에 맞는 카드 목록을 만든다.
    fn rebuild_cards(&mut self) {
        let adv = self.advanced.is_on();
        let q = self.query.trim().to_lowercase();
        let mut chosen: Vec<Snap> = Vec::new();
        if !q.is_empty() {
            for sn in &self.snap {
                if !adv && nsql_settings::is_hidden(sn.entry.key) {
                    continue;
                }
                if self.hidden.contains(&sn.entry.cat) {
                    continue;
                }
                let hay = format!(
                    "{} {} {} {}",
                    sn.entry.key,
                    t(sn.entry.label),
                    t(sn.entry.desc),
                    t(sn.entry.cat)
                )
                .to_lowercase();
                if hay.contains(&q) {
                    chosen.push(sn.clone());
                }
            }
            chosen.sort_by_key(|s| nsql_settings::tree_order(s.entry.cat));
        } else {
            let (gi, ci) = self.sel;
            let cats: Vec<Msg> = match self.vtree.get(gi) {
                Some((_, cats)) => match ci {
                    Some(c) => cats.get(c).copied().into_iter().collect(),
                    None => cats.clone(),
                },
                None => Vec::new(),
            };
            for sn in &self.snap {
                if cats.contains(&sn.entry.cat) && (adv || !nsql_settings::is_hidden(sn.entry.key))
                {
                    chosen.push(sn.clone());
                }
            }
        }
        let show_cat = !q.is_empty() || self.sel.1.is_none();
        self.cards = chosen
            .into_iter()
            .map(|sn| {
                let ctl = match sn.entry.kind {
                    SettingKind::Bool => CardCtl::Bool(
                        Switch::new("", sn.value == "on").with_label_side(LabelSide::None),
                    ),
                    SettingKind::Choice(opts) => {
                        let items: Vec<ComboItem> = opts
                            .iter()
                            .map(|(v, m)| ComboItem::new(*v, t(*m)))
                            .collect();
                        let idx = opts.iter().position(|(v, _)| *v == sn.value).unwrap_or(0);
                        CardCtl::Choice(Box::new(Combo::new(items, idx)))
                    }
                    SettingKind::Lang => {
                        let items: Vec<ComboItem> = Lang::ALL
                            .iter()
                            .map(|l| ComboItem::new(l.code(), l.endonym()))
                            .collect();
                        let idx = Lang::ALL
                            .iter()
                            .position(|l| l.code() == sn.value)
                            .unwrap_or(0);
                        CardCtl::Choice(Box::new(Combo::new(items, idx)))
                    }
                    SettingKind::Position => CardCtl::Pos(Box::new(PositionDropdown::new(
                        HudPos::parse(&sn.value).code(),
                    ))),
                    SettingKind::Int { .. } | SettingKind::Size { .. } | SettingKind::Text => {
                        // 긴 값은 **앞부터** 보이게(캐럿을 0으로 · 사용자 09-17).
                        let mut tb = TextBox::new("").with_text(&sn.value);
                        let mut inv = Invalidations::default();
                        tb.select_range(0, 0, &mut inv);
                        CardCtl::Text(Box::new(tb))
                    }
                };
                let aux = if is_color_key(sn.entry.key) {
                    Some(Button::new(t(Msg::BtnPick)))
                } else if is_key_key(sn.entry.key) {
                    Some(Button::new(t(Msg::BtnCapture)))
                } else {
                    None
                };
                Card {
                    entry: sn.entry,
                    value: sn.value,
                    modified: sn.modified,
                    ctl,
                    reset: Button::new(t(Msg::BtnReset)),
                    aux,
                    rect: Rect::default(),
                    show_cat,
                    error: None,
                    locked: false,
                    default_below: false,
                }
            })
            .collect();
        self.scroll = 0;
        self.content_h = 0;
        self.apply_deps();
    }

    /// 종속 잠금 계산 — 부모 값은 스냅샷(현재 설정)에서.
    fn apply_deps(&mut self) {
        let snap = self.snap.clone();
        let parent_val = |key: &str| -> String {
            snap.iter()
                .find(|s| s.entry.key == key)
                .map(|s| s.value.clone())
                .unwrap_or_default()
        };
        // ★ 실행 속도 향상(`perf.boost` 켬)이 강제하는 키도 잠근다(값은 강제값 · 저장값은 유지 · 09-17).
        let boost = parent_val("perf.boost") == "on";
        for c in &mut self.cards {
            c.locked = nsql_settings::dependency(c.entry.key)
                .is_some_and(|(parent, dep)| !dep.satisfied(&parent_val(parent)))
                || (boost && nsql_settings::perf::boost_value(c.entry.key).is_some());
        }
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if self.window.is_none() {
            return false;
        }
        let mut any = self.bars.tick(now_ms)
            | self.split_fade.tick(now_ms)
            | self.tree.tick(now_ms)
            | self.search.tick(now_ms)
            | self.close_btn.tick(now_ms)
            | self.json_btn.tick(now_ms);
        for c in &mut self.cards {
            any |= c.reset.tick(now_ms);
            if let Some(b) = &mut c.aux {
                any |= b.tick(now_ms);
            }
            match &mut c.ctl {
                CardCtl::Bool(_) => {}
                CardCtl::Choice(cb) => any |= cb.tick_hover(now_ms),
                CardCtl::Pos(_) => {}
                CardCtl::Text(tb) => any |= tb.tick(now_ms),
            }
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some()
            && (self.bars.is_visible()
                || self.split_fade.is_animating()
                || self.search.is_animating()
                || self.close_btn.is_animating()
                || self.json_btn.is_animating()
                || self.cards.iter().any(|c| {
                    c.reset.is_animating()
                        || c.aux.as_ref().is_some_and(|b| b.is_animating())
                        || match &c.ctl {
                            CardCtl::Bool(_) => false,
                            CardCtl::Choice(cb) => cb.hover_animating(),
                            CardCtl::Pos(_) => false,
                            CardCtl::Text(tb) => tb.is_animating(),
                        }
                }))
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        // 창 규칙(사용자 09-17): 같은 모니터면 기록 위치·크기 · 아니면 기본 크기로 메인 창 가운데.
        let same = self.memo.on_same_monitor(owner);
        let (lw, lh) = same.and_then(|(_, s)| s).unwrap_or((920.0, 640.0));
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinPreferences)))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some(((x, y), _)) = same {
            attrs = attrs.with_position(crate::wingeom::logical(x, y));
        } else if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        // 기억한 위치는 프레임 기준으로 다시 놓는다(macOS 제목 표시줄 드리프트 방지 · wingeom::place_outer).
        if let Some(((x, y), _)) = same {
            crate::wingeom::place_outer(&win, Some((x, y)));
        }
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        // ★ IME 허용 — 이 창에도 한글 입력란(검색·값)이 있다. winit 창은 기본으로 IME가 붙지 않아(Windows) 한글 조합이
        //   안 됐다(사용자 09-19 "설정 검색에 한글 입력이 안 된다" · 접속 창·파일 창은 이미 켜 둔 것과 같은 규칙).
        // 앱 조합 모드(T-139)면 IME를 붙이지 않는다 — raw 자모를 받아 상자가 직접 조합한다.
        win.set_ime_allowed(crate::input::system_ime());
        self.window = Some(win);
        self.rebuild_cards();
        self.layout();
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
        self.cards.clear();
    }

    /// 기억된 기하(설정 `window.<name>_pos`/`_size`) — 열 때 같은 모니터면 그대로 쓴다.
    pub(crate) fn set_memo(&mut self, m: crate::wingeom::Memo) {
        self.memo = m;
    }

    /// 마지막으로 닫힌 (위치, 크기)(1회성 · 호스트가 설정에 저장).
    pub(crate) fn take_last(&mut self) -> Option<((i32, i32), (f64, f64))> {
        self.last.take()
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(PAD);
        let mut inv = Invalidations::default();
        let bottom_h = self.s(CTL_H);
        let by = h - pad - bottom_h;
        // 세로 우선 분할(사용자 09-15): 왼쪽 열 = 검색(위) + 트리(아래) · 스플리터 · 오른쪽 = 카드(맨 위부터).
        let max_left = ((w - pad * 2 - self.s(SPLIT_W)) as f32 / s - RIGHT_MIN).max(LEFT_MIN);
        self.left_w = self.left_w.clamp(LEFT_MIN, max_left.max(LEFT_MIN));
        let lw = self.s(self.left_w);
        let top = pad;
        self.search.set_scale(s);
        self.search
            .set_bounds(Rect::new(pad, top, lw, self.s(SEARCH_H)), &mut inv);
        let tree_top = top + self.s(SEARCH_H) + self.s(8.0);
        self.tree.set_scale(s);
        self.tree
            .set_bounds(Rect::new(pad, tree_top, lw, by - pad - tree_top), &mut inv);
        self.split_rect = Rect::new(pad + lw, top, self.s(SPLIT_W), by - pad - top);
        let lx = self.split_rect.right() + pad / 2;
        self.list = Rect::new(lx, top, w - lx - pad, by - pad - top);
        // 아래 줄: 고급 스위치 · 닫기
        self.advanced.set_scale(s);
        self.advanced
            .set_bounds(Rect::new(pad, by, self.s(160.0), bottom_h), &mut inv);
        self.json_btn.set_scale(s);
        self.json_btn.set_bounds(
            Rect::new(
                w - pad - self.s(96.0) * 2 - self.s(8.0),
                by,
                self.s(96.0),
                bottom_h,
            ),
            &mut inv,
        );
        self.close_btn.set_scale(s);
        self.close_btn.set_bounds(
            Rect::new(w - pad - self.s(96.0), by, self.s(96.0), bottom_h),
            &mut inv,
        );
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h - self.list.h).max(0);
        self.scroll = self.scroll.clamp(0, max);
    }

    fn to_input(&self, ev: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: false,
        };
        Some(match ev {
            WindowEvent::CursorMoved { position, .. } => InputEvent::MouseMove {
                x: position.x as i32,
                y: position.y as i32,
            },
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: false,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                (ElementState::Pressed, MouseButton::Right) => InputEvent::RightDown { x, y },
                _ => return None,
            },
            WindowEvent::MouseWheel { delta, .. } => crate::input::wheel_event(delta, self.shift),
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter),
                    Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
                    Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
                    Key::Named(NamedKey::ArrowUp) => key(CtlKey::Up),
                    Key::Named(NamedKey::ArrowDown) => key(CtlKey::Down),
                    Key::Named(NamedKey::Home) => key(CtlKey::Home),
                    Key::Named(NamedKey::End) => key(CtlKey::End),
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) => {
                        // 조합키가 눌린 글자는 타이핑이 아니다(단축키 · 위에서 처리).
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
                }
            }
            _ => return None,
        })
    }

    /// 포커스 텍스트박스(IME).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.search.is_focused() {
            return Some(&mut self.search);
        }
        for c in &mut self.cards {
            if let CardCtl::Text(tb) = &mut c.ctl {
                if tb.is_focused() {
                    return Some(&mut **tb);
                }
            }
        }
        None
    }

    fn any_combo_open(&self) -> bool {
        self.cards.iter().any(|c| match &c.ctl {
            CardCtl::Choice(cb) => cb.is_open(),
            CardCtl::Pos(pd) => pd.is_open(),
            _ => false,
        })
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> PrefsAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return PrefsAction::None;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return PrefsAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return PrefsAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
                return PrefsAction::None;
            }
            // ★ 클립보드·되돌리기(Ctrl/⌘ + C/X/V/A/Z/Y) — 포커스 텍스트박스(검색 · 카드 입력란). 종전엔 Ctrl+V가 글자
            //   `v`로 들어갔다(사용자 09-16 "폰트명에 붙여넣기가 되지 않음"). 접속 창의 `clip`과 같은 OS 클립보드 모듈.
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && self.primary
                    && matches!(kev.logical_key.as_ref(), Key::Character(_)) =>
            {
                let Key::Character(c) = kev.logical_key.as_ref() else {
                    return PrefsAction::None;
                };
                let mut inv = Invalidations::default();
                match c.to_ascii_lowercase().as_str() {
                    "c" => return self.clip(EditCtxAction::Copy),
                    "x" => return self.clip(EditCtxAction::Cut),
                    "v" => return self.clip(EditCtxAction::Paste),
                    "a" => {
                        if let Some(tb) = self.focused_textbox() {
                            tb.on_event(&InputEvent::SelectAll, &mut inv);
                        }
                        self.redraw();
                        return PrefsAction::None;
                    }
                    "z" if !self.shift => {
                        if let Some(tb) = self.focused_textbox() {
                            tb.on_event(&InputEvent::Undo, &mut inv);
                        }
                        return self.after_edit();
                    }
                    "z" | "y" => {
                        if let Some(tb) = self.focused_textbox() {
                            tb.on_event(&InputEvent::Redo, &mut inv);
                        }
                        return self.after_edit();
                    }
                    _ => return PrefsAction::None,
                }
            }
            WindowEvent::Ime(ime) => {
                let mut inv = Invalidations::default();
                if let Some(tb) = self.focused_textbox() {
                    match ime {
                        winit::event::Ime::Preedit(t, _) => tb.set_preedit(t, &mut inv),
                        winit::event::Ime::Commit(t) => {
                            tb.set_preedit("", &mut inv);
                            for c in t.chars().filter(|c| !c.is_control()) {
                                tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                            }
                        }
                        _ => {}
                    }
                    self.redraw();
                }
                // ★ 검색 상자는 IME 경로에서도 **바로** 거른다 — 조합 중 글자까지 포함(`display_text`)해 한 글자씩 반응.
                //   종전엔 `take_changed` 수거가 일반 사건 경로에만 있어 한글을 쳐도 다음 키/클릭 때에야 목록이 바뀌었다(사용자 09-19).
                let _ = self.search.take_changed();
                if self.search.is_focused() {
                    let q = self.search.display_text();
                    if q != self.query {
                        self.query = q;
                        self.rebuild_cards();
                        self.redraw();
                    }
                }
                return self.after_edit();
            }
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
            {
                if self.any_combo_open() {
                    // 콤보가 열려 있으면 Esc는 콤보가 받는다.
                } else {
                    self.close();
                    return PrefsAction::None;
                }
            }
            WindowEvent::RedrawRequested => return PrefsAction::Paint,
            _ => {}
        }
        if let WindowEvent::CursorMoved { position, .. } = ev {
            self.cursor = (position.x as i32, position.y as i32);
        }
        let Some(ie) = self.to_input(ev) else {
            return PrefsAction::None;
        };
        let mut inv = Invalidations::default();
        // 우클릭 직전 — 편집 메뉴의 "붙여넣기" 활성 여부(접속 창과 같은 규약).
        if matches!(ie, InputEvent::RightDown { .. }) {
            let has = crate::clipboard::read_text().is_some_and(|s| !s.is_empty());
            for c in &mut self.cards {
                if let CardCtl::Text(tb) = &mut c.ctl {
                    tb.set_clipboard_has_text(has);
                }
            }
            self.search.set_clipboard_has_text(has);
        }
        // ★ 열린 편집 메뉴 = 모달 · 바깥 좌/우클릭은 닫고 그 클릭을 그대로 진행(팝업 UX 규칙 · 사용자 09-16 "우클릭 메뉴도").
        let outside_click = matches!(
            ie,
            InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
        );
        if let Some(tb) = self.popup_textbox() {
            tb.on_event(&ie, &mut inv);
            let act = tb.take_edit_ctx();
            let still_open = tb.popup_open();
            if let Some(act) = act {
                return self.clip(act);
            }
            if still_open || !outside_click {
                self.redraw();
                return PrefsAction::None;
            }
        }
        // 스플리터: hover = 서서히 진해짐(IntentFade · 마지막 위치만) · 드래그 = 왼쪽 열 폭.
        match ie {
            InputEvent::MouseMove { x, y } => {
                if let Some((x0, w0)) = self.split_drag {
                    self.left_w = w0 + (x - x0) as f32 / self.scale;
                    self.layout();
                    self.redraw();
                    return PrefsAction::None;
                }
                let over = self.split_rect.contains(Point { x, y }) && !self.any_combo_open();
                self.split_fade.set(over.then_some(0));
                if let Some(w) = &self.window {
                    w.set_cursor(if over {
                        winit::window::CursorIcon::ColResize
                    } else {
                        winit::window::CursorIcon::Default
                    });
                }
            }
            InputEvent::MouseDown { x, y, .. }
                if self.split_rect.contains(Point { x, y }) && !self.any_combo_open() =>
            {
                self.split_drag = Some((x, self.left_w));
                self.split_fade.jump(Some(0));
                return PrefsAction::None;
            }
            InputEvent::MouseUp { .. } if self.split_drag.is_some() => {
                self.split_drag = None;
                self.redraw();
                return PrefsAction::None;
            }
            _ => {}
        }
        // 열린 콤보 = 모달(바깥 클릭은 닫고 통과 — 콤보 자체가 처리). ★ 콤보 **머리**(확장 버튼) 재클릭은 접기만 —
        //   통과시키면 같은 클릭이 다시 열었다(사용자 09-16).
        if self.any_combo_open() {
            let mut on_head = false;
            for c in &mut self.cards {
                match &mut c.ctl {
                    CardCtl::Choice(cb) if cb.is_open() => {
                        if let InputEvent::MouseDown { x, y, .. } = ie {
                            on_head |= cb.bounds().contains(Point { x, y });
                        }
                        cb.on_event(&ie, &mut inv);
                    }
                    CardCtl::Pos(pd) if pd.is_open() => {
                        if let InputEvent::MouseDown { x, y, .. } = ie {
                            on_head |= pd.bounds().contains(Point { x, y });
                        }
                        pd.on_event(&ie, &mut inv);
                    }
                    _ => {}
                }
            }
            let a = self.collect_changes();
            self.redraw();
            // ★ 항목을 골라 콤보가 닫힌 클릭은 여기서 끝(변경 보고를 들고 돌아간다). 종전엔 "바깥 클릭 통과" 규칙에
            //   걸려 같은 클릭이 일반 경로로 이어지며 방금 수거한 `Changed`를 버렸다 → 콤보 설정(공백 표시·성능 모드…)이
            //   화면엔 바뀐 듯 보여도 저장되지 않았다(사용자 09-17 "전체로 바꿔도 다시 선택 영역").
            if !matches!(a, PrefsAction::None)
                || !matches!(ie, InputEvent::MouseDown { .. })
                || self.any_combo_open()
                || on_head
            {
                return a;
            }
        }
        let p = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        let is_mouse = matches!(
            ie,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        );
        // 포커스 규칙: MouseDown마다 누른 곳 하나만.
        if let InputEvent::MouseDown { .. } = ie {
            let in_search = self.search.bounds().contains(p);
            self.search.set_focused(in_search);
            self.tree.set_focused(self.tree.bounds().contains(p));
            self.close_btn
                .set_focused(self.close_btn.bounds().contains(p));
            self.json_btn
                .set_focused(self.json_btn.bounds().contains(p));
            for c in &mut self.cards {
                match &mut c.ctl {
                    CardCtl::Text(tb) => tb.set_focused(tb.bounds().contains(p)),
                    CardCtl::Choice(cb) => cb.set_focused(cb.bounds().contains(p)),
                    CardCtl::Pos(pd) => pd.set_focused(pd.bounds().contains(p)),
                    CardCtl::Bool(sw) => sw.set_focused(false),
                }
                c.reset.set_focused(c.reset.bounds().contains(p));
                if let Some(b) = &mut c.aux {
                    b.set_focused(b.bounds().contains(p));
                }
            }
        }
        // 카드 영역 스크롤(휠 · 바)
        let is_wheel = matches!(ie, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
        if self.list.contains(p) || (!is_mouse && !is_wheel) {
            let (_, ny, consumed) = self.bars.on_event(
                &ie,
                self.list,
                self.list.w,
                self.content_h.max(self.list.h),
                0,
                self.scroll,
                self.scale,
            );
            if ny != self.scroll {
                self.scroll = ny;
                self.clamp_scroll();
            }
            if consumed || is_wheel {
                self.redraw();
                return PrefsAction::None;
            }
        }
        // 검색·트리·하단
        // ★ 마우스 사건은 커서 아래 컨트롤에만(CLAUDE.md §3 라우팅 규칙) · 키는 포커스 컨트롤에만.
        let route = |r: Rect, focused: bool| -> bool {
            if is_mouse || is_wheel {
                r.contains(p)
            } else {
                focused
            }
        };
        if route(self.search.bounds(), self.search.is_focused()) {
            self.search.on_event(&ie, &mut inv);
        }
        if route(self.tree.bounds(), self.tree.is_focused()) {
            self.tree.on_event(&ie, &mut inv);
        }
        if route(self.advanced.bounds(), false) {
            self.advanced.on_event(&ie, &mut inv);
        }
        if route(self.close_btn.bounds(), self.close_btn.is_focused())
            || matches!(ie, InputEvent::MouseMove { .. })
        {
            self.close_btn.on_event(&ie, &mut inv);
        }
        if route(self.json_btn.bounds(), self.json_btn.is_focused())
            || matches!(ie, InputEvent::MouseMove { .. })
        {
            self.json_btn.on_event(&ie, &mut inv);
        }
        if let Some(q) = self.search.take_changed() {
            self.query = q;
            self.rebuild_cards();
            self.redraw();
            return PrefsAction::None;
        }
        if self.advanced.take_toggled().is_some() {
            self.rebuild_cards();
            self.redraw();
            return PrefsAction::None;
        }
        if self.close_btn.take_clicked() {
            self.close();
            return PrefsAction::None;
        }
        if self.json_btn.take_clicked() {
            return PrefsAction::EditJson;
        }
        // 트리 선택 변화 → 카테고리 전환(검색 중이면 검색어를 비운다).
        let row = self.tree.selected_row();
        if row != self.last_tree_row {
            self.last_tree_row = row;
            let rows = self.tree.model().flatten();
            if let Some(r) = rows.get(row) {
                let gi = r.path.first().copied().unwrap_or(0);
                let ci = r.path.get(1).copied();
                self.sel = (gi, ci);
                if !self.query.is_empty() {
                    self.query.clear();
                    self.search.set_text("");
                }
                self.rebuild_cards();
                self.redraw();
                return PrefsAction::None;
            }
        }
        // 카드 컨트롤(보이는 것만) — 포커스 텍스트박스는 목록 밖으로 끌어도 MouseMove/MouseUp을 받는다(드래그 선택 · 09-16).
        let drag_follow = matches!(
            ie,
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
        ) && self
            .cards
            .iter()
            .any(|c| matches!(&c.ctl, CardCtl::Text(tb) if tb.is_focused()));
        if self.list.contains(p) || !is_mouse || drag_follow {
            for c in &mut self.cards {
                if c.rect.h == 0 {
                    continue;
                }
                // 종속 잠금(부모 조건 불충족) — 컨트롤은 입력을 받지 않는다(초기화 버튼은 허용).
                if !c.locked {
                    match &mut c.ctl {
                        CardCtl::Bool(sw) => sw.on_event(&ie, &mut inv),
                        CardCtl::Choice(cb) => cb.on_event(&ie, &mut inv),
                        CardCtl::Pos(pd) => pd.on_event(&ie, &mut inv),
                        CardCtl::Text(tb) => tb.on_event(&ie, &mut inv),
                    }
                }
                c.reset.on_event(&ie, &mut inv);
                if let Some(b) = &mut c.aux {
                    b.on_event(&ie, &mut inv);
                }
            }
        }
        let a = self.collect_changes();
        self.redraw();
        a
    }

    fn after_edit(&mut self) -> PrefsAction {
        self.collect_changes()
    }

    /// 클립보드 행동(Ctrl+C/X/V · 입력란 우클릭 편집 메뉴) — 접속 창 `clip`과 같은 OS 클립보드 모듈.
    fn clip(&mut self, act: EditCtxAction) -> PrefsAction {
        let mut inv = Invalidations::default();
        match act {
            EditCtxAction::Copy => {
                if let Some(text) = self.focused_textbox().and_then(|tb| tb.copy_selection()) {
                    let _ = crate::clipboard::write_text(&text);
                }
                self.redraw();
                PrefsAction::None
            }
            EditCtxAction::Cut => {
                if let Some(text) = self
                    .focused_textbox()
                    .and_then(|tb| tb.cut_selection(&mut inv))
                {
                    let _ = crate::clipboard::write_text(&text);
                }
                self.redraw();
                self.after_edit()
            }
            EditCtxAction::Paste => {
                if let Some(text) = crate::clipboard::read_text() {
                    if let Some(tb) = self.focused_textbox() {
                        tb.paste(&text, &mut inv);
                    }
                }
                self.redraw();
                self.after_edit()
            }
            EditCtxAction::Custom(_) => PrefsAction::None,
        }
    }

    /// 편집 메뉴(우클릭)가 열린 텍스트박스(검색 · 카드 입력란).
    fn popup_textbox(&mut self) -> Option<&mut TextBox> {
        if self.search.popup_open() {
            return Some(&mut self.search);
        }
        for c in &mut self.cards {
            if let CardCtl::Text(tb) = &mut c.ctl {
                if tb.popup_open() {
                    return Some(&mut **tb);
                }
            }
        }
        None
    }

    /// 컨트롤 변화 수거 → 첫 변경만 보고(한 이벤트에 하나).
    fn collect_changes(&mut self) -> PrefsAction {
        for c in &mut self.cards {
            let key = c.entry.key.to_string();
            if c.reset.take_clicked() {
                return PrefsAction::Reset(key);
            }
            if let Some(b) = &mut c.aux {
                if b.take_clicked() {
                    return if is_color_key(&key) {
                        PrefsAction::OpenColors(key)
                    } else {
                        PrefsAction::OpenKeys
                    };
                }
            }
            match &mut c.ctl {
                CardCtl::Bool(sw) => {
                    if let Some(on) = sw.take_toggled() {
                        return PrefsAction::Changed {
                            key,
                            value: if on { "on".into() } else { "off".into() },
                        };
                    }
                }
                CardCtl::Choice(cb) => {
                    if let Some(v) = cb.take_changed() {
                        return PrefsAction::Changed { key, value: v };
                    }
                }
                CardCtl::Pos(pd) => {
                    if let Some(code) = pd.take_changed() {
                        return PrefsAction::Changed {
                            key,
                            value: HudPos::from_code(&code).as_str().to_string(),
                        };
                    }
                }
                CardCtl::Text(tb) => {
                    if let Some(v) = tb.take_changed() {
                        // 즉시 검증 — 틀리면 카드에 표시만(저장 안 함).
                        if nsql_settings::normalize(c.entry.kind, &v).is_some()
                            || (v.trim().is_empty() && c.entry.default.is_empty())
                        {
                            c.error = None;
                            return PrefsAction::Changed { key, value: v };
                        }
                        c.error = Some(nsql_settings::allowed(c.entry.kind));
                    }
                }
            }
        }
        PrefsAction::None
    }

    /// 단어 단위 줄바꿈(설명).
    fn wrap(dc: &mut dyn DrawCtx, text: &str, w: i32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut cur = String::new();
            for word in para.split_whitespace() {
                let cand = if cur.is_empty() {
                    word.to_string()
                } else {
                    format!("{cur} {word}")
                };
                if dc.text_width(&cand) <= w || cur.is_empty() {
                    cur = cand;
                } else {
                    lines.push(std::mem::replace(&mut cur, word.to_string()));
                }
            }
            lines.push(cur);
        }
        lines
    }

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
        let (Some(win), Some(surface)) = (self.window.clone(), self.surface.as_mut()) else {
            return;
        };
        let size = win.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let pad = (PAD * s).round() as i32;
        let list = self.list;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs {
                base: SlotFont {
                    size: font_px,
                    bold: false,
                    italic: false,
                },
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, ui, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            // ── 카드 레이아웃(텍스트 폭 실측 · 스크롤 반영) → 컨트롤 bounds
            let card_w = list.w - (18.0 * s).round() as i32;
            let inner_pad = (10.0 * s).round() as i32;
            let ctl_h = (CTL_H * s).round() as i32;
            let gap = (CARD_GAP * s).round() as i32;
            let text_w = card_w - inner_pad * 2 - (120.0 * s).round() as i32;
            let mut y = list.y + gap - self.scroll;
            let mut inv = Invalidations::default();
            let mut desc_lines: Vec<Vec<String>> = Vec::with_capacity(self.cards.len());
            let boost_on = self
                .snap
                .iter()
                .any(|s| s.entry.key == "perf.boost" && s.value == "on");
            for c in &mut self.cards {
                let lines = Self::wrap(&mut dc, t(c.entry.desc), text_w);
                let boost_hint =
                    c.locked && boost_on && nsql_settings::perf::boost_value(c.entry.key).is_some();
                let mut extra = if c.error.is_some() || boost_hint {
                    th_txt
                } else {
                    0
                };
                // 기본값 글자("Default: …")가 컨트롤 줄의 빈 자리에 안 들어가면 설명 아래 줄로 내린다(긴 URL · 사용자 09-17).
                let dw = dc.text_width(&tf(Msg::LblDefaultValue, &[c.entry.default]));
                let row_used = {
                    let ctl_w = match &c.ctl {
                        CardCtl::Bool(_) => (56.0 * s).round() as i32,
                        CardCtl::Choice(_) => (260.0 * s).round() as i32,
                        CardCtl::Pos(_) => (64.0 * s).round() as i32,
                        CardCtl::Text(_) => (320.0 * s).round() as i32,
                    };
                    let aux_w = if c.aux.is_some() {
                        (90.0 * s).round() as i32 + (8.0 * s).round() as i32
                    } else {
                        0
                    };
                    inner_pad * 2
                        + ctl_w
                        + (8.0 * s).round() as i32
                        + aux_w
                        + (90.0 * s).round() as i32
                };
                c.default_below = dw + (16.0 * s).round() as i32 > card_w - row_used;
                if c.default_below {
                    extra += th_txt;
                }
                let ch = inner_pad * 2
                    + th_txt
                    + (4.0 * s).round() as i32
                    + th_txt * lines.len() as i32
                    + (8.0 * s).round() as i32
                    + ctl_h
                    + extra;
                c.rect = Rect::new(list.x, y, card_w, ch);
                // 컨트롤 줄(카드 아래쪽): [컨트롤][보조][초기화]
                let cy = y + ch - inner_pad - ctl_h;
                let mut cx = list.x + inner_pad;
                let ctl_w = match &c.ctl {
                    CardCtl::Bool(_) => (56.0 * s).round() as i32,
                    CardCtl::Choice(_) => (260.0 * s).round() as i32,
                    CardCtl::Pos(_) => (64.0 * s).round() as i32,
                    CardCtl::Text(_) => (320.0 * s).round() as i32,
                };
                let visible = y + ch > list.y && y < list.bottom();
                // ★ 컨트롤 줄은 **목록 안에 온전히 들어올 때만** 배치한다(카드 글자는 클립되지만 컨트롤은 자기 bounds를
                //   그대로 그려 하단 줄 위로 삐져나왔다 · 사용자 09-15) — 밖이면 빈 rect(그리지도 클릭받지도 않음).
                let ctl_visible = visible && cy >= list.y && cy + ctl_h <= list.bottom();
                let hidden = Rect::default();
                let r = if ctl_visible {
                    Rect::new(cx, cy, ctl_w, ctl_h)
                } else {
                    hidden
                };
                match &mut c.ctl {
                    CardCtl::Bool(sw) => {
                        sw.set_scale(s);
                        sw.set_bounds(r, &mut inv);
                    }
                    CardCtl::Choice(cb) => {
                        cb.set_scale(s);
                        cb.set_bounds(r, &mut inv);
                    }
                    CardCtl::Pos(pd) => {
                        pd.set_scale(s);
                        pd.set_max_bottom(list.bottom());
                        pd.set_bounds(r, &mut inv);
                    }
                    CardCtl::Text(tb) => {
                        tb.set_scale(s);
                        tb.set_bounds(r, &mut inv);
                    }
                }
                cx += ctl_w + (8.0 * s).round() as i32;
                if let Some(b) = &mut c.aux {
                    b.set_scale(s);
                    let bw = (90.0 * s).round() as i32;
                    b.set_bounds(
                        if ctl_visible {
                            Rect::new(cx, cy, bw, ctl_h)
                        } else {
                            hidden
                        },
                        &mut inv,
                    );
                    cx += bw + (8.0 * s).round() as i32;
                    if is_color_key(c.entry.key) {
                        cx += ctl_h + (8.0 * s).round() as i32; // 스와치 자리
                    }
                }
                c.reset.set_scale(s);
                let rw = (90.0 * s).round() as i32;
                c.reset.set_bounds(
                    if ctl_visible && c.modified {
                        Rect::new(cx, cy, rw, ctl_h)
                    } else {
                        hidden
                    },
                    &mut inv,
                );
                if !visible {
                    c.rect.h = 0;
                }
                desc_lines.push(lines);
                y += ch + gap;
            }
            self.content_h = y + self.scroll - list.y;
            // ── 그리기: 검색 · 트리 · 카드(클립) · 하단
            self.search.paint(&mut dc, th);
            self.tree.paint(&mut dc, th);
            // 스플리터 — 가는 선 + hover/드래그 시 진해지는 손잡이.
            let sr = self.split_rect;
            let cx = sr.x + sr.w / 2;
            dc.fill_rect(Rect::new(cx, sr.y, 1, sr.h), th.border);
            let a = hover_alpha(self.split_drag.is_some(), self.split_fade.value(0));
            if a > 0.0 {
                dc.fill_rect_alpha(Rect::new(cx - 1, sr.y, 3, sr.h), th.accent, a.max(0.15));
            }
            dc.fill_rect(list, th.window_bg);
            for (c, lines) in self.cards.iter().zip(desc_lines.iter()) {
                if c.rect.h == 0 {
                    continue;
                }
                let r = c.rect;
                let clip = r.intersection(&list);
                if clip.h <= 0 {
                    continue;
                }
                dc.fill_round_rect(clip, (6.0 * s).round() as i32, th.panel_bg);
                // 기본값과 다른 값의 왼쪽 강조선은 그리지 않는다(사용자 09-16 — 초기화 버튼이 같은 뜻을 전한다).
                if false && c.modified {
                    dc.fill_rect(
                        Rect::new(r.x, r.y, (3.0 * s).round() as i32, r.h).intersection(&list),
                        th.accent,
                    );
                }
                let tx = r.x + inner_pad;
                let mut ty = r.y + inner_pad;
                dc.select_font(FontSlot::Base, true);
                let label = if c.show_cat {
                    format!("{} › {}", t(c.entry.cat), t(c.entry.label))
                } else {
                    t(c.entry.label).to_string()
                };
                dc.text(tx, ty, clip, &label, th.text);
                dc.select_font(FontSlot::Base, false);
                let key_w = dc.text_width(c.entry.key);
                dc.text(
                    r.right() - inner_pad - key_w,
                    ty,
                    clip,
                    c.entry.key,
                    th.text_dim,
                );
                ty += th_txt + (4.0 * s).round() as i32;
                for l in lines {
                    dc.text(tx, ty, clip, l, th.text_dim);
                    ty += th_txt;
                }
                if let Some(e) = &c.error {
                    dc.text(tx, ty, clip, e, th.danger);
                }
                // 향상 모드 강제값 안내(잠긴 카드 · 설명 아래 한 줄).
                if c.locked && boost_on {
                    if let Some(v) = nsql_settings::perf::boost_value(c.entry.key) {
                        dc.text(tx, ty, clip, &tf(Msg::PrefsBoostLocked, &[v]), th.accent);
                    }
                }
                // 기본값 표시 — 컨트롤 오른쪽 끝 · 안 들어가면 설명 아래 한 줄(사용자 09-17 "is used instead 줄 밑에").
                let dv = tf(Msg::LblDefaultValue, &[c.entry.default]);
                let dw = dc.text_width(&dv);
                let cy = r.bottom() - inner_pad - ctl_h;
                if c.default_below {
                    let ty2 = ty + if c.error.is_some() { th_txt } else { 0 };
                    dc.text(tx, ty2, clip, &dv, th.text_dim);
                } else {
                    dc.text(
                        r.right() - inner_pad - dw,
                        cy + (ctl_h - th_txt) / 2,
                        clip,
                        &dv,
                        th.text_dim,
                    );
                }
                match &c.ctl {
                    CardCtl::Bool(sw) => sw.paint(&mut dc, th),
                    CardCtl::Choice(_) => {}
                    CardCtl::Pos(pd) => pd.paint(&mut dc, th),
                    CardCtl::Text(tb) => tb.paint(&mut dc, th),
                }
                if c.locked {
                    // 잠긴 종속 항목은 흐리게(콤보는 팝업 층에서 따로 흐림 처리 없이 잠금만).
                    let cr = match &c.ctl {
                        CardCtl::Bool(sw) => sw.bounds(),
                        CardCtl::Choice(cb) => cb.bounds(),
                        CardCtl::Pos(pd) => pd.bounds(),
                        CardCtl::Text(tb) => tb.bounds(),
                    };
                    dc.fill_rect_alpha(cr, th.panel_bg, 0.6);
                }
                if let Some(b) = &c.aux {
                    b.paint(&mut dc, th);
                    // ★ 색 미리보기 스와치(사용자 09-15) — 값이 비면 테마 선택색(= 실제 적용값) · 알파는 바탕 위에 섞어 보인다.
                    if is_color_key(c.entry.key) && b.bounds().h > 0 {
                        let bb = b.bounds();
                        let sz = ctl_h;
                        let sw = Rect::new(bb.right() + (8.0 * s).round() as i32, bb.y, sz, sz);
                        let rgba = nexa_ctl::rgba_from_hex(&c.value);
                        let (col, alpha) = match rgba {
                            Some(v) => (
                                nexa_ctl::Color::from_rgb(
                                    (v >> 24) as u8,
                                    (v >> 16) as u8,
                                    (v >> 8) as u8,
                                ),
                                (v & 0xFF) as f32 / 255.0,
                            ),
                            None => (th.sel_bg, 1.0),
                        };
                        dc.fill_round_rect(sw, 4, th.window_bg);
                        dc.fill_round_rect_alpha(sw, 4, col, alpha);
                        dc.stroke_round_rect(sw, 4, th.border, 1.0);
                    }
                }
                if c.modified {
                    c.reset.paint(&mut dc, th);
                }
            }
            // 콤보는 드롭다운이 아래 카드를 덮어야 하므로 맨 뒤에.
            for c in &self.cards {
                if c.rect.h == 0 {
                    continue;
                }
                match &c.ctl {
                    CardCtl::Choice(cb) => cb.paint(&mut dc, th),
                    CardCtl::Pos(pd) => pd.paint_popup(&mut dc, th),
                    _ => {}
                }
            }
            self.bars.paint(
                &mut dc,
                th,
                list,
                list.w,
                self.content_h.max(list.h),
                0,
                self.scroll,
                s,
            );
            // 하단
            self.advanced.paint(&mut dc, th);
            self.close_btn.paint(&mut dc, th);
            self.json_btn.paint(&mut dc, th);
            let _ = pad;
            self.search.paint_popup(&mut dc, th);
            for c in &self.cards {
                if let CardCtl::Text(tb) = &c.ctl {
                    tb.paint_popup(&mut dc, th);
                }
            }
        }
        let _ = buf.present();
    }
}
