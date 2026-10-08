//! ★ 2-pane diff 뷰어(T-283 2단계 · docs/19 §6-2 · 10-09): 왼쪽 = 편집기 문장(정규화) · 오른쪽 = 서버 DDL(정규화) · 행 정렬 =
//! nexa-ctl `diff::align`(맞은편 빈 행 채움) · 줄 배경 = 추가 `ok` · 삭제 `danger` · 변경 `warn`(테마 토큰 · 알파) · 스크롤 동기(한쪽을
//! 움직이면 다른 쪽도 같은 첫 줄) · 머리 줄 = 일치/N군데 다름 + 지금 덩어리 `k/N` · 다음/이전 덩어리(`compare.next`/`compare.prev`).
//! 본문 상자 둘은 읽기 전용 TextBox(선택·복사·검색은 상자 몫) — 같은 부품을 19 §1(두 버퍼 비교)·§3(시점 캐시)도 쓸 수 있게 nexa-sql
//! 안에서는 이 모듈 하나만 안다.

use nexa_ctl::diff::{self, Hunk, Row, RowKind};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};

pub(crate) struct DiffView {
    bounds: Rect,
    scale: f32,
    left: TextBox,
    right: TextBox,
    rows: Vec<Row>,
    hunks: Vec<Hunk>,
    cur: Option<usize>,
    /// 머리 줄 글(일치 / N군데 다름 · 대상 이름).
    head: String,
    /// 왼쪽/오른쪽 라벨(예 "편집기" · "서버").
    labels: (String, String),
    /// 마지막으로 사건을 받은 쪽(스크롤 동기의 주도권).
    last_side: bool,
}

impl Default for DiffView {
    fn default() -> Self {
        let mk = || {
            let mut tb = TextBox::new("").with_multiline();
            tb.set_read_only(true);
            tb
        };
        Self {
            bounds: Rect::default(),
            scale: 1.0,
            left: mk(),
            right: mk(),
            rows: Vec::new(),
            hunks: Vec::new(),
            cur: None,
            head: String::new(),
            labels: (String::new(), String::new()),
            last_side: false,
        }
    }
}

impl DiffView {
    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// 내용을 넣는다(정규화된 줄 목록 둘 · 제목 · 라벨) — 행 정렬 · 양쪽 본문(빈 행 = "") · 색조 · 덩어리.
    pub(crate) fn set(&mut self, a: &[String], b: &[String], obj: &str, labels: (String, String)) {
        self.rows = diff::align(a, b);
        self.hunks = diff::hunks(&self.rows);
        self.cur = (!self.hunks.is_empty()).then_some(0);
        let lt: Vec<&str> = self
            .rows
            .iter()
            .map(|r| r.a.map_or("", |i| a[i].as_str()))
            .collect();
        let rt: Vec<&str> = self
            .rows
            .iter()
            .map(|r| r.b.map_or("", |j| b[j].as_str()))
            .collect();
        self.left.set_text(&lt.join("\n"));
        self.right.set_text(&rt.join("\n"));
        self.left.goto_line(1);
        self.right.goto_line(1);
        self.labels = labels;
        let n = self.hunks.len();
        self.head = if n == 0 {
            tf(Msg::CompareHeadSame, &[obj])
        } else {
            tf(Msg::CompareHeadDiff2, &[obj, &n.to_string()])
        };
    }

    pub(crate) fn hunk_count(&self) -> usize {
        self.hunks.len()
    }

    /// 진단 덤프(`compare.dump`): 행 종류 요약 + 덩어리 범위.
    pub(crate) fn dump(&self) -> String {
        let (mut e, mut i, mut d, mut r) = (0, 0, 0, 0);
        for row in &self.rows {
            match row.kind {
                RowKind::Equal => e += 1,
                RowKind::Insert => i += 1,
                RowKind::Delete => d += 1,
                RowKind::Replace => r += 1,
            }
        }
        let hs: Vec<String> = self
            .hunks
            .iter()
            .map(|h| {
                format!(
                    "rows {}..{} a {}..{} b {}..{}",
                    h.rows.start, h.rows.end, h.a.start, h.a.end, h.b.start, h.b.end
                )
            })
            .collect();
        format!(
            "rows={} equal={e} insert={i} delete={d} replace={r} hunks={} cur={:?}\n{}",
            self.rows.len(),
            self.hunks.len(),
            self.cur,
            hs.join("\n")
        )
    }

    /// 다음/이전 덩어리로(머리 줄 `k/N` · 양쪽을 그 행으로 스크롤).
    pub(crate) fn step(&mut self, forward: bool) -> bool {
        if self.hunks.is_empty() {
            return false;
        }
        let n = self.hunks.len();
        let k = match (self.cur, forward) {
            (None, _) => 0,
            (Some(k), true) => (k + 1) % n,
            (Some(k), false) => (k + n - 1) % n,
        };
        self.cur = Some(k);
        let row = self.hunks[k].rows.start;
        // 덩어리 첫 행이 화면 위에서 3번째쯤 오게.
        let top = row.saturating_sub(3);
        self.left.set_vscroll_top(top);
        self.right.set_vscroll_top(top);
        self.left.goto_line(row + 1);
        self.right.goto_line(row + 1);
        true
    }

    fn tints(&self, th: &Theme) -> Vec<Option<(nexa_ctl::theme::Color, f32)>> {
        self.rows
            .iter()
            .map(|r| match r.kind {
                RowKind::Equal => None,
                RowKind::Insert => Some((th.ok, 0.18)),
                RowKind::Delete => Some((th.danger, 0.18)),
                RowKind::Replace => Some((th.warn, 0.20)),
            })
            .collect()
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.left.set_focused(on && !self.last_side);
        self.right.set_focused(on && self.last_side);
    }

    pub(crate) fn popup_open(&self) -> bool {
        self.left.popup_open() || self.right.popup_open()
    }

    /// 머리 줄 높이.
    fn head_h(&self) -> i32 {
        self.s(24.0)
    }

    fn pane_rects(&self) -> (Rect, Rect) {
        let b = self.bounds;
        let gap = self.s(6.0);
        let y = b.y + self.head_h();
        let h = (b.h - self.head_h()).max(0);
        let w = ((b.w - gap) / 2).max(0);
        (
            Rect::new(b.x, y, w, h),
            Rect::new(b.x + w + gap, y, b.w - w - gap, h),
        )
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        dc.fill_rect(b, th.panel_bg);
        // 머리 줄: 결과 글 + 덩어리 위치 + 라벨.
        dc.select_font(FontSlot::Base, false);
        let hy = dc.text_center_y(b.y, self.head_h());
        let pos = match (self.cur, self.hunks.len()) {
            (Some(k), n) if n > 0 => format!(" · {}/{n}", k + 1),
            _ => String::new(),
        };
        let head = format!("{}{pos} · {}", self.head, t(Msg::CompareHeadKeys));
        let clip = Rect::new(b.x, b.y, b.w, self.head_h());
        dc.text(b.x + self.s(8.0), hy, clip, &head, th.text);
        // 상자 둘(동기 스크롤 · 색조).
        let (lr, rr) = self.pane_rects();
        let tints = self.tints(th);
        let mut inv = Invalidations::default();
        self.left.set_row_tints(tints.clone());
        self.right.set_row_tints(tints);
        self.left.set_bounds(lr, &mut inv);
        self.right.set_bounds(rr, &mut inv);
        // 주도권 쪽의 첫 줄을 다른 쪽에(사건 뒤 · 그리기 직전).
        if self.last_side {
            let top = self.right.vscroll_top();
            self.left.set_vscroll_top(top);
        } else {
            let top = self.left.vscroll_top();
            self.right.set_vscroll_top(top);
        }
        self.left.paint(dc, th);
        self.right.paint(dc, th);
        // 라벨(각 상자 오른쪽 위 · 흐림).
        dc.select_font(FontSlot::Status, false);
        for (r, lab) in [(lr, &self.labels.0), (rr, &self.labels.1)] {
            if lab.is_empty() || r.w <= 0 {
                continue;
            }
            let w = dc.text_width(lab) + self.s(8.0);
            let x = r.right() - self.s(18.0) - w;
            let y = r.y + self.s(4.0);
            let lb = Rect::new(x, y, w, self.s(16.0));
            dc.fill_round_rect_alpha(lb, self.s(3.0), th.panel_bg, 0.85);
            let cy = dc.text_center_y(lb.y, lb.h);
            dc.text(x + self.s(4.0), cy, lb, lab, th.text_dim);
        }
    }

    /// 팝업(우클릭 편집 메뉴)을 맨 위에.
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.left.paint_popup(dc, th);
        self.right.paint_popup(dc, th);
    }

    /// 사건 — 포인터는 그 아래 상자로 · 키는 주도권 쪽으로. 다시 그려야 하면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        let mut inv = Invalidations::default();
        if self.popup_open() {
            if self.left.popup_open() {
                self.left.on_event(ev, &mut inv);
            } else {
                self.right.on_event(ev, &mut inv);
            }
            return true;
        }
        let (lr, rr) = self.pane_rects();
        let at = |x: i32, y: i32| -> Option<bool> {
            let p = Point { x, y };
            if lr.contains(p) {
                Some(false)
            } else if rr.contains(p) {
                Some(true)
            } else {
                None
            }
        };
        match *ev {
            InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } => {
                let Some(side) = at(x, y) else { return false };
                self.last_side = side;
                self.left.set_focused(!side);
                self.right.set_focused(side);
                if side {
                    self.right.on_event(ev, &mut inv);
                } else {
                    self.left.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::MouseMove { x, y } | InputEvent::MouseUp { x, y } => {
                // 드래그 선택은 주도권 쪽이 받는다(상자 밖으로 나가도).
                let side = at(x, y).unwrap_or(self.last_side);
                if side {
                    self.right.on_event(ev, &mut inv);
                } else {
                    self.left.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => {
                if self.last_side {
                    self.right.on_event(ev, &mut inv);
                } else {
                    self.left.on_event(ev, &mut inv);
                }
                true
            }
            _ => {
                if self.last_side {
                    self.right.on_event(ev, &mut inv);
                } else {
                    self.left.on_event(ev, &mut inv);
                }
                true
            }
        }
    }

    /// 휠이 어느 상자 위인지 갱신(호스트가 MouseMove 없이 휠만 줄 때).
    pub(crate) fn note_pointer(&mut self, p: Point) {
        let (lr, rr) = self.pane_rects();
        if lr.contains(p) {
            self.last_side = false;
        } else if rr.contains(p) {
            self.last_side = true;
        }
    }

    /// 읽기 전용 상자의 편집 메뉴 동작(복사 등)을 호스트에 넘긴다.
    pub(crate) fn take_edit_ctx(&mut self) -> Option<nexa_ctl::EditCtxAction> {
        self.left
            .take_edit_ctx()
            .or_else(|| self.right.take_edit_ctx())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    /// 행 정렬 → 양쪽 본문 줄 수 같음 · 덩어리 이동이 순환 · 덤프 요약.
    #[test]
    fn set_and_step() {
        let mut v = DiffView::default();
        v.set(
            &s(&["a", "b", "c"]),
            &s(&["a", "X", "c", "d"]),
            "T",
            ("L".into(), "R".into()),
        );
        // 양쪽 본문 줄 수 같음(맞은편 빈 행 채움 · 끝 빈 행도 세야 하므로 `split`).
        assert_eq!(
            v.left.text().split('\n').count(),
            v.right.text().split('\n').count()
        );
        assert_eq!(v.left.text().split('\n').count(), 4);
        assert_eq!(v.hunk_count(), 2);
        assert_eq!(v.cur, Some(0));
        assert!(v.step(true));
        assert_eq!(v.cur, Some(1));
        assert!(v.step(true));
        assert_eq!(v.cur, Some(0), "순환");
        assert!(v.step(false));
        assert_eq!(v.cur, Some(1));
        assert!(v
            .dump()
            .starts_with("rows=4 equal=2 insert=1 delete=0 replace=1 hunks=2"));
        v.set(&s(&["a"]), &s(&["a"]), "T", ("L".into(), "R".into()));
        assert_eq!(v.hunk_count(), 0);
        assert!(!v.step(true));
    }
}
