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

/// 클릭한 작업본 줄 `line`이 든 덩어리의 (HEAD 줄 범위, 작업본 줄 범위) — 문맥 `ctx`줄을 양쪽에 붙인다(각 본문 길이로 자른다).
/// 삭제만 있는 덩어리(작업본 줄 0개 · 거터는 그 **아래 줄**에 쐐기)는 `b.start == line`인 것. 없으면 None.
pub(crate) fn hunk_ranges_at(
    hunks: &[diff::Hunk],
    line: usize,
    ctx: usize,
    a_len: usize,
    b_len: usize,
) -> Option<(Range<usize>, Range<usize>)> {
    let h = hunks
        .iter()
        .find(|h| (h.b.start <= line && line < h.b.end) || (h.b.is_empty() && h.b.start == line))?;
    let a = h.a.start.saturating_sub(ctx)..(h.a.end + ctx).min(a_len);
    let b = h.b.start.saturating_sub(ctx)..(h.b.end + ctx).min(b_len);
    Some((a, b))
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
        let Some((ra, rb)) = hunk_ranges_at(&hunks, line, ctx, a.len(), b.len()) else {
            self.sess.status = t(Msg::StGitNoHunk).into();
            self.redraw();
            return true;
        };
        let (head_part, cur_part) = (&a[ra], &b[rb]);
        self.close_context_menus();
        // ★ 왼쪽 = 편집기(현재) · 오른쪽 = HEAD(협업 V1 11-10 관찰 (1)(2)): DiffView 색 규칙이 "a에만 = 초록 · b에만 = 빨강"이라
        //   편집기를 a에 두어야 추가 줄 = 초록 · HEAD에만(지운 줄) = 빨강으로 범례와 맞는다. 머리글은 DB 비교 문구 대신 전용.
        self.githunk.view.set(
            cur_part,
            head_part,
            cur_part,
            head_part,
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
        let h = rows_n * lh + s(42.0) + s(8.0);
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

    fn hunk(rows: Range<usize>, a: Range<usize>, b: Range<usize>) -> diff::Hunk {
        diff::Hunk { rows, a, b }
    }

    /// 덩어리 찾기 = 작업본 줄이 든 것 · 삭제만인 덩어리는 아래 줄 · 문맥은 양쪽 길이로 잘린다.
    #[test]
    fn hunk_ranges_pick_and_clamp() {
        let hs = [hunk(0..2, 0..2, 0..0), hunk(5..8, 4..6, 3..6)];
        assert_eq!(hunk_ranges_at(&hs, 4, 1, 10, 10), Some((3..7, 2..7)));
        assert_eq!(
            hunk_ranges_at(&hs, 5, 3, 7, 7),
            Some((1..7, 0..7)),
            "본문 길이로 자른다"
        );
        assert_eq!(
            hunk_ranges_at(&hs, 0, 0, 10, 10),
            Some((0..2, 0..0)),
            "삭제만 = 그 아래 줄"
        );
        assert_eq!(hunk_ranges_at(&hs, 8, 1, 10, 10), None);
        assert_eq!(hunk_ranges_at(&[], 0, 1, 10, 10), None);
    }

    /// 실제 정렬과 맞물려: HEAD 3줄 → 작업본에서 가운데 줄 수정 · 끝에 추가.
    #[test]
    fn hunk_ranges_with_real_align() {
        let a: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let b: Vec<String> = ["a", "B", "c", "d"].iter().map(|s| s.to_string()).collect();
        let rows = diff::align(&a, &b);
        let hs = diff::hunks(&rows);
        let (ra, rb) = hunk_ranges_at(&hs, 1, 1, a.len(), b.len()).expect("hunk");
        assert_eq!((ra, rb), (0..3, 0..3));
        let (ra, rb) = hunk_ranges_at(&hs, 3, 0, a.len(), b.len()).expect("added");
        assert_eq!((ra, rb), (3..3, 3..4));
    }
}
