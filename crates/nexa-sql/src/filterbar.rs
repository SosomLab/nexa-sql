//! 필터 틀 부품(사용자 09-22) — 텍스트박스 + 안쪽 토글 **Aa · ab · (.*)**(찾기 막대와 같은 `FindBtn`) + 틀 오른쪽 부가 토글.
//!
//! 프로젝트 탐색기 · 북마크 패널 · 확장 패널이 같은 것을 쓴다(둘째 사용처에서 부품으로 · [30 §2](../../../docs/30-architecture-patterns.md)).
//! 검색 패널(파일 검색)은 자체 배치이되 토글은 같은 부품이다. 매칭은 이 부품이 한다([`FilterBar::matches`]):
//! 옵션이 전부 꺼져 있으면 **공백으로 나눈 낱말 전부 포함(대소문자 무시)** · 하나라도 켜지면 `rx::compile`(글자 그대로/정규식 · `\b` · `(?i)`).
//! 정규식 오류 = 틀 테두리 danger + 아무것도 일치하지 않음.

use crate::findbar::{BtnKind, FindBtn};
use crate::search_history::{Recall, RecallEvent, SharedHistory};
use crate::{rx, toolicons};
use nexa_ctl::controls::ctxmenu::MenuIcon;
use nexa_ctl::draw::{draw_tooltip_in, DrawCtx};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TextBox, Widget};
use nsql_i18n::t;

/// 입력 상자 높이(네 패널 공통).
pub(crate) const INPUT_H: f32 = 25.0;
/// 필터 위·아래 여백(검색·프로젝트·북마크·확장 패널 공통 · 사용자 09-22 "위와 아래에 추가 여백 · 모두 동일하게").
pub(crate) const GAP_Y: f32 = 8.0;

/// 틀 오른쪽 부가 토글 하나(종류 · 아이콘).
pub(crate) type SideBtn = (BtnKind, fn() -> MenuIcon);

/// [`FilterBar::on_event`]의 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterEvent {
    None,
    /// 글이나 옵션(Aa·ab·(.*))이 바뀌었다 — 호출자는 목록을 다시 거른다.
    Changed,
    /// 틀 오른쪽 부가 토글이 눌렸다(호출자가 뜻을 정한다 · 예 숨김 파일).
    Side(BtnKind),
    /// 이력(드롭다운/Flat)이 사건을 먹었다 — 호출자는 다시 그리기만.
    Consumed,
    /// 이력의 끝에서 ↓ 또는 Tab — 호출자가 **목록으로 포커스**를 옮긴다(사용자 09-23 "마지막 기록에서 ↓ = 다른 컨트롤로").
    LeaveDown,
}

/// 필터 판정기(부품과 같은 규칙 · 복제 가능 · 부품 없이도 만든다).
#[derive(Clone, Debug)]
pub(crate) struct Matcher {
    rx: Option<fancy_regex::Regex>,
    err: bool,
    text: String,
}

impl Matcher {
    /// 옵션 없는 글 필터(공백으로 나눈 낱말 전부 포함 · 대소문자 무시) — 시험·기동 명령용.
    #[allow(dead_code)]
    pub(crate) fn plain(text: &str) -> Self {
        Matcher {
            rx: None,
            err: false,
            text: text.to_string(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rx.is_none() && self.text.trim().is_empty()
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn matches(&self, hay: &str) -> bool {
        if self.err {
            return false;
        }
        match &self.rx {
            Some(r) => r.is_match(hay).unwrap_or(false),
            None => {
                let q = self.text.trim();
                if q.is_empty() {
                    return true;
                }
                let h = hay.to_lowercase();
                // 한글 토큰 = 자모열 비교(조합 중 "ㄱ"·"기"도 걸린다 · nsql-core `hangul` · 사용자 09-23).
                q.split_whitespace().all(|t| {
                    if nsql_core::hangul::has_hangul(t) {
                        nsql_core::hangul::contains_jamo(
                            hay,
                            &nsql_core::hangul::decompose(t, true),
                            true,
                        )
                    } else {
                        h.contains(&t.to_lowercase())
                    }
                })
            }
        }
    }
}

/// 검색 진행 상태(호스트 → 필터 틀 · 84 §8).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SearchState {
    Idle,
    Running,
    Done,
}

/// 도는 선 한 바퀴(ms) · 선 길이(둘레 비율) · 완료 깜빡임 한 위상(ms) · 위상 수(켬·끔·켬·끔 = 두 번).
const LAP_MS: u64 = 1200;
const SEG_FRACTION: f32 = 0.22;
/// 혜성 꼬리 단계 수(꼬리 = 배경에 가깝게 · 머리 = 강조색 · 사용자 09-25 "조금 더 눈에 띄게").
const COMET_STEPS: usize = 8;
const BLINK_MS: u64 = 130;
const BLINK_PHASES: u64 = 4;

/// 둥근 사각형 둘레의 폴리라인(시작 = 위쪽 변 왼쪽 끝 · 시계 방향 · 모서리는 호를 6분할).
pub(crate) fn round_rect_path(fb: Rect, r: i32) -> Vec<(f32, f32)> {
    let r = r.max(0).min(fb.w / 2).min(fb.h / 2) as f32;
    let (x0, y0, x1, y1) = (
        fb.x as f32,
        fb.y as f32,
        fb.right() as f32,
        fb.bottom() as f32,
    );
    let mut pts = Vec::with_capacity(32);
    let arc = |pts: &mut Vec<(f32, f32)>, cx: f32, cy: f32, a0: f32, a1: f32| {
        let n = 6;
        for i in 0..=n {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    };
    use std::f32::consts::PI;
    pts.push((x0 + r, y0));
    pts.push((x1 - r, y0));
    arc(&mut pts, x1 - r, y0 + r, -PI / 2.0, 0.0);
    pts.push((x1, y1 - r));
    arc(&mut pts, x1 - r, y1 - r, 0.0, PI / 2.0);
    pts.push((x0 + r, y1));
    arc(&mut pts, x0 + r, y1 - r, PI / 2.0, PI);
    pts.push((x0, y0 + r));
    arc(&mut pts, x0 + r, y0 + r, PI, 1.5 * PI);
    pts
}

pub(crate) fn path_len(path: &[(f32, f32)]) -> f32 {
    path.windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum()
}

/// 둘레 위 `[start, start+len)` 구간의 점들(끝점 보간 · 한 바퀴를 넘으면 둘로 나눠 돌려준다).
pub(crate) fn path_window(
    path: &[(f32, f32)],
    total: f32,
    start: f32,
    len: f32,
) -> Vec<Vec<(i32, i32)>> {
    if total <= 0.0 || len <= 0.0 || path.len() < 2 {
        return Vec::new();
    }
    let s = start.rem_euclid(total);
    let e = s + len.min(total);
    let mut out = Vec::new();
    if e <= total {
        out.push(path_slice(path, s, e));
    } else {
        out.push(path_slice(path, s, total));
        out.push(path_slice(path, 0.0, e - total));
    }
    out.retain(|v| v.len() >= 2);
    out
}

fn path_slice(path: &[(f32, f32)], a: f32, b: f32) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::new();
    let mut acc = 0.0f32;
    let push = |p: (f32, f32), out: &mut Vec<(i32, i32)>| {
        let q = (p.0.round() as i32, p.1.round() as i32);
        if out.last() != Some(&q) {
            out.push(q);
        }
    };
    for w in path.windows(2) {
        let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        let (sa, sb) = (acc, acc + d);
        if sb >= a && sa <= b && d > 0.0 {
            let ta = ((a - sa) / d).clamp(0.0, 1.0);
            let tb = ((b - sa) / d).clamp(0.0, 1.0);
            let pa = (
                w[0].0 + (w[1].0 - w[0].0) * ta,
                w[0].1 + (w[1].1 - w[0].1) * ta,
            );
            let pb = (
                w[0].0 + (w[1].0 - w[0].0) * tb,
                w[0].1 + (w[1].1 - w[0].1) * tb,
            );
            push(pa, &mut out);
            push(pb, &mut out);
        }
        acc = sb;
        if acc > b {
            break;
        }
    }
    out
}

pub(crate) struct FilterBar {
    tb: TextBox,
    /// 틀(텍스트박스 + 안쪽 토글) · `area` = 틀 + 오른쪽 부가 토글(히트 영역).
    frame: Rect,
    area: Rect,
    btns: Vec<FindBtn>,
    matcher: Option<fancy_regex::Regex>,
    regex_err: bool,
    /// 마지막으로 반영한 글(표시 글 = 조합 중 글자 포함).
    text: String,
    scale: f32,
    tooltip_ms: u128,
    clamp_w: i32,
    /// 자리 표시 글(비활성 그리기용).
    placeholder: String,
    /// 검색어 이력(전역 · 상자 이름별 · ↑/↓ 되부르기 · Enter/포커스 잃음 = 기록 · 사용자 09-23) — 호스트가 `set_history`로 준다.
    history: Option<(SharedHistory, Recall)>,
    /// ★ 검색 진행 표시(84 §8 · 사용자 09-25 "진행 중인지 직관적으로"): 진행 = 테두리를 따라 도는 밝은 선 · 완료 = 두 번 깜빡인 뒤 완료 테두리.
    search: SearchState,
    /// 완료 깜빡임 시작 시각(ms · `tick`의 시계) — Running → Done 전환 뒤 첫 tick에 잡는다.
    blink_start: Option<u64>,
    blink_pending: bool,
    anim_now: u64,
    /// 완료 시그니처를 사용자가 "봤다"(상자 재포커스 · 글 변경 · 초기화) → 다음 Running까지 Done을 기본 테두리로(사용자 09-25).
    done_seen: bool,
}

impl FilterBar {
    /// `side` = 틀 오른쪽에 붙는 부가 토글(없으면 빈 슬라이스).
    pub(crate) fn new(placeholder: &str, side: &[SideBtn]) -> Self {
        // × 지우기 = 검색 입력란 공통(글이 있을 때만 · 사용자 09-23) — 네 패널이 함께 얻는다.
        let mut tb = TextBox::new(placeholder).with_clearable();
        tb.set_focus_ring(false);
        let mut btns = vec![
            FindBtn::new(BtnKind::Case, toolicons::mi_match_case),
            FindBtn::new(BtnKind::Word, toolicons::mi_match_word),
            FindBtn::new(BtnKind::Regex, toolicons::mi_regex),
        ];
        for &(k, icon) in side {
            btns.push(FindBtn::new(k, icon));
        }
        FilterBar {
            tb,
            frame: Rect::default(),
            area: Rect::default(),
            btns,
            matcher: None,
            regex_err: false,
            text: String::new(),
            scale: 1.0,
            tooltip_ms: 600,
            clamp_w: i32::MAX / 2,
            placeholder: placeholder.to_string(),
            history: None,
            search: SearchState::Idle,
            blink_start: None,
            blink_pending: false,
            anim_now: 0,
            done_seen: false,
        }
    }

    /// 검색어 이력 잇기 — `key` = 상자 이름(`filter.project` 등 · 같은 이름 = 이력 공유).
    pub(crate) fn set_history(&mut self, h: SharedHistory, key: &str) {
        self.history = Some((h, Recall::new(key)));
    }

    /// 지금 글을 이력에 올린다(Enter · 포커스 잃음 · 빈 글은 부품이 무시).
    fn history_commit(&mut self) {
        if let Some((h, r)) = &mut self.history {
            r.commit(&self.tb, h);
        }
    }

    fn inline_kind(k: BtnKind) -> bool {
        matches!(
            k,
            BtnKind::Case | BtnKind::Word | BtnKind::Regex | BtnKind::PathMatch
        )
    }

    /// 틀 안 넷째 토글 "경로까지 검색"(프로젝트 탐색기 · 기본 끔 · 사용자 09-22) — 매칭 대상은 호출자가 고른다(`is_on(PathMatch)`).
    pub(crate) fn with_path_toggle(mut self) -> Self {
        let at = self
            .btns
            .iter()
            .position(|b| !Self::inline_kind(b.kind))
            .unwrap_or(self.btns.len());
        self.btns.insert(
            at,
            FindBtn::new(BtnKind::PathMatch, toolicons::mi_path_match),
        );
        self
    }

    pub(crate) fn tb_mut(&mut self) -> &mut TextBox {
        &mut self.tb
    }

    /// 텍스트박스 영역(시험 · 클릭 좌표 계산용).
    #[cfg(test)]
    pub(crate) fn text_bounds(&self) -> Rect {
        self.tb.bounds()
    }

    fn history_open(&self) -> bool {
        self.history.as_ref().is_some_and(|(_, r)| r.is_open())
    }

    /// 열린 팝업의 영역(이력 드롭다운 · 텍스트박스 편집 메뉴) — 호스트의 `menu_bounds`에 합쳐 바깥 클릭 판정이 맞게(같은 클릭을 두 번 보내지 않게).
    pub(crate) fn popup_bounds(&self) -> Rect {
        if let Some((_, r)) = &self.history {
            if r.is_open() {
                return r.bounds();
            }
        }
        if self.tb.popup_open() {
            return self.tb.popup_bounds();
        }
        Rect::default()
    }

    /// 팝업(텍스트박스 편집 메뉴 · 이력 드롭다운)이 열려 있는가 — 패널의 `menu_open`에 합친다.
    pub(crate) fn popup_open(&self) -> bool {
        self.tb.popup_open() || self.history.as_ref().is_some_and(|(_, r)| r.is_open())
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        if !on && self.tb.is_focused() {
            self.history_commit();
        }
        if !on {
            if let Some((_, r)) = &mut self.history {
                r.close();
            }
        }
        self.tb.set_focused(on);
        if !on {
            for b in &mut self.btns {
                b.focused = false;
            }
        }
    }

    pub(crate) fn is_focused(&self) -> bool {
        self.tb.is_focused()
    }

    pub(crate) fn set_tooltip_delay(&mut self, ms: u128) {
        self.tooltip_ms = ms;
    }

    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
    }

    /// 확정된 글.
    pub(crate) fn text(&self) -> String {
        self.tb.text()
    }

    /// 표시 글(IME 조합 중 글자 포함) — 거르기의 기준.
    pub(crate) fn display_text(&self) -> String {
        self.tb.display_text()
    }

    pub(crate) fn set_text(&mut self, s: &str) {
        self.tb.set_text(s);
        self.refresh();
    }

    /// 거르는 글이 비었는가.
    pub(crate) fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub(crate) fn is_on(&self, k: BtnKind) -> bool {
        self.btns.iter().any(|b| b.kind == k && b.checked)
    }

    pub(crate) fn set_checked(&mut self, k: BtnKind, on: bool) {
        if let Some(b) = self.btns.iter_mut().find(|b| b.kind == k) {
            b.checked = on;
        }
    }

    #[cfg(test)]
    pub(crate) fn regex_err(&self) -> bool {
        self.regex_err
    }

    /// 히트 영역(틀 + 부가 토글).
    pub(crate) fn bounds(&self) -> Rect {
        self.area
    }

    /// 배치 — `area` = 한 줄 전체(높이 = 입력 상자) · 부가 토글은 오른쪽 끝에서 안쪽으로.
    pub(crate) fn set_bounds(&mut self, area: Rect, scale: f32) {
        self.area = area;
        self.scale = scale;
        let px = |v: f32| (v * scale).round() as i32;
        let ih = area.h;
        let gap = px(4.0);
        let side_n = self
            .btns
            .iter()
            .filter(|b| !Self::inline_kind(b.kind))
            .count() as i32;
        let side_w = side_n * (ih + gap);
        let frame = Rect::new(area.x, area.y, (area.w - side_w).max(px(80.0)), ih);
        self.frame = frame;
        // 안쪽 토글 = 검색 패널·찾기 막대와 같은 크기(20 · 간격 2 · 세로 중앙).
        let tg = px(20.0);
        let tgap = px(2.0);
        let mut tx = frame.right() - px(2.0) - tg;
        // 안쪽 토글은 넣은 순서대로 왼쪽→오른쪽(Aa · ab · (.*) · [경로]) = 오른쪽 끝에서 거꾸로 놓는다.
        for b in self
            .btns
            .iter_mut()
            .rev()
            .filter(|b| Self::inline_kind(b.kind))
        {
            b.rect = Rect::new(tx, area.y + (ih - tg) / 2, tg, tg);
            tx -= tg + tgap;
        }
        let mut inv = Invalidations::default();
        self.tb.set_scale(scale);
        self.tb.set_bounds(
            Rect::new(
                frame.x,
                frame.y,
                (tx + tg + tgap - frame.x).max(px(40.0)),
                ih,
            ),
            &mut inv,
        );
        let mut x = frame.right() + gap;
        for b in self.btns.iter_mut().filter(|b| !Self::inline_kind(b.kind)) {
            b.rect = Rect::new(x, area.y, ih, ih);
            x += ih + gap;
        }
    }

    /// 사건 — 토글은 언제나 · 텍스트박스는 키/글자 = 포커스일 때 · 마우스 = 상자 안일 때(MouseUp·이동은 늘).
    pub(crate) fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> FilterEvent {
        let was_focused = self.tb.is_focused();
        let r = self.on_event_inner(ev, inv);
        if (!was_focused && self.tb.is_focused()) || r == FilterEvent::Changed {
            self.dismiss_done();
        }
        r
    }

    fn on_event_inner(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> FilterEvent {
        for b in &mut self.btns {
            b.on_event(ev);
        }
        let mut to_tb = false;
        match *ev {
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                let hit = self.btns.iter().position(|b| b.rect.contains(p));
                for (i, b) in self.btns.iter_mut().enumerate() {
                    b.focused = Some(i) == hit;
                }
                if hit.is_some() {
                    self.tb.set_focused(false);
                } else if self.tb.bounds().contains(p) {
                    self.tb.set_focused(true);
                    to_tb = true;
                } else if self.history_open() {
                    // ★ 열린 이력 드롭다운(상자 아래) 클릭 = 항목 고르기 · 그 밖 = 닫기(사용자 09-25 "클릭으로는 선택이 안 된다").
                    to_tb = true;
                }
            }
            // 우클릭 = 드롭다운이 열려 있으면 닫힘 판정을 받게(바깥 우클릭 = 즉시 감춤 · 사용자 09-25).
            InputEvent::RightDown { .. } => to_tb = self.history_open(),
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. } => to_tb = true,
            InputEvent::Key { .. }
            | InputEvent::Char { .. }
            | InputEvent::Undo
            | InputEvent::Redo => to_tb = self.tb.is_focused(),
            _ => to_tb = self.tb.is_focused(),
        }
        let clicked: Vec<BtnKind> = self
            .btns
            .iter_mut()
            .filter_map(|b| b.take_clicked().then_some(b.kind))
            .collect();
        if let Some(&k) = clicked.first() {
            if Self::inline_kind(k) {
                if let Some(b) = self.btns.iter_mut().find(|b| b.kind == k) {
                    b.checked = !b.checked;
                }
                self.refresh();
                return FilterEvent::Changed;
            }
            return FilterEvent::Side(k);
        }
        if to_tb {
            // 이력(드롭다운/Flat)이 먼저 — 열린 드롭다운의 이동/고르기 · 닫힌 채 ↑/↓ · 끝에서 ↓/Tab = 목록으로(사용자 09-23).
            let clicked_tb = matches!(ev, InputEvent::MouseDown { x, y, .. } if self.tb.bounds().contains(Point { x: *x, y: *y }));
            if let Some((h, r)) = &mut self.history {
                if self.tb.is_focused() || r.is_open() {
                    match r.on_event(ev, &mut self.tb, h, inv) {
                        RecallEvent::Consumed => return FilterEvent::Consumed,
                        RecallEvent::Changed => {
                            let _ = self.tb.take_changed();
                            self.refresh();
                            return FilterEvent::Changed;
                        }
                        RecallEvent::LeaveDown => return FilterEvent::LeaveDown,
                        RecallEvent::Pass => {}
                    }
                }
            }
            self.tb.on_event(ev, inv);
            if self.tb.take_committed().is_some() {
                self.history_commit();
            }
            if let Some((h, r)) = &mut self.history {
                if clicked_tb && self.tb.is_focused() {
                    // 상자 클릭 = 드롭다운(보기 방식이 Dropdown일 때 · 이력이 있을 때만).
                    let host = Rect::new(0, 0, self.clamp_w, i32::MAX / 4);
                    r.on_click(&self.tb, h, host, self.scale);
                }
            }
        }
        if self.tb.take_changed().is_some() {
            self.refresh();
            // 타이핑으로 글이 바뀌면 열린 드롭다운은 그 글로 다시 거른다.
            if let Some((h, r)) = &mut self.history {
                r.after_edit(&self.tb, h);
            }
            return FilterEvent::Changed;
        }
        FilterEvent::None
    }

    /// 표시 글·옵션으로 매처를 다시 만든다(IME 조합 뒤 호스트가 부르기도 한다).
    pub(crate) fn refresh(&mut self) {
        self.text = self.tb.display_text();
        let text = self.text.trim().to_string();
        let (case, word, regex) = (
            self.is_on(BtnKind::Case),
            self.is_on(BtnKind::Word),
            self.is_on(BtnKind::Regex),
        );
        self.regex_err = false;
        if text.is_empty() || !(case || word || regex) {
            self.matcher = None;
            return;
        }
        match rx::compile(&text, regex, case, word) {
            Ok(r) => self.matcher = Some(r),
            Err(_) => {
                self.matcher = None;
                self.regex_err = true;
            }
        }
    }

    /// 이 글이 필터에 걸리는가(빈 필터 = 전부).
    pub(crate) fn matches(&self, hay: &str) -> bool {
        self.matcher().matches(hay)
    }

    /// 지금 글·옵션의 **복제 가능한 판정기**(탐색기처럼 구조가 바뀔 때마다 스스로 다시 걸러야 하는 쪽이 들고 있는다 · 09-25).
    pub(crate) fn matcher(&self) -> Matcher {
        Matcher {
            rx: self.matcher.clone(),
            err: self.regex_err,
            text: self.text.clone(),
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.anim_now = now_ms;
        let mut any = self.tb.tick(now_ms);
        for b in &mut self.btns {
            any |= b.hover.tick(now_ms);
        }
        if self.blink_pending {
            self.blink_pending = false;
            self.blink_start = Some(now_ms);
        }
        any || self.search_animating()
    }

    /// 호스트가 검색 상태를 알려 준다(검색어 없음 = Idle · 인덱스/완성 진행 = Running · 다 끝남 = Done).
    pub(crate) fn set_search_state(&mut self, st: SearchState) {
        // 완료 시그니처 뒤 사용자가 상자를 다시 만졌으면(포커스·글 변경·초기화) 다음 Running까지 Done = 기본 테두리.
        let st = if st == SearchState::Done && self.done_seen {
            SearchState::Idle
        } else {
            st
        };
        if st == SearchState::Running {
            self.done_seen = false;
        }
        if self.search == st {
            return;
        }
        if self.search == SearchState::Running && st == SearchState::Done {
            // 완료 = 두 번 깜빡임(시각은 다음 tick에서).
            self.blink_pending = true;
            self.blink_start = None;
        }
        if st != SearchState::Done {
            self.blink_start = None;
            self.blink_pending = false;
        }
        self.search = st;
    }

    #[cfg(test)]
    pub(crate) fn search_state(&self) -> SearchState {
        self.search
    }

    /// 완료 시그니처 원복(사용자 09-25): 상자 재포커스 · 글 변경 · 초기화 → 기본 테두리(다음 검색이 Running으로 가면 다시 살아난다).
    fn dismiss_done(&mut self) {
        self.done_seen = true;
        if self.search == SearchState::Done {
            self.search = SearchState::Idle;
            self.blink_start = None;
            self.blink_pending = false;
        }
    }

    fn blinking(&self) -> bool {
        self.blink_pending
            || self
                .blink_start
                .is_some_and(|t0| self.anim_now.saturating_sub(t0) < BLINK_MS * BLINK_PHASES)
    }

    fn search_animating(&self) -> bool {
        self.search == SearchState::Running || self.blinking()
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.search_animating()
            || self.tb.is_animating()
            || self.btns.iter().any(|b| b.hover.is_animating())
            || self.btns.iter().any(|b| {
                b.hover_since
                    .is_some_and(|t0| t0.elapsed().as_millis() < self.tooltip_ms + 50)
            })
    }

    /// 틀 + 텍스트박스 + 토글. `enabled` = false면 흐린 자리 표시만(프로젝트 없음 등).
    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme, enabled: bool) {
        let s = self.scale;
        let px = |v: f32| (v * s).round() as i32;
        let fb = self.frame;
        let r = px(6.0);
        dc.fill_round_rect(fb, r, th.field_bg);
        dc.stroke_round_rect(
            fb,
            r,
            if self.regex_err { th.danger } else { th.border },
            1.0,
        );
        if !enabled {
            let ty = dc.text_center_y(fb.y, fb.h);
            dc.text(fb.x + px(6.0), ty, fb, &self.placeholder, th.text_dim);
            return;
        }
        self.tb.paint(dc, th);
        let tbb = self.tb.bounds();
        if tbb.w > 0 {
            // 텍스트박스 오른쪽 테두리를 틀 배경으로 덮는다(둥근 모서리 안쪽만 · 찾기 막대 `paint_frame`과 같다).
            dc.fill_rect(
                Rect::new(tbb.right() - 2, tbb.y + r, 3, (tbb.h - r * 2).max(0)),
                th.field_bg,
            );
        }
        if self.tb.is_focused() {
            dc.stroke_round_rect(fb, r, th.accent, 1.0);
        }
        self.paint_search(dc, th, fb, r);
        for b in &self.btns {
            b.paint(dc, th, s);
        }
    }

    /// ★ 검색 진행 표시(84 §8): Running = 테두리 둘레를 따라 짧은 밝은 선이 한 바퀴(1.6 s) · Done 직후 = 두 번 깜빡임(강조 2px ↔ 없음) ·
    /// Done 유지 = 완료 테두리(`th.ok`) — 검색어를 지우면 Idle.
    fn paint_search(&self, dc: &mut dyn DrawCtx, th: &Theme, fb: Rect, r: i32) {
        match self.search {
            SearchState::Idle => {}
            SearchState::Running => {
                let path = round_rect_path(fb, r);
                let total = path_len(&path);
                if total <= 0.0 {
                    return;
                }
                // ② 진행 중엔 테두리 전체를 강조색 절반으로 물들여 "일하는 중"이 상자 단위로 읽히게.
                dc.stroke_round_rect(fb, r, th.accent.lerp(th.border, 0.5), 1.0);
                // ① 혜성: 꼬리(배경에 가깝게) → 머리(강조색) 8단계 그라데이션 · 3px · 1.2 s 한 바퀴.
                let t = (self.anim_now % LAP_MS) as f32 / LAP_MS as f32;
                let seg = total * SEG_FRACTION;
                let start = t * total;
                let step = seg / COMET_STEPS as f32;
                for i in 0..COMET_STEPS {
                    let k = (i + 1) as f32 / COMET_STEPS as f32; // 0 = 꼬리 끝 · 1 = 머리
                    let color = th.accent.lerp(th.field_bg, 0.85 * (1.0 - k));
                    let width = 1.5 + 1.5 * k;
                    for pts in path_window(&path, total, start + step * i as f32, step + 0.5) {
                        dc.polyline(&pts, color, width);
                    }
                }
            }
            SearchState::Done => {
                if self.blinking() {
                    let phase = self
                        .blink_start
                        .map_or(0, |t0| self.anim_now.saturating_sub(t0) / BLINK_MS);
                    if phase.is_multiple_of(2) {
                        dc.stroke_round_rect(fb, r, th.accent, 2.0);
                    }
                } else {
                    dc.stroke_round_rect(fb, r, th.ok, 1.0);
                }
            }
        }
    }

    /// 팝업 층 — 텍스트박스 편집 메뉴 + 토글 툴팁(지연 뒤 · 버튼 아래).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.tb.paint_popup(dc, th);
        if let Some((_, r)) = &self.history {
            r.paint_popup(dc, th);
        }
        if let Some(b) = self.btns.iter().find(|b| {
            b.hover_since
                .is_some_and(|t0| t0.elapsed().as_millis() >= self.tooltip_ms)
        }) {
            let anchor = Rect::new(b.rect.x, b.rect.bottom() + (2.0 * self.scale) as i32, 0, 0);
            draw_tooltip_in(
                dc,
                th,
                anchor,
                (0, self.clamp_w),
                t(b.kind.tip()),
                self.scale,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_terms_case_word_regex() {
        let mut f = FilterBar::new("", &[]);
        f.set_text("sel emp");
        assert!(f.matches("SELECT * FROM Emp"));
        assert!(!f.matches("select 1"));
        f.set_checked(BtnKind::Case, true);
        f.refresh();
        assert!(!f.matches("SELECT * FROM Emp"));
        assert!(f.matches("sel emp"));
        f.set_checked(BtnKind::Case, false);
        f.set_checked(BtnKind::Word, true);
        f.set_text("emp");
        assert!(f.matches("from emp where"));
        assert!(!f.matches("employee"));
        f.set_checked(BtnKind::Word, false);
        f.set_checked(BtnKind::Regex, true);
        f.set_text("^q_.*\\.sql$");
        assert!(f.matches("q_lines.sql"));
        assert!(!f.matches("a.sql"));
        f.set_text("(");
        assert!(f.regex_err());
        assert!(!f.matches("("));
        f.set_text("");
        assert!(!f.regex_err());
        assert!(f.matches("anything"));
    }
}

#[cfg(test)]
mod search_anim_tests {
    use super::*;

    /// 둘레 폴리라인 = 네 변 + 네 호(≈ 2πr) · 창은 한 바퀴를 넘으면 둘로 · 길이 0 = 없음.
    #[test]
    fn round_rect_path_len_and_window_wrap() {
        let fb = Rect::new(10, 20, 200, 30);
        let r = 6;
        let path = round_rect_path(fb, r);
        let total = path_len(&path);
        let expect = 2.0 * (200.0 + 30.0) - 8.0 * r as f32 + 2.0 * std::f32::consts::PI * r as f32;
        assert!((total - expect).abs() < 1.5, "둘레 {total} ≈ {expect}");
        let one = path_window(&path, total, 10.0, 40.0);
        assert_eq!(one.len(), 1);
        assert!(one[0].len() >= 2);
        // 끝을 넘어가는 창 = 두 조각.
        let two = path_window(&path, total, total - 5.0, 20.0);
        assert_eq!(two.len(), 2, "한 바퀴를 넘으면 둘로");
        assert!(path_window(&path, total, 0.0, 0.0).is_empty());
        // 시작이 음수/둘레 초과여도 rem_euclid로 안전.
        assert_eq!(path_window(&path, total, -3.0, 10.0).len(), 2);
    }

    /// Running → Done = 두 번 깜빡임(4 위상 × 130 ms) 뒤 완료 테두리 · Idle로 가면 깜빡임 취소 · Running 동안은 늘 애니메이션.
    #[test]
    fn search_state_transitions_and_blink_window() {
        let mut f = FilterBar::new("f", &[]);
        assert!(!f.is_animating());
        f.set_search_state(SearchState::Running);
        assert!(f.is_animating(), "도는 선 = 계속 그린다");
        assert!(f.tick(1000));
        f.set_search_state(SearchState::Done);
        assert!(f.is_animating(), "깜빡임 예약 = 애니메이션");
        assert!(f.tick(1010), "첫 tick에 시작 시각을 잡는다");
        assert!(f.tick(1010 + BLINK_MS * BLINK_PHASES - 1), "네 위상 동안");
        assert!(
            !f.tick(1010 + BLINK_MS * BLINK_PHASES + 1),
            "끝나면 멈춘다(완료 테두리는 정적)"
        );
        assert_eq!(f.search_state(), SearchState::Done);
        f.set_search_state(SearchState::Running);
        f.set_search_state(SearchState::Done);
        f.set_search_state(SearchState::Idle);
        assert!(!f.is_animating(), "Idle = 깜빡임 취소");
        // Idle → Done(진행 없이 바로 끝난 검색) = 깜빡임 없이 완료 테두리만.
        f.set_search_state(SearchState::Done);
        assert!(!f.is_animating());
        assert_eq!(f.search_state(), SearchState::Done);
        // ★ 완료 시그니처 원복(사용자 09-25): 상자를 다시 포커스하면 기본 테두리 · 호스트가 Done을 다시 말해도 그대로 · 다음 Running에서 해제.
        let mut inv = Invalidations::default();
        f.set_bounds(Rect::new(0, 0, 300, 30), 1.0);
        let tbb = f.text_bounds();
        f.on_event(
            &InputEvent::MouseDown {
                x: tbb.x + 5,
                y: tbb.y + tbb.h / 2,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert!(f.is_focused());
        assert_eq!(f.search_state(), SearchState::Idle, "재포커스 = 원복");
        f.set_search_state(SearchState::Done);
        assert_eq!(
            f.search_state(),
            SearchState::Idle,
            "본 시그니처는 다시 켜지지 않는다"
        );
        f.set_search_state(SearchState::Running);
        f.set_search_state(SearchState::Done);
        assert_eq!(
            f.search_state(),
            SearchState::Done,
            "새 검색이 끝나면 다시 시그니처"
        );
    }
}
