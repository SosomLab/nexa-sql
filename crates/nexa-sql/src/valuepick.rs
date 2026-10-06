//! ★ 열 머리 깔때기 → **값 목록 팝업**(T-181 후속 · 10-06 · [77 §2-2](../../../docs/77-data-workbench-architecture.md)) — **엑셀 자동 필터
//! 그대로**(사용자 10-06): 검색 상자(`FilterBar` = Aa·ab·(.*) · 한글 자모) · 목록 맨 위 **(모두 선택)** 3상태 · 고유값 체크 목록(받은 행에서
//! 처음 나온 순서 · NULL 제외 · 상한 `grid.filter_values_max`) · 필터가 없으면 **전부 체크** · 전부 체크 = 필터 없음 · 검색 중에는 맨 위가
//! **(모든 검색 결과 선택)** + **필터에 현재 선택 내용 추가** 체크 — 검색 결과는 처음에 전부 체크되고, 확인 = 추가가 꺼져 있으면
//! 검색 결과의 체크된 값으로 **바꿈** · 켜져 있으면 기존 필터 값에 **더함**. 검색어는 저장하지 않는다(선택 결과만 · 사용자 10-06).
//! 버튼 = [필터 해제] · [확인] [취소]. 적용 = 그 열에 `=`/`IN` 술어 하나(비면 그 열 필터 제거) — 뜻은 호스트(`Grid::set_pick_values`).
//! 팝업 규칙(61 §2-2) = 바깥 클릭은 닫고 **그 클릭을 통과** · Esc 닫기 · Enter 확인 · 세로 = `place_popup`(아래 → 위 → 밀어 넣기) ·
//! 가로 = 열 왼쪽, 넘치면 열 오른쪽에 맞춤.
//!
//! **크기 고정(사용자 10-06)**: 팝업 크기·자리는 **열 때** 정하고 검색으로 바뀌지 않는다 — 결과가 없으면 목록 자리에 "일치하는 값 없음".
//! **검색 속도 설계**: ① 값 수 상한 ② 열 때 소문자 사본(`lower`) + `Matcher::matches_cached` ③ **좁히기**(덧붙인 글자는 앞 결과 안에서만)
//! ④ 그리기 = 보이는 행만. 500값 기준 입력당 수십 µs — 디바운스·스레드는 두지 않는다(39 §3 · 상한이 예산).
//! **스크롤**: 세로 = 휠·↑↓·오른쪽 띠 드래그 · 가로 = 가로 휠(Shift+휠)·아래 띠 드래그(값이 팝업보다 길 때) · 체크 상자는 고정.
use crate::filterbar::{FilterBar, FilterEvent};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Invalidations, Key, Widget};
use nsql_i18n::{t, Msg};
use std::cell::Cell;

/// 팝업이 끝내며 남기는 결과(호스트가 `take_result`로 가져간다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickResult {
    /// 이 열의 값 필터를 `values`로(비면 그 열 필터 제거 = "전부 선택"과 같다).
    Apply { col: usize, values: Vec<String> },
}

/// 표시 행 수 기본(설정 `grid.filter_popup_rows` · 호스트가 `open`에 준다).
const ROWS_DEFAULT: usize = 12;

/// 띠(스크롤 표시) 두께 · 잡는 폭.
const BAR_PX: f32 = 4.0;
const BAR_GRAB_PX: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drag {
    /// 세로 띠 — (누른 y, 그때 scroll 행).
    V(i32, usize),
    /// 가로 띠 — (누른 x, 그때 scroll_x).
    H(i32, i32),
}

/// 목록의 고정 행(스크롤되지 않는 맨 위 줄들).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HeadRow {
    /// (모두 선택) / (모든 검색 결과 선택) — 3상태.
    All,
    /// 필터에 현재 선택 내용 추가(검색 중에만).
    AddToFilter,
}

pub(crate) struct ValuePick {
    open: bool,
    col: usize,
    anchor: Rect,
    rect: Rect,
    host: Rect,
    bar: FilterBar,
    values: Vec<String>,
    /// 값의 소문자 사본(열 때 1회 · 낱말 검색 캐시).
    lower: Vec<String>,
    checked: Vec<bool>,
    /// 열 때의 필터 값(검색 중 "필터에 추가"의 바탕 · 비면 필터 없음).
    base: Vec<String>,
    /// 검색을 시작하기 직전의 체크 상태(검색어를 지우면 되돌린다 · 엑셀과 같다).
    before_search: Option<Vec<bool>>,
    /// 필터에 현재 선택 내용 추가(검색 중에만 뜻이 있다).
    add_to_filter: bool,
    /// 상한을 넘어 목록에 없는 값이 있다.
    more: bool,
    /// 검색을 통과한 값의 index(순서 유지).
    visible: Vec<usize>,
    /// 마지막 검색 글(좁히기 판정).
    last_query: String,
    /// 열 때 정한 목록 행 수(고정 행 포함 · 검색으로 바뀌지 않음).
    rows_fixed: usize,
    scroll: usize,
    scroll_x: i32,
    /// 가장 긴 값의 글 폭(그리기 때 재서 둔다 · 가로 스크롤 범위).
    content_w: Cell<i32>,
    /// hover 행(`visible` index).
    hover: Option<usize>,
    hover_head: Option<HeadRow>,
    drag: Option<Drag>,
    shift: bool,
    scale: f32,
    row_h: i32,
    list: Rect,
    btn_clear: Rect,
    btn_ok: Rect,
    btn_cancel: Rect,
    btn_hot: Option<u8>,
    /// MouseDown으로 누른 버튼(놓을 때 같은 버튼이면 동작 · 사용자 10-06 "마우스업에서 동작").
    pressed: Option<u8>,
    result: Option<PickResult>,
}

impl Default for ValuePick {
    fn default() -> Self {
        ValuePick {
            open: false,
            col: 0,
            anchor: Rect::default(),
            rect: Rect::default(),
            host: Rect::default(),
            bar: FilterBar::new(t(Msg::VpickSearch), &[]),
            values: Vec::new(),
            lower: Vec::new(),
            checked: Vec::new(),
            base: Vec::new(),
            before_search: None,
            add_to_filter: false,
            more: false,
            visible: Vec::new(),
            last_query: String::new(),
            rows_fixed: ROWS_DEFAULT,
            scroll: 0,
            scroll_x: 0,
            content_w: Cell::new(0),
            hover: None,
            hover_head: None,
            drag: None,
            shift: false,
            scale: 1.0,
            row_h: 20,
            list: Rect::default(),
            btn_clear: Rect::default(),
            btn_ok: Rect::default(),
            btn_cancel: Rect::default(),
            btn_hot: None,
            pressed: None,
            result: None,
        }
    }
}

impl ValuePick {
    /// 열 `col`의 값 목록을 `anchor`(열 머리 칸) 아래에 연다 — `chosen` = 지금 걸린 값들(비면 필터 없음 = **전부 체크**) · `rows` =
    /// 표시 행 수(설정 `grid.filter_popup_rows` · 0 = 기본 · 고정 행 포함). 팝업 높이는 값 수와 무관하게 `rows`로 **고정**.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        &mut self,
        col: usize,
        anchor: Rect,
        values: Vec<String>,
        more: bool,
        chosen: &[String],
        host: Rect,
        scale: f32,
        row_h: i32,
        rows: usize,
    ) {
        self.checked = if chosen.is_empty() {
            vec![true; values.len()]
        } else {
            values
                .iter()
                .map(|v| chosen.iter().any(|c| c.eq_ignore_ascii_case(v)))
                .collect()
        };
        self.base = chosen.to_vec();
        self.lower = values.iter().map(|v| v.to_lowercase()).collect();
        self.rows_fixed = if rows == 0 {
            ROWS_DEFAULT
        } else {
            rows.clamp(2, 200)
        };
        self.values = values;
        self.more = more;
        self.col = col;
        self.anchor = anchor;
        self.host = host;
        self.scale = scale;
        self.row_h = row_h.max(16);
        self.bar.set_text("");
        self.bar.set_focused(true);
        self.open = true;
        self.result = None;
        self.btn_hot = None;
        self.pressed = None;
        self.drag = None;
        self.scroll_x = 0;
        self.content_w.set(0);
        self.last_query.clear();
        self.before_search = None;
        self.add_to_filter = false;
        self.visible = (0..self.values.len()).collect();
        self.scroll = 0;
        self.hover = None;
        self.hover_head = None;
        self.layout();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn close(&mut self) {
        if self.open {
            self.open = false;
            self.bar.set_focused(false);
            self.hover = None;
            self.hover_head = None;
            self.drag = None;
        }
    }

    #[cfg(test)]
    pub(crate) fn rect(&self) -> Rect {
        self.rect
    }

    /// 글 입력 상자가 포커스(IME 허용 근거).
    pub(crate) fn wants_ime(&self) -> bool {
        self.open && self.bar.is_focused()
    }

    /// 열려 있으면 검색 상자(호스트의 `focused_textbox` = IME 조합·확정 · 복사/붙여넣기/전체 선택 명령의 대상 · 10-06).
    pub(crate) fn textbox_mut(&mut self) -> Option<&mut nexa_ctl::TextBox> {
        self.open.then(|| self.bar.tb_mut())
    }

    /// 상자 글이 상자 밖 길(IME 확정·조합 중 글자·붙여넣기)로 바뀐 뒤 — 조합 중 글자까지 바로 거른다(다른 검색 상자와 같은 규칙).
    pub(crate) fn query_changed(&mut self) {
        self.bar.refresh();
        self.refilter();
    }

    /// 호스트의 Shift 상태(Shift+세로 휠 = 가로 스크롤 — OS가 HWheel로 바꿔 주지 않는 경우).
    pub(crate) fn set_shift(&mut self, on: bool) {
        self.shift = on;
    }

    pub(crate) fn take_result(&mut self) -> Option<PickResult> {
        self.result.take()
    }

    /// 검색 상자 우클릭 편집 메뉴에서 고른 동작(호스트가 클립보드로 잇는다).
    pub(crate) fn take_edit_ctx(&mut self) -> Option<nexa_ctl::EditCtxAction> {
        self.bar.tb_mut().take_edit_ctx()
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.open && self.bar.tick(now_ms)
    }

    /// 검색 글을 바꾼다(기동 명령 자체 시험용 · 사용자는 상자에 친다).
    pub(crate) fn set_search(&mut self, text: &str) {
        self.bar.set_text(text);
        self.refilter();
    }

    /// 보이는 목록의 `n`번째 값을 토글(자체 시험 · Space).
    pub(crate) fn toggle(&mut self, n: usize) {
        if let Some(&vi) = self.visible.get(n) {
            if let Some(c) = self.checked.get_mut(vi) {
                *c = !*c;
            }
        }
    }

    /// (모두 선택)/(모든 검색 결과 선택) — 보이는 값이 전부 체크면 전부 해제 · 아니면 전부 체크(자체 시험 `grid.vpick.all`).
    pub(crate) fn toggle_all_visible(&mut self) {
        let all = self.all_state() == Some(true);
        for &vi in &self.visible {
            self.checked[vi] = !all;
        }
    }

    /// "필터에 현재 선택 내용 추가" 토글(검색 중에만 · 자체 시험 `grid.vpick.add`).
    pub(crate) fn toggle_add_to_filter(&mut self) {
        if self.searching() {
            self.add_to_filter = !self.add_to_filter;
        }
    }

    fn searching(&self) -> bool {
        !self.bar.is_empty()
    }

    /// [확인]을 누를 수 있는가 — 체크된 값이 하나도 없으면 비활성(엑셀 · 협업 V1 10-06) · "필터에 추가"가 켜져 있으면 기존 값이 있으니 가능.
    fn ok_enabled(&self) -> bool {
        self.add_to_filter
            || (if self.searching() {
                self.visible.iter().any(|&vi| self.checked[vi])
            } else {
                self.checked.iter().any(|&c| c)
            })
    }

    /// 보이는 값의 체크 상태 — Some(true) 전부 · Some(false) 없음 · None 섞임(3상태 상자).
    fn all_state(&self) -> Option<bool> {
        let n = self.visible.iter().filter(|&&vi| self.checked[vi]).count();
        if n == 0 {
            Some(false)
        } else if n == self.visible.len() {
            Some(true)
        } else {
            None
        }
    }

    /// 확인 = 결과를 남기고 닫는다. 검색 중이면 검색 결과의 체크된 값(추가 켜짐 = 기존 필터 값에 더함 · 꺼짐 = 바꿈) · 아니면 체크된
    /// 값 전부 — **전부 체크 = 필터 없음**(빈 목록).
    pub(crate) fn apply(&mut self) {
        if !self.ok_enabled() {
            // 체크된 값이 없으면 확인 무시(버튼 비활성과 같은 판정 · 기동 명령 경로도 · 협업 V1 k2).
            return;
        }
        let mut values: Vec<String> = if self.searching() {
            let sel: Vec<String> = self
                .visible
                .iter()
                .filter(|&&vi| self.checked[vi])
                .map(|&vi| self.values[vi].clone())
                .collect();
            if self.add_to_filter {
                let mut out = self.base.clone();
                for v in sel {
                    if !out.iter().any(|o| o.eq_ignore_ascii_case(&v)) {
                        out.push(v);
                    }
                }
                out
            } else {
                sel
            }
        } else {
            self.values
                .iter()
                .zip(&self.checked)
                .filter(|(_, &c)| c)
                .map(|(v, _)| v.clone())
                .collect()
        };
        if values.len() == self.values.len() && !self.more {
            // 전부 선택 = 필터 없음(엑셀).
            values.clear();
        }
        self.result = Some(PickResult::Apply {
            col: self.col,
            values,
        });
        self.close();
    }

    /// 필터 해제 = 그 열 필터 제거(빈 목록 적용)하고 닫는다.
    fn clear_all(&mut self) {
        self.result = Some(PickResult::Apply {
            col: self.col,
            values: Vec::new(),
        });
        self.close();
    }

    /// 다시 거르기 — 검색어가 앞 것에 **덧붙인** 것이면 앞 결과 안에서만(좁히기) · 아니면 전체. 크기·자리는 그대로.
    /// 검색을 시작하면 체크 상태를 떠 두고 **검색 결과를 전부 체크**(엑셀) · 검색어를 비우면 떠 둔 상태로 되돌린다.
    fn refilter(&mut self) {
        let q = self.bar.text();
        let narrowing = !self.last_query.is_empty()
            && q.len() > self.last_query.len()
            && q.starts_with(&self.last_query);
        let m = self.bar.matcher();
        let pass =
            |i: usize| m.is_empty() || m.matches_cached(&self.lower[i], || self.values[i].clone());
        self.visible = if narrowing {
            self.visible.iter().copied().filter(|&i| pass(i)).collect()
        } else {
            (0..self.values.len()).filter(|&i| pass(i)).collect()
        };
        let was = !self.last_query.is_empty();
        let now = !q.is_empty();
        if now {
            if !was {
                self.before_search = Some(self.checked.clone());
            }
            for &vi in &self.visible {
                self.checked[vi] = true;
            }
        } else if was {
            if let Some(b) = self.before_search.take() {
                self.checked = b;
            }
            self.add_to_filter = false;
        }
        self.last_query = q;
        self.scroll = 0;
        self.hover = None;
    }

    /// 고정 행 수(검색 중 = 2 · 아니면 1).
    fn head_rows(&self) -> usize {
        if self.searching() {
            2
        } else {
            1
        }
    }

    /// 값 행 자리 수(= 고정 행을 뺀 나머지 · 최소 1).
    fn rows_shown(&self) -> usize {
        self.rows_fixed.saturating_sub(self.head_rows()).max(1)
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// 자리·크기 = 열 때 한 번(세로 = 열 머리 아래 → 위 → 밀어 넣기 · 가로 = 열 왼쪽, 넘치면 열 오른쪽에 맞춤) · 행 수 = `rows_fixed`.
    fn layout(&mut self) {
        let pad = self.s(6.0);
        let bar_h = self.s(26.0);
        let btn_h = self.s(22.0);
        let w = self.anchor.w.clamp(self.s(260.0), self.s(380.0));
        let rows = self.rows_fixed as i32 + i32::from(self.more);
        let h = pad + bar_h + pad + rows * self.row_h + pad + btn_h + pad;
        let at = nexa_ctl::geom::place_popup(
            Point {
                x: self.anchor.x,
                y: self.anchor.bottom(),
            },
            (w, h),
            self.host,
        );
        let x = if self.anchor.x + w > self.host.right() {
            (self.anchor.right() - w).max(self.host.x)
        } else {
            self.anchor.x
        };
        self.rect = nexa_ctl::geom::nudge_into(Rect::new(x, at.y, w, h), self.host);
        self.bar.set_bounds(
            Rect::new(self.rect.x + pad, self.rect.y + pad, w - pad * 2, bar_h),
            self.scale,
        );
        self.list = Rect::new(
            self.rect.x + pad,
            self.rect.y + pad + bar_h + pad,
            w - pad * 2,
            self.rows_fixed as i32 * self.row_h,
        );
        let by = self.rect.bottom() - pad - btn_h;
        let bw = self.s(60.0);
        let gap = self.s(6.0);
        // [필터 해제] 왼쪽 · [확인] [취소] 오른쪽 묶음(엑셀 = 확인·취소 · 사용자 10-06).
        self.btn_clear = Rect::new(self.rect.x + pad, by, self.s(72.0), btn_h);
        self.btn_cancel = Rect::new(self.rect.right() - pad - bw, by, bw, btn_h);
        self.btn_ok = Rect::new(self.btn_cancel.x - gap - bw, by, bw, btn_h);
    }

    /// 값 행이 시작하는 y(고정 행 아래).
    fn data_y0(&self) -> i32 {
        self.list.y + self.head_rows() as i32 * self.row_h
    }

    /// 점 아래 고정 행.
    fn head_at(&self, p: Point) -> Option<HeadRow> {
        if !self.list.contains(p) || self.row_h <= 0 {
            return None;
        }
        match ((p.y - self.list.y) / self.row_h, self.searching()) {
            (0, _) => Some(HeadRow::All),
            (1, true) => Some(HeadRow::AddToFilter),
            _ => None,
        }
    }

    /// 점 아래 값 행(`visible` index).
    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list.contains(p) || self.row_h <= 0 || p.y < self.data_y0() {
            return None;
        }
        let i = ((p.y - self.data_y0()) / self.row_h) as usize + self.scroll;
        (i < self.visible.len()).then_some(i)
    }

    fn btn_at(&self, p: Point) -> Option<u8> {
        if self.btn_clear.contains(p) {
            Some(0)
        } else if self.btn_ok.contains(p) {
            Some(1)
        } else if self.btn_cancel.contains(p) {
            Some(2)
        } else {
            None
        }
    }

    fn max_scroll(&self) -> usize {
        self.visible.len().saturating_sub(self.rows_shown())
    }

    fn scroll_by(&mut self, rows: i32) {
        self.scroll = (self.scroll as i32 + rows).clamp(0, self.max_scroll() as i32) as usize;
    }

    /// 가로 스크롤 범위(값 폭 − 글 자리 폭 · 0 이하 = 없음) — 글 자리 = 체크 상자 오른쪽부터 목록 오른쪽까지.
    fn max_scroll_x(&self) -> i32 {
        let text_w = self.list.w - self.text_x0();
        (self.content_w.get() - text_w).max(0)
    }

    /// 글이 시작하는 x(목록 왼쪽 기준 · 체크 상자 + 여백).
    fn text_x0(&self) -> i32 {
        let pad = self.s(6.0);
        pad + self.s(14.0) + pad
    }

    fn scroll_x_by(&mut self, dx: i32) {
        self.scroll_x = (self.scroll_x + dx).clamp(0, self.max_scroll_x());
    }

    /// 값 행 영역(고정 행 아래).
    fn data_rect(&self) -> Rect {
        Rect::new(
            self.list.x,
            self.data_y0(),
            self.list.w,
            self.rows_shown() as i32 * self.row_h,
        )
    }

    /// 세로 띠(트랙 · 썸) — 값이 보이는 행보다 많을 때만.
    fn vbar(&self) -> Option<(Rect, Rect)> {
        let n = self.visible.len();
        let rows = self.rows_shown();
        if n <= rows {
            return None;
        }
        let bw = self.s(BAR_PX);
        let d = self.data_rect();
        let track = Rect::new(d.right() - bw, d.y, bw, d.h);
        let th_h = ((rows as f32 / n as f32) * track.h as f32).max(self.s(12.0) as f32) as i32;
        let th_y = track.y + ((self.scroll as f32 / n as f32) * track.h as f32) as i32;
        let thumb = Rect::new(track.x, th_y, track.w, th_h.min(track.bottom() - th_y));
        Some((track, thumb))
    }

    /// 가로 띠 — 값이 글 자리보다 넓을 때만(목록 아래 가장자리).
    fn hbar(&self) -> Option<(Rect, Rect)> {
        let max = self.max_scroll_x();
        if max <= 0 {
            return None;
        }
        let bw = self.s(BAR_PX);
        let x0 = self.list.x + self.text_x0();
        let track = Rect::new(x0, self.list.bottom() - bw, self.list.right() - x0, bw);
        let total = (self.content_w.get()).max(1) as f32;
        let th_w = ((track.w as f32 / total) * track.w as f32).max(self.s(12.0) as f32) as i32;
        let th_x = track.x + ((self.scroll_x as f32 / total) * track.w as f32) as i32;
        let thumb = Rect::new(th_x, track.y, th_w.min(track.right() - th_x), track.h);
        Some((track, thumb))
    }

    /// 띠 근처(잡는 폭 안)인가 — 드래그 시작 판정.
    fn bar_hit(&self, p: Point) -> Option<Drag> {
        let grab = self.s(BAR_GRAB_PX);
        if let Some((track, _)) = self.vbar() {
            let zone = Rect::new(track.right() - grab, track.y, grab, track.h);
            if zone.contains(p) {
                return Some(Drag::V(p.y, self.scroll));
            }
        }
        if let Some((track, _)) = self.hbar() {
            let zone = Rect::new(track.x, track.bottom() - grab, track.w, grab);
            if zone.contains(p) {
                return Some(Drag::H(p.x, self.scroll_x));
            }
        }
        None
    }

    /// 사건 — 돌려주는 값 = 먹었는가. 바깥 좌/우클릭은 닫고 **false**(호스트가 그 클릭을 그대로 진행 · 팝업 규칙).
    pub(crate) fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> bool {
        if !self.open {
            return false;
        }
        // 검색 상자의 우클릭 편집 메뉴가 떠 있으면(목록 위에 그려진다) 마우스 사건은 전부 상자로 — 목록 행·버튼이 먼저 먹어
        //   항목 클릭이 확정되지 않던 결함(협업 V1 A · 10-06 · 조건 바와 같은 규칙).
        if self.bar.tb_mut().popup_open()
            && matches!(
                ev,
                InputEvent::MouseMove { .. }
                    | InputEvent::MouseDown { .. }
                    | InputEvent::MouseUp { .. }
                    | InputEvent::RightDown { .. }
            )
        {
            self.bar.tb_mut().on_event(ev, inv);
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                match self.drag {
                    Some(Drag::V(y0, s0)) => {
                        let n = self.visible.len().max(1) as f32;
                        let per_row = (self.data_rect().h as f32 / n).max(0.5);
                        let d = ((y - y0) as f32 / per_row).round() as i32;
                        self.scroll = (s0 as i32 + d).clamp(0, self.max_scroll() as i32) as usize;
                    }
                    Some(Drag::H(x0, sx0)) => {
                        let track_w = (self.list.w - self.text_x0()).max(1) as f32;
                        let total = self.content_w.get().max(1) as f32;
                        let d = ((x - x0) as f32 * total / track_w) as i32;
                        self.scroll_x = (sx0 + d).clamp(0, self.max_scroll_x());
                    }
                    None => {
                        self.hover = self.row_at(p);
                        self.hover_head = self.head_at(p);
                        self.btn_hot = self.btn_at(p);
                        self.bar.on_event(ev, inv);
                    }
                }
                true
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if !self.rect.contains(p) && !self.bar.popup_bounds().contains(p) {
                    self.close();
                    return false;
                }
                if let Some(d) = self.bar_hit(p) {
                    self.drag = Some(d);
                } else if let Some(h) = self.head_at(p) {
                    match h {
                        HeadRow::All => self.toggle_all_visible(),
                        HeadRow::AddToFilter => self.toggle_add_to_filter(),
                    }
                } else if let Some(i) = self.row_at(p) {
                    self.toggle(i);
                } else if let Some(b) = self.btn_at(p) {
                    // 버튼은 놓을 때 동작(메뉴 확정 = MouseUp과 같은 규칙 · 사용자 10-06).
                    self.pressed = Some(b);
                } else if self.bar.on_event(ev, inv) == FilterEvent::Changed {
                    self.refilter();
                }
                true
            }
            InputEvent::MouseUp { x, y } => {
                if self.drag.take().is_some() {
                    return true;
                }
                if let Some(b) = self.pressed.take() {
                    if self.btn_at(Point { x, y }) == Some(b) {
                        match b {
                            0 => self.clear_all(),
                            1 if self.ok_enabled() => self.apply(),
                            1 => {}
                            _ => self.close(),
                        }
                    }
                    return true;
                }
                self.bar.on_event(ev, inv);
                true
            }
            InputEvent::RightDown { x, y } => {
                let p = Point { x, y };
                if !self.rect.contains(p) {
                    self.close();
                    return false;
                }
                // 검색 상자 안 우클릭 = 상자 편집 메뉴(복사·잘라내기·붙여넣기·전체 선택 · 필터 틀은 이력 드롭다운이 열려 있을
                //   때만 상자에 넘기므로 여기서 직접 · 협업 V1 ③ 10-06).
                if self.bar.tb_mut().bounds().contains(p) {
                    self.bar.tb_mut().on_event(ev, inv);
                } else {
                    self.bar.on_event(ev, inv);
                }
                true
            }
            InputEvent::Wheel { delta } => {
                if self.shift {
                    self.scroll_x_by(if delta < 0 { 1 } else { -1 } * self.s(24.0));
                } else {
                    self.scroll_by(if delta < 0 { 3 } else { -3 });
                }
                true
            }
            InputEvent::HWheel { delta } => {
                self.scroll_x_by(if delta > 0 { 1 } else { -1 } * self.s(24.0));
                true
            }
            InputEvent::Key {
                key: Key::Escape, ..
            } => {
                self.close();
                true
            }
            InputEvent::Key {
                key: Key::Enter, ..
            } => {
                if self.ok_enabled() {
                    self.apply();
                }
                true
            }
            InputEvent::Key { key: Key::Down, .. } => {
                let n = self.visible.len();
                if n > 0 {
                    let i = self.hover.map_or(0, |h| (h + 1).min(n - 1));
                    self.hover = Some(i);
                    if i >= self.scroll + self.rows_shown() {
                        self.scroll = i + 1 - self.rows_shown();
                    }
                }
                true
            }
            InputEvent::Key { key: Key::Up, .. } => {
                if let Some(h) = self.hover {
                    let i = h.saturating_sub(1);
                    self.hover = Some(i);
                    if i < self.scroll {
                        self.scroll = i;
                    }
                }
                true
            }
            InputEvent::Key {
                key: Key::Space, ..
            } if self.hover.is_some() && self.bar.is_empty() => {
                if let Some(h) = self.hover {
                    self.toggle(h);
                }
                true
            }
            _ => {
                if self.bar.on_event(ev, inv) == FilterEvent::Changed {
                    self.refilter();
                }
                true
            }
        }
    }

    /// 체크 상자 하나(3상태 = `None` 섞임) — 글꼴 글리프 없이 도형으로.
    fn draw_check(&self, dc: &mut dyn DrawCtx, th: &Theme, bx: Rect, state: Option<bool>) {
        match state {
            Some(true) => {
                dc.fill_round_rect(bx, self.s(3.0), th.accent);
                check_mark(dc, bx, th.panel_bg);
            }
            Some(false) => dc.stroke_round_rect(bx, self.s(3.0), th.text_dim, 1.0),
            None => {
                dc.fill_round_rect(bx, self.s(3.0), th.accent);
                let inset = (bx.w / 4).max(2);
                dc.fill_rect(
                    Rect::new(
                        bx.x + inset,
                        bx.y + inset,
                        bx.w - inset * 2,
                        bx.h - inset * 2,
                    ),
                    th.panel_bg,
                );
            }
        }
    }

    /// 팝업 층 그리기(호스트의 메뉴 뒤 · 맨 마지막).
    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.open {
            return;
        }
        let pad = self.s(6.0);
        let r = self.rect;
        dc.fill_round_rect(r, self.s(6.0), th.panel_bg);
        dc.stroke_round_rect(r, self.s(6.0), th.border, 1.0);
        self.bar.paint(dc, th, true);
        dc.select_font(FontSlot::Base, false);
        // 값 폭(가로 스크롤 범위) — 목록 전체 중 최대를 한 번 재 둔다(값 수 = 상한 안).
        if self.content_w.get() == 0 {
            let w = self
                .values
                .iter()
                .map(|v| dc.text_width(v))
                .max()
                .unwrap_or(0);
            self.content_w.set(w + pad);
        }
        let cb = self.s(14.0);
        let tx0 = self.list.x + self.text_x0();
        // ── 고정 행: (모두 선택)/(모든 검색 결과 선택) · 필터에 현재 선택 내용 추가.
        let searching = self.searching();
        let heads: Vec<(HeadRow, &str, Option<bool>)> = if searching {
            vec![
                (HeadRow::All, t(Msg::VpickAllResults), self.all_state()),
                (
                    HeadRow::AddToFilter,
                    t(Msg::VpickAddToFilter),
                    Some(self.add_to_filter),
                ),
            ]
        } else {
            vec![(HeadRow::All, t(Msg::VpickAll), self.all_state())]
        };
        for (k, (row_kind, label, state)) in heads.into_iter().enumerate() {
            let ry = self.list.y + k as i32 * self.row_h;
            let row = Rect::new(self.list.x, ry, self.list.w, self.row_h);
            if self.hover_head == Some(row_kind) {
                dc.fill_rect_alpha(row, th.sel_bg, 0.35);
            }
            let bx = Rect::new(row.x + pad, ry + (self.row_h - cb) / 2, cb, cb);
            // 검색 결과가 없으면 상자는 비활성처럼 흐리게.
            if self.visible.is_empty() && row_kind == HeadRow::All {
                dc.stroke_round_rect(bx, self.s(3.0), th.text_dim, 1.0);
            } else {
                self.draw_check(dc, th, bx, state);
            }
            let ty = dc.text_center_y(ry, self.row_h);
            dc.text(
                tx0,
                ty,
                Rect::new(tx0, ry, (row.right() - tx0 - pad).max(0), self.row_h),
                label,
                th.text,
            );
        }
        // 고정 행 아래 가는 구분선.
        let dy0 = self.data_y0();
        dc.fill_rect_alpha(
            Rect::new(self.list.x, dy0 - 1, self.list.w, 1),
            th.text_dim,
            0.25,
        );
        // ── 값 행.
        let rows = self.rows_shown();
        if self.visible.is_empty() {
            // 결과 없음 = 값 자리에 안내만 · 크기는 그대로(사용자 10-06).
            let ty = dc.text_center_y(dy0, self.row_h);
            dc.text(
                self.list.x + pad,
                ty,
                self.data_rect(),
                t(Msg::VpickNone),
                th.text_dim,
            );
        }
        for (i, &vi) in self.visible.iter().enumerate().skip(self.scroll).take(rows) {
            let ry = dy0 + (i - self.scroll) as i32 * self.row_h;
            let row = Rect::new(self.list.x, ry, self.list.w, self.row_h);
            if self.hover == Some(i) {
                dc.fill_rect_alpha(row, th.sel_bg, 0.35);
            }
            let bx = Rect::new(row.x + pad, ry + (self.row_h - cb) / 2, cb, cb);
            self.draw_check(dc, th, bx, Some(self.checked[vi]));
            // 글은 가로 스크롤을 따라(체크 상자는 고정) · 클립 = 글 자리.
            let clip = Rect::new(tx0, ry, (row.right() - tx0 - pad).max(0), self.row_h);
            let ty = dc.text_center_y(ry, self.row_h);
            dc.text(tx0 - self.scroll_x, ty, clip, &self.values[vi], th.text);
        }
        if self.more {
            let ry = self.list.bottom();
            let ty = dc.text_center_y(ry, self.row_h);
            dc.text(
                self.list.x + pad,
                ty,
                Rect::new(self.list.x, ry, self.list.w, self.row_h),
                t(Msg::VpickMore),
                th.text_dim,
            );
        }
        // 스크롤 띠(세로 = 오른쪽 · 가로 = 아래) — 필요할 때만 · 드래그 중 진하게.
        if let Some((track, thumb)) = self.vbar() {
            dc.fill_rect_alpha(track, th.text_dim, 0.15);
            let a = if matches!(self.drag, Some(Drag::V(..))) {
                0.9
            } else {
                0.6
            };
            dc.fill_round_rect_alpha(thumb, self.s(1.0), th.accent, a);
        }
        if let Some((track, thumb)) = self.hbar() {
            dc.fill_rect_alpha(track, th.text_dim, 0.15);
            let a = if matches!(self.drag, Some(Drag::H(..))) {
                0.9
            } else {
                0.6
            };
            dc.fill_round_rect_alpha(thumb, self.s(1.0), th.accent, a);
        }
        // 버튼 = [필터 해제] · [확인] [취소] — 머티리얼 텍스트 버튼(확인 = 강조 채움).
        dc.select_font(FontSlot::Status, false);
        for (n, (br, label)) in [
            (self.btn_clear, t(Msg::VpickClear)),
            (self.btn_ok, t(Msg::VpickApply)),
            (self.btn_cancel, t(Msg::VpickCancel)),
        ]
        .into_iter()
        .enumerate()
        {
            let hot = self.btn_hot == Some(n as u8);
            let primary = n == 1;
            if primary && !self.ok_enabled() {
                // 체크된 값이 없으면 [확인] 비활성(엑셀).
                dc.fill_round_rect_alpha(br, self.s(3.0), th.text_dim, 0.18);
                let tw = dc.text_width(label);
                let ty = dc.text_center_y(br.y, br.h);
                dc.text(br.x + (br.w - tw) / 2, ty, br, label, th.text_dim);
                continue;
            }
            if primary {
                dc.fill_round_rect_alpha(br, self.s(3.0), th.accent, if hot { 1.0 } else { 0.85 });
            } else {
                dc.fill_round_rect_alpha(br, self.s(3.0), th.accent, if hot { 0.3 } else { 0.14 });
            }
            let tw = dc.text_width(label);
            let ty = dc.text_center_y(br.y, br.h);
            dc.text(
                br.x + (br.w - tw) / 2,
                ty,
                br,
                label,
                if primary { th.panel_bg } else { th.text },
            );
        }
        dc.select_font(FontSlot::Base, false);
        self.bar.paint_popup(dc, th);
    }

    /// 자체 시험 덤프: 첫 줄 = `col=… search=… visible=n/N more=… rect=x,y,w,h scroll=행,px all=… add=…` · 다음 줄부터 보이는 값 `[x] 값`.
    pub(crate) fn dump(&self) -> String {
        if !self.open {
            return String::from("closed");
        }
        let all = match self.all_state() {
            Some(true) => "on",
            Some(false) => "off",
            None => "mixed",
        };
        let mut out = format!(
            "col={} search={} visible={}/{} more={} rect={},{},{},{} scroll={},{} all={} add={}",
            self.col,
            self.bar.text(),
            self.visible.len(),
            self.values.len(),
            self.more,
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
            self.scroll,
            self.scroll_x,
            all,
            self.add_to_filter
        );
        for &vi in &self.visible {
            out.push('\n');
            out.push_str(if self.checked[vi] { "[x] " } else { "[ ] " });
            out.push_str(&self.values[vi]);
        }
        out
    }
}

/// 체크 표시(✓) — 글꼴 글리프 없이 상자 안에 짧은 선 둘(3-OS 동일).
fn check_mark(dc: &mut dyn DrawCtx, bx: Rect, color: nexa_ctl::Color) {
    let t = (bx.w as f32 / 7.0).max(1.0) as i32;
    let (x0, y0) = (bx.x + bx.w * 22 / 100, bx.y + bx.h * 50 / 100);
    let (x1, y1) = (bx.x + bx.w * 42 / 100, bx.y + bx.h * 70 / 100);
    let (x2, y2) = (bx.x + bx.w * 78 / 100, bx.y + bx.h * 30 / 100);
    for (ax, ay, bxx, byy) in [(x0, y0, x1, y1), (x1, y1, x2, y2)] {
        let n = (bxx - ax).abs().max((byy - ay).abs()).max(1);
        for i in 0..=n {
            let x = ax + (bxx - ax) * i / n;
            let y = ay + (byy - ay) * i / n;
            dc.fill_rect(Rect::new(x, y - t / 2, t, t), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(values: &[&str], chosen: &[&str]) -> ValuePick {
        let mut v = ValuePick::default();
        v.open(
            3,
            Rect::new(100, 40, 120, 20),
            values.iter().map(|s| s.to_string()).collect(),
            false,
            &chosen.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            Rect::new(0, 0, 1000, 800),
            1.0,
            20,
            0,
        );
        v
    }

    fn down(x: i32, y: i32) -> InputEvent {
        InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        }
    }

    /// 엑셀 기본: 필터 없음 = 전부 체크 · (모두 선택) = on · 하나 끄면 mixed · 확인 = 체크된 값만(전부면 필터 없음 = 빈 목록).
    #[test]
    fn excel_defaults_all_checked_and_tristate() {
        let mut v = vp(&["1", "2", "3", "4"], &[]);
        assert!(v.dump().contains("all=on"));
        assert!(v.checked.iter().all(|&c| c));
        v.apply();
        assert_eq!(
            v.take_result(),
            Some(PickResult::Apply {
                col: 3,
                values: vec![]
            }),
            "전부 체크 = 필터 없음"
        );
        let mut v = vp(&["1", "2", "3", "4"], &[]);
        v.toggle(1);
        v.toggle(2);
        assert!(v.dump().contains("all=mixed"));
        v.apply();
        assert_eq!(
            v.take_result(),
            Some(PickResult::Apply {
                col: 3,
                values: vec!["1".into(), "4".into()]
            })
        );
        // 필터가 있으면 그 값만 체크 · (모두 선택) 클릭 = 전부 체크 → 다시 클릭 = 전부 해제 → [확인] 비활성(Enter도 무시).
        let mut v = vp(&["1", "2", "3", "4"], &["1", "4"]);
        assert_eq!(v.checked, vec![true, false, false, true]);
        v.toggle_all_visible();
        assert!(v.checked.iter().all(|&c| c));
        v.toggle_all_visible();
        assert!(v.checked.iter().all(|&c| !c));
        assert!(!v.ok_enabled());
        let mut inv = Invalidations::default();
        v.on_event(
            &InputEvent::Key {
                key: Key::Enter,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert!(v.is_open() && v.result.is_none(), "체크 없음 = 확인 무시");
        v.set_search("2");
        v.toggle_add_to_filter();
        assert!(v.ok_enabled(), "추가 켜짐 = 기존 값으로 확인 가능");
    }

    /// 엑셀 검색: 검색하면 결과는 전부 체크 · 확인(추가 끔) = 검색 결과로 **바꿈** · 추가 켬 = 기존 필터에 **더함** · 검색어를 지우면 체크 복원.
    #[test]
    fn excel_search_replace_or_add() {
        let mut v = vp(&["1", "2", "3", "4"], &["1", "4"]);
        v.set_search("3");
        assert_eq!(v.visible, vec![2]);
        assert!(v.checked[2], "검색 결과는 전부 체크");
        assert!(v.dump().contains("all=on") && v.dump().contains("add=false"));
        v.apply();
        assert_eq!(
            v.take_result(),
            Some(PickResult::Apply {
                col: 3,
                values: vec!["3".into()]
            }),
            "추가 끔 = 바꿈"
        );
        let mut v = vp(&["1", "2", "3", "4"], &["1", "4"]);
        v.set_search("3");
        v.toggle_add_to_filter();
        assert!(v.dump().contains("add=true"));
        v.apply();
        assert_eq!(
            v.take_result(),
            Some(PickResult::Apply {
                col: 3,
                values: vec!["1".into(), "4".into(), "3".into()]
            }),
            "추가 켬 = 기존 + 검색 선택"
        );
        // 검색어를 지우면 검색 전 체크로 돌아간다.
        let mut v = vp(&["1", "2", "3", "4"], &["1", "4"]);
        v.set_search("2");
        assert!(v.checked[1]);
        v.set_search("");
        assert_eq!(v.checked, vec![true, false, false, true]);
        assert!(v.dump().contains("add=false"));
    }

    /// 고정 행 클릭 = (모두 선택) 토글 · 검색 중 둘째 행 = 추가 토글 · 값 행은 고정 행 아래부터.
    #[test]
    fn head_rows_click() {
        let mut v = vp(&["a", "b"], &[]);
        let mut inv = Invalidations::default();
        let (lx, ly) = (v.list.x + 10, v.list.y + 5);
        v.on_event(&down(lx, ly), &mut inv);
        assert!(
            v.checked.iter().all(|&c| !c),
            "(모두 선택) 클릭 = 전부 해제"
        );
        v.on_event(&down(lx, ly + v.row_h), &mut inv);
        assert_eq!(v.checked, vec![true, false], "첫 값 행 토글");
        v.set_search("a");
        v.on_event(&down(lx, ly + v.row_h), &mut inv);
        assert!(v.add_to_filter, "검색 중 둘째 고정 행 = 추가 토글");
        assert_eq!(v.rows_shown(), ROWS_DEFAULT - 2);
    }

    /// ★ 크기 고정: 검색으로 결과가 0이 되어도 rect·목록 자리는 그대로 · 좁히기 · 바꿔 치기.
    #[test]
    fn size_fixed_and_narrowing() {
        let mut v = vp(&["alpha", "alpine", "beta", "album", "gamma"], &[]);
        let r0 = v.rect();
        let l0 = v.list;
        v.set_search("al");
        assert_eq!(v.visible, vec![0, 1, 3]);
        v.set_search("alp");
        assert_eq!(v.visible, vec![0, 1]);
        v.set_search("alpx");
        assert!(v.visible.is_empty());
        assert_eq!((v.rect(), v.list), (r0, l0));
        v.set_search("be");
        assert_eq!(v.visible, vec![2]);
        v.set_search("");
        assert_eq!(v.visible.len(), 5);
    }

    /// 글자 입력 = 검색 상자로(열면 상자가 포커스) · 한글 자모 · 대소문자 무시 · Backspace.
    #[test]
    fn typing_goes_to_search_box() {
        let mut v = vp(&["김철수", "lee", "Kim"], &[]);
        let mut inv = Invalidations::default();
        assert!(v.wants_ime());
        assert!(v.on_event(&InputEvent::Char { c: 'k', now_ms: 0 }, &mut inv));
        assert_eq!(v.bar.text(), "k");
        assert_eq!(v.visible, vec![2]);
        v.set_search("ㄱ");
        assert_eq!(v.visible, vec![0], "자모 ㄱ = 김철수");
        v.set_search("k");
        assert!(v.on_event(
            &InputEvent::Char {
                c: '\u{8}',
                now_ms: 0
            },
            &mut inv
        ));
        assert_eq!(v.bar.text(), "");
        assert_eq!(v.visible.len(), 3);
    }

    /// 세로 스크롤 = 휠 3행 · 범위 안 · 가로 스크롤은 값 폭이 글 자리보다 넓을 때만.
    #[test]
    fn scrolling() {
        let vals: Vec<String> = (0..30).map(|i| format!("v{i:02}")).collect();
        let mut v = vp(&vals.iter().map(String::as_str).collect::<Vec<_>>(), &[]);
        let rows = v.rows_shown();
        let mut inv = Invalidations::default();
        assert!(v.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv));
        assert_eq!(v.scroll, 3);
        for _ in 0..20 {
            v.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv);
        }
        assert_eq!(v.scroll, 30 - rows, "끝에서 멈춤");
        assert!(v.vbar().is_some());
        v.on_event(&InputEvent::HWheel { delta: 120 }, &mut inv);
        assert_eq!(v.scroll_x, 0);
        v.content_w.set(v.list.w + 100);
        v.on_event(&InputEvent::HWheel { delta: 120 }, &mut inv);
        assert_eq!(v.scroll_x, 24);
        assert!(v.hbar().is_some());
    }

    /// 바깥 클릭 = 닫고 통과(false) · Esc = 닫기 · 자리 = 열 머리 아래.
    #[test]
    fn outside_click_passes_through_and_esc_closes() {
        let mut v = vp(&["a", "b"], &[]);
        assert_eq!(v.rect().y, 60);
        let mut inv = Invalidations::default();
        assert!(!v.on_event(&down(900, 700), &mut inv));
        assert!(!v.is_open());
        let mut v = vp(&["a", "b"], &[]);
        assert!(v.on_event(
            &InputEvent::Key {
                key: Key::Escape,
                shift: false,
                primary: false
            },
            &mut inv
        ));
        assert!(!v.is_open() && v.take_result().is_none());
    }

    /// 오른쪽 끝 열 = 팝업 오른쪽을 열 오른쪽에 맞춘다 · 값이 하나여도 표시 행 수는 설정대로 고정.
    #[test]
    fn right_edge_column_aligns_right() {
        let mut v = ValuePick::default();
        v.open(
            26,
            Rect::new(900, 40, 100, 20),
            vec!["a".into()],
            false,
            &[],
            Rect::new(0, 0, 1000, 800),
            1.0,
            20,
            5,
        );
        let r = v.rect();
        assert_eq!(r.right(), 1000, "{r:?}");
        assert_eq!(r.y, 60);
        assert_eq!(v.rows_fixed, 5);
    }

    /// 버튼은 MouseUp에서(같은 버튼 위에서 놓을 때만) · [취소] = 결과 없이 닫힘 · [필터 해제] = 빈 목록 · 배치 = 해제 왼쪽 · 확인·취소 오른쪽.
    #[test]
    fn buttons_act_on_mouse_up() {
        let mut v = vp(&["a", "b"], &[]);
        let mut inv = Invalidations::default();
        let (ax, ay) = (v.btn_ok.x + 2, v.btn_ok.y + 2);
        v.on_event(&down(ax, ay), &mut inv);
        assert!(v.is_open() && v.result.is_none());
        v.on_event(&InputEvent::MouseUp { x: ax - 300, y: ay }, &mut inv);
        assert!(v.is_open(), "다른 곳에서 놓으면 없음");
        v.on_event(&down(ax, ay), &mut inv);
        v.on_event(&InputEvent::MouseUp { x: ax, y: ay }, &mut inv);
        assert!(!v.is_open() && v.take_result().is_some());
        let mut v = vp(&["a", "b"], &["a"]);
        let (cx, cy) = (v.btn_cancel.x + 2, v.btn_cancel.y + 2);
        v.on_event(&down(cx, cy), &mut inv);
        v.on_event(&InputEvent::MouseUp { x: cx, y: cy }, &mut inv);
        assert!(
            !v.is_open() && v.take_result().is_none(),
            "취소 = 결과 없음"
        );
        let mut v = vp(&["a", "b"], &["a"]);
        let (kx, ky) = (v.btn_clear.x + 2, v.btn_clear.y + 2);
        v.on_event(&down(kx, ky), &mut inv);
        v.on_event(&InputEvent::MouseUp { x: kx, y: ky }, &mut inv);
        assert_eq!(
            v.take_result(),
            Some(PickResult::Apply {
                col: 3,
                values: vec![]
            })
        );
        let v = vp(&["a"], &[]);
        assert!(v.btn_clear.right() < v.btn_ok.x && v.btn_ok.right() < v.btn_cancel.x);
        assert_eq!(v.btn_cancel.right(), v.rect.right() - 6);
    }
}
