//! 찾기/바꾸기 **플로팅 위젯** — VS Code Find Widget을 픽셀 치수까지 그대로(사용자 09-16 · 캡처 11장 대조).
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────────┐ ← 419 × 34(찾기만) / 62(바꾸기 펼침) · 아래 모서리 r4
//! │[›]│[Find                              Aa ab .*]│ 2 of 23 │ ↑ ↓ ≡ × │
//! │   │[Replace                                 AB]│ ⇄ ⇄⇄                   │
//! └──────────────────────────────────────────────────────────────────────────┘
//!   3+18   26                      243  246   315  318 343 368 393(+22)
//! ```
//! - 왼쪽 세로 토글(`.button.toggle` 18px · 전체 높이 · 셰브론 › / ⌄) = 바꾸기 줄 펼침/접기.
//! - 입력 상자 25px(위 3px) · 안쪽 오른쪽에 20×20 토글(위 3 · 오른쪽 2 · 간격 2 · r3 · 1px 테두리).
//! - 일치 수 69px("No results" / "2 of 23") · 동작 버튼 22×22(아이콘 16 + 3 · r5 · 간격 3).
//! - 아이콘 = Google Material Symbols SVG를 마스크로(`toolicons::mi_*`) · 글자색 틴트.
//! - 토글 **On** = accent 40% 채움 + accent 테두리 · **hover** = 1초(`ui.fade_slow`)에 걸쳐 밝아지는 회색 · **선택(포커스)** = 점선 accent
//!   테두리 — 세 상태가 서로 구별된다(사용자 09-16).
//! - 툴팁 = 버튼 **위** 캡슐 · 단축키는 Sublime Text 기준(Alt+C/W/R/A · Shift+Enter/Enter · Ctrl+Shift+H · Ctrl+Alt+Enter · Esc).
//! - 검색 자체(대소문자 · 단어 · 정규식 · 범위)는 호스트가 편집기 텍스트에 대해 수행하고 여기는 입력·토글·상태만.

use nexa_ctl::controls::ctxmenu::MenuIcon;
use nexa_ctl::draw::{draw_tooltip_in, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{Color, IconImage, Theme};
use nexa_ctl::tokens::{hover_color, Fade, FadeSpeed};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nsql_i18n::{t, Msg};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::search_history::{Recall, RecallEvent, SharedHistory};
use crate::toolicons;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindAction {
    None,
    Next,
    Prev,
    Replace,
    ReplaceAll,
    Close,
    /// 질의·토글이 바뀌었다(호스트가 개수를 다시 센다 · 첫 일치로 이동).
    Changed,
    /// "선택 범위에서 찾기"가 켜지거나 꺼졌다(호스트가 범위를 잡거나 푼다).
    ScopeChanged,
    /// 일치 전부 다중 선택(Sublime Alt+Enter "Find All").
    SelectAll,
}

/// 토글·동작 버튼 종류(툴팁·단축키 문구).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BtnKind {
    Fold,
    Case,
    Word,
    Regex,
    Prev,
    Next,
    Selection,
    Close,
    Preserve,
    Replace,
    ReplaceAll,
    /// 프로젝트 탐색기 전용(09-22) — 숨김 파일 표시 · 점 파일 표시(Windows) · 경로까지 검색(틀 안 넷째 토글 · 기본 끔).
    Hidden,
    DotFiles,
    PathMatch,
}

impl BtnKind {
    pub(crate) fn tip(self) -> Msg {
        match self {
            BtnKind::Fold => Msg::TipFindToggleReplace,
            BtnKind::Case => Msg::TipFindCase,
            BtnKind::Word => Msg::TipFindWord,
            BtnKind::Regex => Msg::TipFindRegex,
            BtnKind::Hidden => Msg::TipProjectHidden,
            BtnKind::DotFiles => Msg::TipProjectDot,
            BtnKind::PathMatch => Msg::TipFilterPath,
            BtnKind::Prev => Msg::TipFindPrev,
            BtnKind::Next => Msg::TipFindNext,
            BtnKind::Selection => Msg::TipFindSelection,
            BtnKind::Close => Msg::TipFindClose,
            BtnKind::Preserve => Msg::TipFindPreserve,
            BtnKind::Replace => Msg::TipFindReplace,
            BtnKind::ReplaceAll => Msg::TipFindReplaceAll,
        }
    }
    fn is_toggle(self) -> bool {
        matches!(
            self,
            BtnKind::Case
                | BtnKind::Word
                | BtnKind::Regex
                | BtnKind::Selection
                | BtnKind::Preserve
                | BtnKind::Hidden
                | BtnKind::DotFiles
                | BtnKind::PathMatch
        )
    }
}

// VS Code findWidget.css 치수(논리 px).
const WIDGET_W: f32 = 419.0;
const ROW1_H: f32 = 34.0;
const ROW2_H: f32 = 62.0;
const PAD_L: f32 = 9.0;
const PAD_R: f32 = 4.0;
/// 접기 셰브론 왼쪽 여백 = 오른쪽(찾기 상자까지) 여백 = 5(사용자 09-17 · VS Code 3/5에서 조정).
const FOLD_X: f32 = 5.0;
const FOLD_W: f32 = 18.0;
const PART_ML: f32 = 19.0;
const INPUT_H: f32 = 25.0;
const INPUT_TOP: f32 = 3.0;
const ROW_PITCH: f32 = 28.0;
const TOGGLE: f32 = 20.0;
const TOGGLE_GAP: f32 = 2.0;
const TOGGLE_TOP: f32 = 3.0;
const TOGGLE_RIGHT: f32 = 2.0;
const BTN: f32 = 22.0;
const BTN_GAP: f32 = 3.0;
const COUNT_W: f32 = 69.0;
const COUNT_GAP: f32 = 3.0;
/// 우상단 여백(사용자 09-17 "흰 편집 영역 기준 위·오른쪽 각각 10px") — 편집기 텍스트박스 사각형이 기준.
const MARGIN_RIGHT: f32 = 10.0;
const MARGIN_TOP: f32 = 10.0;

/// 아이콘 버튼/토글 하나 — 상태 3종이 구별된다(On · hover 페이드 · 포커스). 파일 검색 패널도 같은 부품을 쓴다.
pub(crate) struct FindBtn {
    pub(crate) kind: BtnKind,
    pub(crate) rect: Rect,
    /// 아이콘은 첫 그리기 때 만든다(찾기 막대는 시작 시 숨겨져 있다 — 09-22 Linux 기동 계측: 11개 즉시 래스터 = 166 ms).
    icon: fn() -> MenuIcon,
    built: std::cell::OnceCell<MenuIcon>,
    tint: RefCell<Option<(Color, Rc<IconImage>)>>,
    pub(crate) checked: bool,
    pub(crate) hover: Fade,
    pub(crate) hover_on: bool,
    pressed: bool,
    pub(crate) focused: bool,
    enabled: bool,
    clicked: bool,
    /// 툴팁 대기 시작(hover 진입 시각).
    pub(crate) hover_since: Option<Instant>,
    /// 둥근 정도(토글 3 · 동작 5 · 접기 2).
    radius: f32,
}

impl FindBtn {
    pub(crate) fn new(kind: BtnKind, icon: fn() -> MenuIcon) -> Self {
        let radius = match kind {
            BtnKind::Fold => 2.0,
            k if k.is_toggle() => 3.0,
            _ => 5.0,
        };
        FindBtn {
            kind,
            rect: Rect::default(),
            icon,
            built: std::cell::OnceCell::new(),
            tint: RefCell::new(None),
            checked: false,
            // 1초에 걸쳐 밝아진다(사용자 09-16) = `FadeSpeed::Slow` ↔ 설정 `ui.fade_slow`.
            hover: Fade::at(FadeSpeed::Slow),
            hover_on: false,
            pressed: false,
            focused: false,
            enabled: true,
            clicked: false,
            hover_since: None,
            radius,
        }
    }

    fn tinted(&self, fg: Color) -> Rc<IconImage> {
        if let Some((c, img)) = self.tint.borrow().as_ref() {
            if *c == fg {
                return img.clone();
            }
        }
        let (r, g, b) = fg.rgb();
        let icon = self.built.get_or_init(self.icon);
        let img = Rc::new(IconImage::from_alpha_tinted(
            icon.w,
            icon.h,
            &icon.alpha,
            (r, g, b),
        ));
        *self.tint.borrow_mut() = Some((fg, img.clone()));
        img
    }

    fn set_hover(&mut self, on: bool) {
        if on != self.hover_on {
            self.hover_on = on;
            self.hover.set(on);
            self.hover_since = on.then(Instant::now);
        }
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) {
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let inside = self.rect.contains(Point { x, y });
                self.set_hover(inside && self.enabled);
            }
            InputEvent::MouseDown { x, y, .. } => {
                let inside = self.rect.contains(Point { x, y });
                self.pressed = inside && self.enabled;
                // 접기 셰브론은 포커스를 갖지 않는다(클릭만 · 사용자 09-17).
                self.focused = inside && self.enabled && self.kind != BtnKind::Fold;
            }
            InputEvent::MouseUp { x, y } => {
                if self.pressed && self.rect.contains(Point { x, y }) {
                    self.clicked = true;
                }
                self.pressed = false;
            }
            _ => {}
        }
    }

    /// 사용 가능 상태 — 끄면 hover·눌림·포커스를 비우고 흐리게 그린다(사용자 09-17: 쓸 수 없으면 Disable).
    pub(crate) fn set_enabled(&mut self, on: bool) {
        if self.enabled == on {
            return;
        }
        self.enabled = on;
        if !on {
            self.pressed = false;
            self.focused = false;
            self.clicked = false;
            self.hover_since = None;
            self.set_hover(false);
            self.hover.jump(false);
        }
    }

    pub(crate) fn take_clicked(&mut self) -> bool {
        std::mem::take(&mut self.clicked)
    }

    pub(crate) fn clear_transient(&mut self) {
        self.pressed = false;
        self.focused = false;
        self.set_hover(false);
        self.hover.jump(false);
    }

    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let b = self.rect;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        let r = (self.radius * s).round() as i32;
        let px = |v: f32| (v * s).round().max(1.0) as i32;
        // On = accent 40% + accent 테두리(VS Code inputOption.activeBackground/activeBorder).
        if self.checked {
            dc.fill_round_rect_alpha(b, r, th.accent, 0.4);
            dc.stroke_round_rect(b, r, th.accent, 1.0);
        }
        // hover = 회색 오버레이가 1초에 걸쳐 진해진다(On 위에도 얹혀 "켜진 채 hover"가 보인다).
        let hv = self.hover.value();
        if hv > 0.0 && self.enabled {
            let (hc, ha) = hover_color(th);
            dc.fill_round_rect_alpha(b, r, hc, (ha * 1.2).min(1.0) * hv);
        }
        if self.pressed {
            dc.fill_round_rect_alpha(b, r, th.text, 0.12);
        }
        // 아이콘 16px(스케일) · 글자색 틴트(비활성 = 흐림).
        let fg = if self.enabled { th.text } else { th.text_dim };
        let img = self.tinted(fg);
        let icon = px(16.0);
        let dst = Rect::new(b.x + (b.w - icon) / 2, b.y + (b.h - icon) / 2, icon, icon);
        dc.image_scaled(dst, &img, b);
        // 포커스(선택) = 점선 accent 테두리(VS Code focus-visible dashed) — On의 실선과 구별.
        if self.focused {
            let dash = px(2.0);
            let mut x = b.x;
            while x < b.right() {
                let w = dash.min(b.right() - x);
                dc.fill_rect(Rect::new(x, b.y, w, 1), th.accent);
                dc.fill_rect(Rect::new(x, b.bottom() - 1, w, 1), th.accent);
                x += dash * 2;
            }
            let mut y = b.y;
            while y < b.bottom() {
                let h = dash.min(b.bottom() - y);
                dc.fill_rect(Rect::new(b.x, y, 1, h), th.accent);
                dc.fill_rect(Rect::new(b.right() - 1, y, 1, h), th.accent);
                y += dash * 2;
            }
        }
    }
}

pub(crate) struct FindBar {
    visible: bool,
    with_replace: bool,
    bounds: Rect,
    scale: f32,
    query: TextBox,
    repl: TextBox,
    /// 입력 상자 전체 틀(토글 포함) — 텍스트박스는 왼쪽 부분만 차지한다.
    find_frame: Rect,
    repl_frame: Rect,
    count_rect: Rect,
    btns: Vec<FindBtn>,
    status: String,
    shift: bool,
    /// 툴팁 지연(ms · 설정 `ui.tooltip_delay_ms`).
    tooltip_ms: u128,
    /// 창 클라이언트 폭(툴팁 클램프).
    clamp_w: i32,
    /// 검색어 이력(전역 · `find.query`/`find.replace` · ↑/↓ 되부르기 · Enter·Replace = 기록 · 사용자 09-23).
    history: Option<SharedHistory>,
    rq: Recall,
    rr: Recall,
}

impl FindBar {
    /// 검색어 이력 잇기(호스트).
    pub(crate) fn set_history(&mut self, h: SharedHistory) {
        self.history = Some(h);
    }

    /// 찾기(+바꾸기) 글을 이력에 올린다 — 실행하는 순간(Enter · Replace/Replace All).
    fn history_commit(&mut self, with_repl: bool) {
        if let Some(h) = &self.history {
            self.rq.commit(&self.query, h);
            if with_repl && self.with_replace {
                self.rr.commit(&self.repl, h);
            }
        }
    }

    pub(crate) fn new() -> Self {
        let btns = vec![
            FindBtn::new(BtnKind::Fold, toolicons::mi_chevron_right),
            FindBtn::new(BtnKind::Case, toolicons::mi_match_case),
            FindBtn::new(BtnKind::Word, toolicons::mi_match_word),
            FindBtn::new(BtnKind::Regex, toolicons::mi_regex),
            FindBtn::new(BtnKind::Prev, toolicons::mi_arrow_up),
            FindBtn::new(BtnKind::Next, toolicons::mi_arrow_down),
            FindBtn::new(BtnKind::Selection, toolicons::mi_in_selection),
            FindBtn::new(BtnKind::Close, toolicons::mi_close),
            FindBtn::new(BtnKind::Preserve, toolicons::mi_preserve_case),
            FindBtn::new(BtnKind::Replace, toolicons::mi_find_replace),
            FindBtn::new(BtnKind::ReplaceAll, toolicons::mi_replace_all),
        ];
        // × 지우기 = 검색 입력란 공통(글이 있을 때만 · nexa-ctl `with_clearable` · 사용자 09-23).
        let mut query = TextBox::new(t(Msg::PhFind)).with_clearable();
        query.set_focus_ring(false);
        let mut repl = TextBox::new(t(Msg::PhReplace)).with_clearable();
        repl.set_focus_ring(false);
        FindBar {
            visible: false,
            with_replace: false,
            bounds: Rect::default(),
            scale: 1.0,
            query,
            repl,
            find_frame: Rect::default(),
            repl_frame: Rect::default(),
            count_rect: Rect::default(),
            btns,
            status: String::new(),
            shift: false,
            tooltip_ms: 600,
            clamp_w: i32::MAX / 2,
            history: None,
            rq: Recall::new("find.query"),
            rr: Recall::new("find.replace"),
        }
    }

    fn btn(&self, k: BtnKind) -> &FindBtn {
        self.btns.iter().find(|b| b.kind == k).expect("버튼")
    }
    fn btn_mut(&mut self, k: BtnKind) -> &mut FindBtn {
        self.btns.iter_mut().find(|b| b.kind == k).expect("버튼")
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }

    /// 찾을 글 — **조합 중 글자 포함**(IME preedit · 한 글자씩 바로 찾는다 · 사용자 09-23).
    pub(crate) fn query(&self) -> String {
        self.query.display_text()
    }

    pub(crate) fn replacement(&self) -> String {
        self.repl.text()
    }

    pub(crate) fn case_sensitive(&self) -> bool {
        self.btn(BtnKind::Case).checked
    }

    pub(crate) fn whole_word(&self) -> bool {
        self.btn(BtnKind::Word).checked
    }

    /// 정규식 모드(D-76).
    pub(crate) fn regex(&self) -> bool {
        self.btn(BtnKind::Regex).checked
    }

    /// 선택 범위에서 찾기(≡ · Alt+L).
    pub(crate) fn in_selection(&self) -> bool {
        self.btn(BtnKind::Selection).checked
    }

    /// 대소문자 보존(AB · Alt+A).
    pub(crate) fn preserve_case(&self) -> bool {
        self.btn(BtnKind::Preserve).checked
    }

    /// 호스트가 범위를 잡지 못했을 때(선택 없음) 토글을 되돌린다.
    pub(crate) fn set_in_selection(&mut self, on: bool) {
        self.btn_mut(BtnKind::Selection).checked = on;
    }

    /// 일치가 1건 이상일 때만 이전/다음·바꾸기·전부 바꾸기를 쓸 수 있다(사용자 09-17).
    pub(crate) fn set_has_matches(&mut self, on: bool) {
        for k in [
            BtnKind::Prev,
            BtnKind::Next,
            BtnKind::Replace,
            BtnKind::ReplaceAll,
        ] {
            self.btn_mut(k).set_enabled(on);
        }
    }

    /// In selection 토글 = 편집기에 선택이 있을 때(또는 이미 켜져 있어 끌 수 있을 때)만.
    pub(crate) fn set_selection_available(&mut self, on: bool) {
        let b = self.btn_mut(BtnKind::Selection);
        let usable = on || b.checked;
        b.set_enabled(usable);
    }

    pub(crate) fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    pub(crate) fn set_tooltip_delay(&mut self, ms: u128) {
        self.tooltip_ms = ms;
    }

    /// 열기(이미 열려 있으면 질의 상자로 포커스) — `seed` = 편집기 선택 텍스트(한 줄일 때).
    pub(crate) fn open(&mut self, with_replace: bool, seed: Option<String>) {
        self.visible = true;
        self.with_replace |= with_replace;
        if let Some(s) = seed.filter(|s| !s.is_empty() && !s.contains('\n')) {
            self.query.set_text(&s);
        }
        self.sync_fold_icon();
        self.focus_query();
    }

    pub(crate) fn close(&mut self) {
        self.visible = false;
        self.with_replace = false;
        self.query.set_focused(false);
        self.repl.set_focused(false);
        for b in &mut self.btns {
            b.clear_transient();
        }
        self.btn_mut(BtnKind::Selection).checked = false;
    }

    /// 바꾸기 줄 펼침/접기(Ctrl+H · 왼쪽 셰브론) — 호스트가 다시 배치.
    pub(crate) fn toggle_replace(&mut self) {
        self.with_replace = !self.with_replace;
        if !self.with_replace {
            self.repl.set_focused(false);
            if !self.query.is_focused() {
                self.focus_query();
            }
        }
        self.sync_fold_icon();
    }

    fn sync_fold_icon(&mut self) {
        let icon: fn() -> MenuIcon = if self.with_replace {
            toolicons::mi_chevron_down
        } else {
            toolicons::mi_chevron_right
        };
        let b = self.btn_mut(BtnKind::Fold);
        b.icon = icon;
        b.built = std::cell::OnceCell::new();
        *b.tint.borrow_mut() = None;
    }

    fn focus_query(&mut self) {
        self.query.set_focused(true);
        self.repl.set_focused(false);
        for b in &mut self.btns {
            b.focused = false;
        }
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        if on {
            if !self.repl.is_focused() {
                self.query.set_focused(true);
            }
        } else {
            self.query.set_focused(false);
            self.repl.set_focused(false);
            for b in &mut self.btns {
                b.focused = false;
            }
        }
    }

    /// 포커스 텍스트박스(IME · 클립보드 라우팅).
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.repl.is_focused() {
            Some(&mut self.repl)
        } else if self.query.is_focused() {
            Some(&mut self.query)
        } else {
            None
        }
    }

    /// 편집기 사각형 기준 **오른쪽 위**에 붙는다(VS Code · 본문을 밀지 않는다).
    pub(crate) fn set_bounds(&mut self, editor: Rect, scale: f32) {
        self.scale = scale;
        let s = scale;
        let px = |v: f32| (v * s).round() as i32;
        let right = editor.right();
        let w = px(WIDGET_W).min((right - editor.x - px(MARGIN_RIGHT)).max(px(260.0)));
        let h = px(if self.with_replace { ROW2_H } else { ROW1_H });
        let x = (right - px(MARGIN_RIGHT) - w).max(editor.x);
        self.bounds = Rect::new(x, editor.y + px(MARGIN_TOP), w, h);
        let b = self.bounds;
        let mut inv = Invalidations::default();
        // 오른쪽 동작 버튼: 닫기부터 왼쪽으로 22 + 3.
        let bw = px(BTN);
        let pitch = bw + px(BTN_GAP);
        let y1 = b.y + px(INPUT_TOP);
        let row_h = px(INPUT_H);
        // 왼쪽 접기 셰브론: x 3 · 폭 18 · 높이 = 접힘이면 옆 입력 상자와 같은 top/높이 · 펼침이면 찾기 상자 top부터
        //   바꾸기 상자 bottom까지(두 상자 + 사이 여백 · 사용자 09-17).
        let fold_h = if self.with_replace {
            px(ROW_PITCH) + row_h
        } else {
            row_h
        };
        self.btn_mut(BtnKind::Fold).rect = Rect::new(b.x + px(FOLD_X), y1, px(FOLD_W), fold_h);
        let by = |row_y: i32| row_y + (row_h - bw) / 2;
        let close_x = b.right() - px(PAD_R) - bw;
        let sel_x = close_x - pitch;
        let next_x = sel_x - pitch;
        let prev_x = next_x - pitch;
        for (k, x) in [
            (BtnKind::Close, close_x),
            (BtnKind::Selection, sel_x),
            (BtnKind::Next, next_x),
            (BtnKind::Prev, prev_x),
        ] {
            self.btn_mut(k).rect = Rect::new(x, by(y1), bw, bw);
        }
        // 일치 수 69px(버튼 왼쪽 3px 앞).
        let count_w = px(COUNT_W);
        self.count_rect = Rect::new(prev_x - px(COUNT_GAP) - count_w, y1, count_w, row_h);
        // 찾기 입력 틀: 26px부터 일치 수 3px 앞까지 · 안쪽 오른쪽에 토글 3개.
        let in_x = b.x + px(PAD_L) + px(PART_ML);
        let in_w = (self.count_rect.x - px(COUNT_GAP) - in_x).max(px(80.0));
        self.find_frame = Rect::new(in_x, y1, in_w, row_h);
        let tg = px(TOGGLE);
        let tgap = px(TOGGLE_GAP);
        let ty = y1 + px(TOGGLE_TOP);
        let mut tx = self.find_frame.right() - px(TOGGLE_RIGHT) - tg;
        for k in [BtnKind::Regex, BtnKind::Word, BtnKind::Case] {
            self.btn_mut(k).rect = Rect::new(tx, ty, tg, tg);
            tx -= tg + tgap;
        }
        // 텍스트박스 = 틀의 왼쪽(토글 영역 제외).
        let text_w = (tx + tg + tgap - in_x).max(px(40.0));
        self.query.set_scale(s);
        self.query
            .set_bounds(Rect::new(in_x, y1, text_w, row_h), &mut inv);
        // 바꾸기 줄(펼쳤을 때만): 같은 x · 같은 폭 · AB 토글 · 바꾸기/모두 버튼은 틀 오른쪽 3px 뒤.
        if self.with_replace {
            let y2 = y1 + px(ROW_PITCH);
            self.repl_frame = Rect::new(in_x, y2, in_w, row_h);
            let px_x = self.repl_frame.right() - px(TOGGLE_RIGHT) - tg;
            self.btn_mut(BtnKind::Preserve).rect = Rect::new(px_x, y2 + px(TOGGLE_TOP), tg, tg);
            let rtext_w = (px_x - tgap - in_x).max(px(40.0));
            self.repl.set_scale(s);
            self.repl
                .set_bounds(Rect::new(in_x, y2, rtext_w, row_h), &mut inv);
            let rx = self.repl_frame.right() + px(COUNT_GAP);
            self.btn_mut(BtnKind::Replace).rect = Rect::new(rx, by(y2), bw, bw);
            self.btn_mut(BtnKind::ReplaceAll).rect = Rect::new(rx + pitch, by(y2), bw, bw);
        } else {
            self.repl_frame = Rect::default();
            self.repl.set_bounds(Rect::default(), &mut inv);
            for k in [BtnKind::Preserve, BtnKind::Replace, BtnKind::ReplaceAll] {
                self.btn_mut(k).rect = Rect::default();
            }
        }
    }

    /// 창 클라이언트 폭(툴팁이 창 밖으로 나가지 않게).
    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if !self.visible {
            return false;
        }
        let mut any = self.query.tick(now_ms) | self.repl.tick(now_ms);
        for b in &mut self.btns {
            any |= b.hover.tick(now_ms);
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.visible
            && (self.query.is_animating()
                || self.repl.is_animating()
                || self.btns.iter().any(|b| b.hover.is_animating())
                || self.tooltip_pending())
    }

    /// 툴팁 지연 중(호스트가 프레임 예약).
    pub(crate) fn tooltip_pending(&self) -> bool {
        self.btns.iter().any(|b| {
            b.hover_since
                .is_some_and(|t0| t0.elapsed().as_millis() < self.tooltip_ms + 50)
        })
    }

    /// 단축키(호스트 키맵 `find.*` · 패널에 포커스일 때만 호스트가 부른다).
    pub(crate) fn command(&mut self, id: &str) -> FindAction {
        match id {
            "find.case" => self.flip(BtnKind::Case),
            "find.word" => self.flip(BtnKind::Word),
            "find.regex" => self.flip(BtnKind::Regex),
            "find.preserve" => self.flip(BtnKind::Preserve),
            "find.selection" => self.flip(BtnKind::Selection),
            "find.replace_one" => FindAction::Replace,
            "find.replace_all" => FindAction::ReplaceAll,
            "find.all" => FindAction::SelectAll,
            _ => FindAction::None,
        }
    }

    fn flip(&mut self, k: BtnKind) -> FindAction {
        let b = self.btn_mut(k);
        b.checked = !b.checked;
        if k == BtnKind::Selection {
            FindAction::ScopeChanged
        } else {
            FindAction::Changed
        }
    }

    /// 이벤트 → 호스트 동작. 마우스는 커서가 패널 안일 때 · 키는 패널에 포커스일 때 호스트가 넘긴다.
    /// 찾기/바꾸기 상자의 우클릭 편집 메뉴가 열려 있는가 — 호스트가 마우스를 바 밖까지 보내고 Esc를 메뉴에 준다.
    pub(crate) fn popup_open(&self) -> bool {
        self.query.popup_open()
            || (self.with_replace && self.repl.popup_open())
            || self.rq.is_open()
            || self.rr.is_open()
    }

    /// 편집 메뉴에서 고른 클립보드 행동(Copy/Cut/Paste) — 호스트가 실행.
    pub(crate) fn take_edit_ctx(&mut self) -> Option<nexa_ctl::EditCtxAction> {
        self.query
            .take_edit_ctx()
            .or_else(|| self.repl.take_edit_ctx())
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> FindAction {
        if !self.visible {
            return FindAction::None;
        }
        let mut inv = Invalidations::default();
        if let InputEvent::Key { shift, .. } = ev {
            self.shift = *shift;
        }
        // 우클릭 편집 메뉴가 열려 있으면 그 메뉴가 먼저(항목 클릭 · Esc = 메뉴만 닫기 · 사용자 09-17).
        if self.query.popup_open() || (self.with_replace && self.repl.popup_open()) {
            if self.query.popup_open() {
                self.query.on_event(ev, &mut inv);
            } else {
                self.repl.on_event(ev, &mut inv);
            }
            return FindAction::None;
        }
        // 검색어 이력(드롭다운/Flat)이 먼저 — 찾기 상자는 글이 바뀐 것이라 다시 찾는다 · 끝에서 ↓/Tab은 찾기 막대엔 다음 컨트롤이 없어 무시.
        if let Some(h) = &self.history {
            if self.query.is_focused() || self.rq.is_open() {
                match self.rq.on_event(ev, &mut self.query, h, &mut inv) {
                    RecallEvent::Consumed | RecallEvent::LeaveDown => return FindAction::None,
                    RecallEvent::Changed => {
                        let _ = self.query.take_changed();
                        return FindAction::Changed;
                    }
                    RecallEvent::Pass => {}
                }
            }
            if self.with_replace && (self.repl.is_focused() || self.rr.is_open()) {
                match self.rr.on_event(ev, &mut self.repl, h, &mut inv) {
                    RecallEvent::Pass => {}
                    RecallEvent::Changed => {
                        let _ = self.repl.take_changed();
                        return FindAction::None;
                    }
                    _ => return FindAction::None,
                }
            }
        }
        // Enter = 다음(Shift = 이전) · Esc = 닫기(Sublime 규약 · 바꾸기 상자에서도 Enter = 다음 · 바꾸기는 Ctrl+Shift+H).
        if let InputEvent::Key { key, shift, .. } = ev {
            match key {
                CtlKey::Enter => {
                    self.history_commit(false);
                    return if *shift {
                        FindAction::Prev
                    } else {
                        FindAction::Next
                    };
                }
                CtlKey::Escape => return FindAction::Close,
                _ => {}
            }
        }
        // 클릭 = 포커스 이동(링 ≤ 1 · CLAUDE.md §3).
        if let InputEvent::MouseDown { x, y, .. } = *ev {
            let p = Point { x, y };
            let in_q = self.query.bounds().contains(p);
            let in_r = self.with_replace && self.repl.bounds().contains(p);
            self.query.set_focused(in_q);
            self.repl.set_focused(in_r);
        }
        self.query.on_event(ev, &mut inv);
        if self.with_replace {
            self.repl.on_event(ev, &mut inv);
        }
        // 상자 클릭 = 이력 드롭다운(보기 방식 Dropdown · 이력 있을 때 · 사용자 09-23).
        if let (InputEvent::MouseDown { x, y, .. }, Some(h)) = (ev, &self.history) {
            let p = Point { x: *x, y: *y };
            let host = Rect::new(0, 0, self.clamp_w, i32::MAX / 4);
            if self.query.bounds().contains(p) {
                self.rq.on_click(&self.query, h, host, self.scale);
            } else if self.with_replace && self.repl.bounds().contains(p) {
                self.rr.on_click(&self.repl, h, host, self.scale);
            }
        }
        for b in &mut self.btns {
            b.on_event(ev);
        }
        // 버튼 하나만 포커스(마지막 눌린 것).
        if let InputEvent::MouseDown { x, y, .. } = *ev {
            let p = Point { x, y };
            let hit = self.btns.iter().position(|b| b.rect.contains(p));
            for (i, b) in self.btns.iter_mut().enumerate() {
                b.focused = Some(i) == hit;
            }
        }
        if self.query.take_changed().is_some() {
            if let Some(h) = &self.history {
                self.rq.after_edit(&self.query, h);
            }
            return FindAction::Changed;
        }
        if self.repl.take_changed().is_some() {
            if let Some(h) = &self.history {
                self.rr.after_edit(&self.repl, h);
            }
        }
        let clicked: Vec<BtnKind> = self
            .btns
            .iter_mut()
            .filter_map(|b| b.take_clicked().then_some(b.kind))
            .collect();
        let Some(&k) = clicked.first() else {
            return FindAction::None;
        };
        match k {
            BtnKind::Fold => {
                self.toggle_replace();
                FindAction::Changed
            }
            BtnKind::Case
            | BtnKind::Word
            | BtnKind::Regex
            | BtnKind::Preserve
            | BtnKind::Selection => self.flip(k),
            BtnKind::Prev => {
                self.history_commit(false);
                FindAction::Prev
            }
            BtnKind::Next => {
                self.history_commit(false);
                FindAction::Next
            }
            BtnKind::Close => FindAction::Close,
            BtnKind::Replace => {
                self.history_commit(true);
                FindAction::Replace
            }
            BtnKind::ReplaceAll => {
                self.history_commit(true);
                FindAction::ReplaceAll
            }
            // 프로젝트 탐색기 전용 토글 — 찾기 막대에는 없다.
            BtnKind::Hidden | BtnKind::DotFiles | BtnKind::PathMatch => FindAction::None,
        }
    }

    /// 입력 틀(토글 포함) 하나 그리기 — 텍스트박스는 왼쪽 부분이라 그 오른쪽 경계선을 지워 한 상자로 보이게 한다.
    fn paint_frame(dc: &mut dyn DrawCtx, th: &Theme, frame: Rect, tb: &TextBox, s: f32) {
        let r = (6.0 * s).round() as i32;
        dc.fill_round_rect(frame, r, th.field_bg);
        dc.stroke_round_rect(frame, r, th.border, 1.0);
        tb.paint(dc, th);
        let tbb = tb.bounds();
        if tbb.w > 0 {
            // 텍스트박스 오른쪽 테두리를 틀 배경으로 덮는다(둥근 모서리 안쪽만).
            dc.fill_rect(
                Rect::new(tbb.right() - 2, tbb.y + r, 3, (tbb.h - r * 2).max(0)),
                th.field_bg,
            );
        }
        if tb.is_focused() {
            dc.stroke_round_rect(frame, r, th.accent, 1.0);
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let s = self.scale;
        let b = self.bounds;
        let r = (4.0 * s).round() as i32;
        // 떠 있는 패널 — 네 모서리 둥글게 · **bounds에 정확히**(예전엔 편집기 위에 붙어 y−r에서 그려 위 여백을
        //   8px 잠식했다 · 사용자 09-17 "위 3~5px") · 테두리 + 그림자 한 줄.
        dc.fill_round_rect(b, r, th.chrome_bg);
        dc.stroke_round_rect(b, r, th.border, 1.0);
        dc.fill_rect_alpha(Rect::new(b.x + 1, b.bottom(), b.w - 2, 1), th.text, 0.12);
        dc.select_font(FontSlot::Base, false);
        Self::paint_frame(dc, th, self.find_frame, &self.query, s);
        if self.with_replace {
            Self::paint_frame(dc, th, self.repl_frame, &self.repl, s);
        }
        // 일치 수("No results" / "2 of 23") — 왼쪽 정렬 · 흐린 글자 · 없음은 경고색.
        if !self.status.is_empty() {
            dc.select_font(FontSlot::Status, false);
            let cr = self.count_rect;
            let ty = dc.text_center_y(cr.y, cr.h);
            let none = self.status == t(Msg::StFindNone);
            dc.text(
                cr.x + (2.0 * s).round() as i32,
                ty,
                cr,
                &self.status,
                if none { th.danger } else { th.text_dim },
            );
        }
        for btn in &self.btns {
            if !self.with_replace
                && matches!(
                    btn.kind,
                    BtnKind::Preserve | BtnKind::Replace | BtnKind::ReplaceAll
                )
            {
                continue;
            }
            btn.paint(dc, th, s);
        }
        self.query.paint_popup(dc, th);
        if self.with_replace {
            self.repl.paint_popup(dc, th);
        }
        self.rq.paint_popup(dc, th);
        self.rr.paint_popup(dc, th);
    }

    /// 툴팁(팝업 층 · 버튼 **위** 캡슐 · hover가 지연 시간을 넘긴 버튼 하나).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let Some(btn) = self.btns.iter().find(|b| {
            b.hover_on
                && b.rect.w > 0
                && b.hover_since
                    .is_some_and(|t0| t0.elapsed().as_millis() >= self.tooltip_ms)
        }) else {
            return;
        };
        let s = self.scale;
        let text = t(btn.kind.tip()).to_string();
        // 캡슐 높이를 재서 버튼 위에 오도록 앵커를 잡는다(draw_tooltip은 앵커 아래 6px에 그린다).
        dc.select_font(FontSlot::Status, false);
        let h = dc.text_height() + (8.0 * s).round() as i32;
        let anchor = Rect::new(
            btn.rect.x,
            btn.rect.y - h - (12.0 * s).round() as i32,
            btn.rect.w,
            0,
        );
        draw_tooltip_in(dc, th, anchor, (0, self.clamp_w), &text, s);
    }
}
