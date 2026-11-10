//! App — git 덩어리 diff 팝업(C-1 후속 · docs/19 §2 M3 "hunk diff 팝업" · 11-10 1차): 거터 git 띠의 덩어리를 **좌클릭**하면 그
//! 덩어리의 HEAD/작업본 조각(문맥 `editor.git_context`줄)을 편집기 위 팝업(2-pane [`diff_view::DiffView`])으로 보인다.
//! 설정 `editor.git_popup`(기본 on) · Esc·바깥 클릭 = 닫기(팝업 비트 512 · 배타 규칙 61 §2-2-b) · 팝업 안 포인터·휠은 뷰어가 받는다.
//! 되돌리기(revert hunk) 버튼 · 상태줄 ahead/behind는 2차(19 §2 나머지).

use crate::*;
use nexa_ctl::diff;
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::place_popup_beside;
use nexa_ctl::theme::Theme;
use std::ops::Range;

/// 전체 정렬(LCS)의 상한 — 이보다 긴 본문은 팝업 대신 상태줄 안내(클릭 때만 도는 계산이지만 큰 파일은 피한다).
const MAX_LINES: usize = 20_000;
/// 팝업에 보이는 최대 행(그 이상은 뷰어 안에서 스크롤).
const MAX_ROWS: usize = 14;

/// 편집기 위에 뜨는 덩어리 diff 팝업(앱 필드 `githunk`).
#[derive(Default)]
pub(crate) struct GitHunkPopup {
    view: diff_view::DiffView,
    rect: Rect,
    open: bool,
}

impl GitHunkPopup {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// 열려 있을 때의 사각형(배타 규칙 `menu_hit_bits` · 닫혀 있으면 빈 사각형).
    pub(crate) fn bounds(&self) -> Rect {
        if self.open {
            self.rect
        } else {
            Rect::default()
        }
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.view.set_focused(false);
    }

    /// 팝업 층 그리기(테두리 1px + 뷰어 + 뷰어의 편집 메뉴).
    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.open {
            return;
        }
        let b = self.rect;
        dc.fill_rect(Rect::new(b.x - 1, b.y - 1, b.w + 2, b.h + 2), th.border);
        self.view.paint(dc, th);
        self.view.paint_popup(dc, th);
    }
}

/// 클릭한 작업본 줄 `line`이 든 덩어리의 **정렬 행 구간**(전체 정렬 `rows` 기준 · 문맥 `ctx`행을 앞뒤에 붙여 `n_rows`로 자른다).
/// 삭제만 있는 덩어리(작업본 줄 0개 · 거터는 그 **아래 줄**에 쐐기)는 `b.start == line`인 것. 없으면 None.
/// ★ 2차(11-10 협업 관찰 ①): 줄 범위를 양쪽에서 따로 잘라 다시 정렬하면 이웃 덩어리의 지운 줄이 Replace로 묶여 색이 달라졌다 →
///   전체 정렬의 **행**을 그대로 쓴다.
/// ★ 3차(협업 V1 11-10 b ◐): 문맥은 **Equal 행** 기준으로 센다 — 덩어리에 바로 이어진 비-Equal 행(이웃 덩어리 · 지운 줄)은 문맥
///   예산을 먹지 않고 함께 보인다(git의 덩어리 병합과 같다). 종전 = 행 수로 세어 변경 띠 팝업이 지운 줄에서 끝났다.
pub(crate) fn hunk_rows_at(
    rows: &[diff::Row],
    hunks: &[diff::Hunk],
    line: usize,
    ctx: usize,
) -> Option<Range<usize>> {
    // 덩어리 판정: 작업본 줄이 든 것 · 또는 그 줄 **바로 위에서 끝나며 지운 줄을 가진** 것(쐐기 = 삭제만인 덩어리 · 변경 뒤에 이어진
    //   삭제가 한 덩어리로 묶인 경우 둘 다 · 3차).
    let h = hunks.iter().find(|h| {
        (h.b.start <= line && line < h.b.end) || (h.b.end == line && h.a.len() > h.b.len())
    })?;
    let eq = |i: usize| rows[i].kind == diff::RowKind::Equal;
    let mut start = h.rows.start.min(rows.len());
    let mut n = 0;
    while start > 0 && n < ctx {
        start -= 1;
        if eq(start) {
            n += 1;
        }
    }
    let mut end = h.rows.end.min(rows.len());
    n = 0;
    while end < rows.len() && n < ctx {
        if eq(end) {
            n += 1;
        }
        end += 1;
    }
    Some(start..end)
}

/// 행의 a↔b를 맞바꾼다(팝업은 왼쪽 = 편집기 · 오른쪽 = HEAD인데 전체 정렬은 a = HEAD · b = 편집기로 했다).
fn flip_row(r: &diff::Row) -> diff::Row {
    diff::Row {
        a: r.b,
        b: r.a,
        kind: match r.kind {
            diff::RowKind::Insert => diff::RowKind::Delete,
            diff::RowKind::Delete => diff::RowKind::Insert,
            k => k,
        },
    }
}

impl App {
    /// 거터 git 띠 좌클릭 → 덩어리 팝업. 띠 위가 아니면 false(그 클릭은 편집기 몫).
    pub(crate) fn git_hunk_click(&mut self, p: Point) -> bool {
        if !self.settings.flag("editor.git_popup") || self.editors.active_view().is_some() {
            return false;
        }
        let i = self.editors.active();
        let Some(tb) = self.editors.tab_box(i) else {
            return false;
        };
        let Some(line) = tb.git_band_line_at(p) else {
            return false;
        };
        let lh = tb.line_h().max(1);
        let work = tb.text();
        let Some(head) = self.git.info().and_then(|g| g.head.clone()) else {
            return false;
        };
        let a: Vec<String> = head.lines().map(str::to_string).collect();
        let b: Vec<String> = work.lines().map(str::to_string).collect();
        if a.len().max(b.len()) > MAX_LINES {
            self.sess.status = t(Msg::StGitPopupTooBig).into();
            self.redraw();
            return true;
        }
        let rows = diff::align(&a, &b);
        let hunks = diff::hunks(&rows);
        let ctx = self.settings.int("editor.git_context").clamp(0, 10) as usize;
        let Some(win) = hunk_rows_at(&rows, &hunks, line, ctx) else {
            self.sess.status = t(Msg::StGitNoHunk).into();
            self.redraw();
            return true;
        };
        self.close_context_menus();
        // ★ 왼쪽 = 편집기(현재) · 오른쪽 = HEAD(협업 V1 11-10 관찰 (1)(2)): DiffView 색 규칙이 "a에만 = 초록 · b에만 = 빨강"이라
        //   편집기를 a에 두어야 추가 줄 = 초록 · HEAD에만(지운 줄) = 빨강으로 범례와 맞는다. 전체 정렬 행을 뒤집어 **그대로** 넣는다
        //   (2차 · 관찰 ①: 조각을 다시 정렬하면 분류가 달라졌다).
        let slice: Vec<diff::Row> = rows[win].iter().map(flip_row).collect();
        let (sp, font_px) = self.editors.line_spacing();
        self.githunk.view.set_line_spacing(sp, font_px);
        self.githunk.view.set_rows(
            slice,
            &b,
            &a,
            "",
            (t(Msg::CompareLabelCurrent).to_string(), "HEAD".to_string()),
        );
        let n = self.githunk.view.hunk_count();
        self.githunk.view.set_head(tf(
            Msg::GitHunkHeadDiff,
            &[&(line + 1).to_string(), &n.to_string()],
        ));
        // 크기 = 편집기 폭 80%(최소 320) × (정렬 행 수(≤14) × 줄 높이 + 머리 42) · 자리 = 클릭한 줄을 가리지 않게 아래/위 · 창 안.
        let ed = self.editors.editor_bounds();
        let s = |v: f32| (v * self.scale).round() as i32;
        let rows_n = self.githunk.view.row_count().clamp(1, MAX_ROWS) as i32;
        let w = (ed.w * 4 / 5).max(s(320.0));
        // 상자 안쪽 여백 12(위 8 + 아래 4) · 줄 높이는 팝업 칸도 편집기와 같은 줄 간격이라 같다(2차 · 관찰 ② = 종전엔 칸이 Auto 간격 +
        //   여백 8이라 2~3행이 잘렸다).
        let h = rows_n * lh + s(42.0) + s(12.0);
        let host = self
            .window
            .as_ref()
            .map(|win| {
                let sz = win.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(ed);
        let avoid = Rect::new(ed.x, p.y - lh / 2, ed.w, lh);
        let pos = place_popup_beside(p, avoid, (w, h), host);
        self.githunk.rect = Rect::new(pos.x, pos.y, w, h);
        self.githunk.view.set_bounds(self.githunk.rect, self.scale);
        self.githunk.open = true;
        self.githunk.view.set_focused(true);
        self.redraw();
        true
    }

    /// 열린 덩어리 팝업의 사건: Esc = 닫기 · 팝업 안 포인터·휠 = 뷰어 · 바깥 클릭은 배타 규칙(`ctxmenu_exclusive_step`)이 닫는다.
    pub(crate) fn route_githunk(&mut self, ev: &InputEvent) -> bool {
        if !self.githunk.open {
            return false;
        }
        let inside = |x: i32, y: i32| self.githunk.rect.contains(Point { x, y });
        match ev {
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } => {
                self.githunk.close();
                self.redraw();
                true
            }
            InputEvent::MouseDown { x, y, .. }
            | InputEvent::RightDown { x, y }
            | InputEvent::MouseUp { x, y }
            | InputEvent::MouseMove { x, y }
                if inside(*x, *y) =>
            {
                if self.githunk.view.on_event(ev) {
                    self.redraw();
                }
                true
            }
            InputEvent::Wheel { .. } if inside(self.cursor.0, self.cursor.1) => {
                if self.githunk.view.on_event(ev) {
                    self.redraw();
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// 문맥 = Equal 행 기준(3차): 변경 줄 뒤에 지운 줄이 이어져도 그 뒤 문맥 2줄이 보인다 · 삭제만인 덩어리는 아래 줄 · 끝에서 잘린다.
    #[test]
    fn hunk_rows_context_counts_equal_rows_only() {
        let a = lines(&["a", "b", "c", "d", "e", "f", "g"]);
        let b = lines(&["a", "B", "d", "e", "f", "g", "h"]);
        let rows = diff::align(&a, &b);
        let hs = diff::hunks(&rows);
        // 작업본 1행(B) = 변경 · 바로 뒤 HEAD c 삭제 → 창은 앞 Equal 1(a) + 변경 + 삭제 + 뒤 Equal 2(d·e).
        let w = hunk_rows_at(&rows, &hs, 1, 2).expect("hunk");
        let kinds: Vec<diff::RowKind> = rows[w.clone()].iter().map(|r| r.kind).collect();
        assert_eq!(w.start, 0, "{kinds:?}");
        assert_eq!(
            kinds.iter().filter(|k| **k == diff::RowKind::Equal).count(),
            3,
            "앞 1 + 뒤 2 = Equal 3: {kinds:?}"
        );
        assert!(
            kinds.contains(&diff::RowKind::Delete),
            "이어진 지운 줄은 문맥 예산을 안 먹고 같이 보인다: {kinds:?}"
        );
        // 삭제만인 덩어리 = 그 아래 작업본 줄(2 = d)에서 열린다.
        assert!(hunk_rows_at(&rows, &hs, 2, 0)
            .is_some_and(|w| rows[w].iter().any(|r| r.kind == diff::RowKind::Delete)));
        // 끝에 추가된 줄(작업본 6 = h) · 문맥 1 · 끝에서 잘림.
        let w = hunk_rows_at(&rows, &hs, 6, 1).expect("added");
        assert_eq!(w.end, rows.len());
        assert_eq!(rows[w.start].kind, diff::RowKind::Equal);
        assert_eq!(hunk_rows_at(&rows, &hs, 3, 1), None, "변경 없는 줄 = 없음");
        assert_eq!(hunk_rows_at(&[], &[], 0, 1), None);
    }

    /// 실제 정렬과 맞물려: 뒤집으면 Insert↔Delete.
    #[test]
    fn hunk_rows_with_real_align_and_flip() {
        let a = lines(&["a", "b", "c"]);
        let b = lines(&["a", "B", "c", "d"]);
        let rows = diff::align(&a, &b);
        let hs = diff::hunks(&rows);
        assert_eq!(hunk_rows_at(&rows, &hs, 1, 1), Some(0..3));
        let added = hunk_rows_at(&rows, &hs, 3, 0).expect("added");
        let r = flip_row(&rows[added.start]);
        assert_eq!(
            (r.a, r.b, r.kind),
            (Some(3), None, diff::RowKind::Delete),
            "편집기(b)에만 있던 줄 = 뒤집으면 왼쪽(a)에만 = Delete(초록)"
        );
    }
}
