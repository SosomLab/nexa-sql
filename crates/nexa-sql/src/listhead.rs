//! **목록 창 머리 줄 부품**(사용자 11-13 "헤더 정렬 · 헤더 필터" — 트랜잭션 로그 창 · 변수 창): 열 머리 클릭 정렬 + 깔때기 값 필터.
//!
//! 정렬 규칙은 결과 그리드·접속 창과 같다(클릭 = 단일 키 3단 ▲ → ▼ → 해제 · Shift = 결합 키). 값 필터는 그리드의 값 목록
//! 팝업([`crate::valuepick::ValuePick`])을 그대로 쓴다(엑셀식 체크 목록 · 검색). 창은 자체 그리기 목록을 유지하고 이 부품에
//! ① 머리 칸마다 [`ListHead::paint_cell`] ② 누름 판정 [`ListHead::hit`] ③ 행 거르기 [`ListHead::passes`] · 정렬
//! [`ListHead::sort_rows`] ④ 팝업 사건 [`ListHead::route`] · 그리기 [`ListHead::paint_popup`]만 맡긴다(docs/30 §2 부품 표).

use crate::valuepick::{PickResult, ValuePick};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Invalidations, Key as CtlKey, Widget};
use std::cmp::Ordering;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::keyboard::{Key, NamedKey};

/// 헤더 클릭 정렬 키 갱신(결과 그리드와 같은 규칙): 일반 클릭 = 단일 키 3단(▲ → ▼ → 해제) ·
/// Shift = 결합 키 추가/토글(dir2 방식).
pub(crate) fn toggle_sort_key(keys: &mut Vec<(usize, bool)>, col: usize, additive: bool) {
    let pos = keys.iter().position(|(c, _)| *c == col);
    match (additive, pos) {
        (false, Some(0)) if keys.len() == 1 => {
            if keys[0].1 {
                keys[0].1 = false;
            } else {
                keys.clear();
            }
        }
        (false, _) => *keys = vec![(col, true)],
        (true, Some(i)) => {
            if keys[i].1 {
                keys[i].1 = false;
            } else {
                keys.remove(i);
            }
        }
        (true, None) => keys.push((col, true)),
    }
}

/// 정렬 값(열마다 호출자가 고른다 — 숫자 열을 글자로 비교하면 틀어진다).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SortVal {
    /// 빈 값 = 방향과 무관하게 뒤.
    Empty,
    Num(f64),
    /// 글자(대소문자 무관 · 자연 정렬).
    Text(String),
}

impl SortVal {
    pub(crate) fn text(s: &str) -> Self {
        if s.is_empty() {
            SortVal::Empty
        } else {
            SortVal::Text(s.to_lowercase())
        }
    }

    fn cmp_asc(&self, other: &Self) -> Ordering {
        match (self, other) {
            (SortVal::Num(a), SortVal::Num(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (SortVal::Text(a), SortVal::Text(b)) => nexa_ctl::natural::cmp_names(a, b),
            (SortVal::Num(_), SortVal::Text(_)) => Ordering::Less,
            (SortVal::Text(_), SortVal::Num(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}

/// 머리 줄에서 누른 것.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeadHit {
    Sort(usize),
    Funnel(usize),
}

/// [`ListHead::route`]의 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Routed {
    /// 팝업이 닫혀 있거나 이 사건을 쓰지 않았다(바깥 클릭 = 닫고 통과 포함) — 창이 평소대로 처리.
    Pass,
    /// 팝업이 썼다 — 창은 다시 그리기만.
    Consumed,
}

#[derive(Default)]
pub(crate) struct ListHead {
    /// 정렬 키(열 · 오름차순) — 비면 창의 기본 순서.
    sort: Vec<(usize, bool)>,
    /// 값 필터(열 · 허용 값들 · 대소문자 무관 일치).
    filters: Vec<(usize, Vec<String>)>,
    /// 마지막 그리기의 머리 칸 · 깔때기 영역(누름 판정).
    cells: Vec<(usize, Rect)>,
    funnels: Vec<(usize, Rect)>,
    vpick: ValuePick,
    /// 깔때기를 눌렀다 — 값 목록은 다음 그리기에서 연다(행 데이터는 그릴 때만 손에 있다).
    pending: Option<usize>,
}

impl ListHead {
    pub(crate) fn has_sort(&self) -> bool {
        !self.sort.is_empty()
    }

    pub(crate) fn has_filters(&self) -> bool {
        !self.filters.is_empty()
    }

    /// 그리기 시작(머리 칸 기록 비우기).
    pub(crate) fn begin_paint(&mut self) {
        self.cells.clear();
        self.funnels.clear();
    }

    /// 머리 칸 하나의 표식(정렬 삼각형 · 결합 순번 · 깔때기)을 그리고 누름 영역을 기록한다 — 돌려주는 값 = 오른쪽에서 쓴 폭
    /// (호출자가 머리 글의 자르기 폭에서 뺀다).
    pub(crate) fn paint_cell(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        col: usize,
        cell: Rect,
        scale: f32,
    ) -> i32 {
        let px = |v: f32| (v * scale).round() as i32;
        self.cells.push((col, cell));
        let g = px(9.0).max(6);
        let pad = px(6.0);
        let gap = px(4.0);
        let cy = cell.y + cell.h / 2;
        let right = cell.right() - pad;
        // ① 깔때기(오른쪽 끝) — 걸린 열 = 강조색.
        let filtered = self.filters.iter().any(|(c, _)| *c == col);
        let fr = Rect::new(right - g, cy - g / 2, g, g);
        crate::grid::funnel_glyph(dc, fr, if filtered { th.accent } else { th.text_dim });
        self.funnels.push((
            col,
            Rect::new(fr.x - pad / 2, cell.y, g + pad, cell.h).intersection(&cell),
        ));
        let mut used = g + pad;
        // ② 정렬 표식(깔때기 왼쪽 · 결합이면 순번).
        if let Some(i) = self.sort.iter().position(|(c, _)| *c == col) {
            let mut sx_right = right - g - gap;
            if self.sort.len() > 1 {
                let n = (i + 1).to_string();
                let nw = dc.text_width(&n);
                let hy = dc.text_center_y(cell.y, cell.h);
                dc.text(sx_right - nw, hy, cell, &n, th.accent);
                sx_right -= nw + gap;
            }
            let gx = sx_right - g;
            let (top, bottom) = (cy - g / 2, cy + g / 2);
            let (a, b, c) = if self.sort[i].1 {
                ((gx + g / 2, top), (gx + g, bottom), (gx, bottom))
            } else {
                ((gx, top), (gx + g, top), (gx + g / 2, bottom))
            };
            dc.fill_triangle(a, b, c, th.accent);
            used = cell.right() - gx;
        }
        used + gap
    }

    /// 머리 줄에서 무엇을 눌렀나(깔때기가 먼저).
    pub(crate) fn hit(&self, p: Point) -> Option<HeadHit> {
        if let Some((c, _)) = self.funnels.iter().find(|(_, r)| r.contains(p)) {
            return Some(HeadHit::Funnel(*c));
        }
        self.cells
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|(c, _)| HeadHit::Sort(*c))
    }

    /// 머리 줄 누름 처리 — 정렬 토글 또는 값 목록 예약. 머리 줄 밖이면 false.
    pub(crate) fn click(&mut self, p: Point, additive: bool) -> bool {
        match self.hit(p) {
            Some(HeadHit::Sort(c)) => toggle_sort_key(&mut self.sort, c, additive),
            Some(HeadHit::Funnel(c)) => self.pending = Some(c),
            None => return false,
        }
        true
    }

    /// 예약된 값 목록 열기(그리기 안에서 · `values` = 그 열의 값들을 내는 함수 — 예약이 있을 때만 불린다).
    pub(crate) fn open_pending(
        &mut self,
        values: impl FnOnce(usize) -> Vec<String>,
        host: Rect,
        scale: f32,
        row_h: i32,
    ) {
        let Some(col) = self.pending.take() else {
            return;
        };
        let Some(anchor) = self.cells.iter().find(|(c, _)| *c == col).map(|(_, r)| *r) else {
            return;
        };
        let vals = crate::grid::distinct_capped(
            values(col).into_iter().filter(|v| !v.is_empty()),
            usize::MAX - 1,
        );
        let chosen: Vec<String> = self
            .filters
            .iter()
            .find(|(c, _)| *c == col)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        self.vpick
            .open(col, anchor, vals, false, &chosen, host, scale, row_h, 0);
    }

    /// 값 목록 예약이 있나(창이 "전체 행의 열 글"을 준비할지 판단).
    pub(crate) fn wants_values(&self) -> bool {
        self.pending.is_some()
    }

    /// 행이 값 필터를 전부 지나는가 — `cell(col)` = 그 행의 열 글.
    pub(crate) fn passes(&self, cell: impl Fn(usize) -> String) -> bool {
        self.filters.iter().all(|(c, vs)| {
            let v = cell(*c);
            vs.iter().any(|x| x.eq_ignore_ascii_case(&v))
        })
    }

    /// 정렬(안정 · 키를 한 번씩만 계산) — `key(행, 열)` = 정렬 값. 키가 없으면 그대로.
    pub(crate) fn sort_rows<T>(&self, rows: Vec<T>, key: impl Fn(&T, usize) -> SortVal) -> Vec<T> {
        if self.sort.is_empty() {
            return rows;
        }
        let mut deco: Vec<(Vec<SortVal>, T)> = rows
            .into_iter()
            .map(|r| (self.sort.iter().map(|(c, _)| key(&r, *c)).collect(), r))
            .collect();
        let dirs: Vec<bool> = self.sort.iter().map(|(_, asc)| *asc).collect();
        deco.sort_by(|(a, _), (b, _)| compare_keys(a, b, &dirs));
        deco.into_iter().map(|(_, r)| r).collect()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.vpick.is_open()
    }

    /// 값 목록 팝업이 열려 있을 때의 창 사건 라우팅(winit 사건을 컨트롤 사건으로 바꿔 팝업에 준다) — 결과가 나오면 필터를 고친다.
    /// 바깥 클릭은 팝업이 닫고 false를 돌려주므로 `Pass` = 창이 그 클릭을 평소대로 처리한다(팝업 규칙).
    pub(crate) fn route(
        &mut self,
        ev: &WindowEvent,
        cursor: &mut (i32, i32),
        shift: bool,
        primary: bool,
    ) -> Routed {
        if !self.vpick.is_open() {
            return Routed::Pass;
        }
        self.vpick.set_shift(shift);
        let mut inv = Invalidations::default();
        let (x, y) = *cursor;
        let e = match ev {
            WindowEvent::CursorMoved { position, .. } => {
                *cursor = (position.x as i32, position.y as i32);
                Some(InputEvent::MouseMove {
                    x: cursor.0,
                    y: cursor.1,
                })
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => Some(InputEvent::MouseDown {
                x,
                y,
                shift,
                primary,
            }),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => Some(InputEvent::MouseUp { x, y }),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => Some(InputEvent::RightDown { x, y }),
            WindowEvent::MouseWheel { delta, .. } => Some(crate::input::wheel_event(delta, shift)),
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(tb) = self.vpick.textbox_mut() {
                    tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
                }
                self.vpick.query_changed();
                return Routed::Consumed;
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(tb) = self.vpick.textbox_mut() {
                    tb.set_preedit(text, &mut inv);
                }
                self.vpick.query_changed();
                return Routed::Consumed;
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                let key = |k: CtlKey| InputEvent::Key {
                    key: k,
                    shift,
                    primary,
                };
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => Some(key(CtlKey::Escape)),
                    Key::Named(NamedKey::Enter) => Some(key(CtlKey::Enter)),
                    Key::Named(NamedKey::ArrowUp) => Some(key(CtlKey::Up)),
                    Key::Named(NamedKey::ArrowDown) => Some(key(CtlKey::Down)),
                    _ => crate::input::text_key_event(
                        kev,
                        shift,
                        primary,
                        crate::input::TextKeys::Line,
                    ),
                }
            }
            _ => None,
        };
        let Some(e) = e else {
            return Routed::Pass;
        };
        let consumed = self.vpick.on_event(&e, &mut inv);
        if let Some(nexa_ctl::EditCtxAction::Copy) = self.vpick.take_edit_ctx() {
            if let Some(text) = self.vpick.textbox_mut().and_then(|tb| tb.copy_selection()) {
                let _ = crate::clipboard::write_text(&text);
            }
        }
        if let Some(PickResult::Apply { col, values }) = self.vpick.take_result() {
            self.filters.retain(|(c, _)| *c != col);
            if !values.is_empty() {
                self.filters.push((col, values));
            }
        }
        // 키 입력은 팝업이 열려 있는 동안 창으로 새지 않게(검색 상자에 치는 글이 창의 입력란에도 들어가지 않도록).
        if consumed || matches!(ev, WindowEvent::KeyboardInput { .. }) {
            Routed::Consumed
        } else {
            Routed::Pass
        }
    }

    /// 값 목록 팝업(맨 마지막 층에서 부른다).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.vpick.is_open() {
            self.vpick.paint(dc, th);
        }
    }

    /// 기동 명령·시험용: 정렬 키 · 필터를 직접 정한다.
    #[cfg(test)]
    pub(crate) fn set_for_test(
        &mut self,
        sort: Vec<(usize, bool)>,
        filters: Vec<(usize, Vec<String>)>,
    ) {
        self.sort = sort;
        self.filters = filters;
    }
}

/// 결합 정렬 비교(빈 값은 방향과 무관하게 뒤).
fn compare_keys(a: &[SortVal], b: &[SortVal], dirs: &[bool]) -> Ordering {
    for ((ka, kb), asc) in a.iter().zip(b.iter()).zip(dirs.iter()) {
        let o = match (ka, kb) {
            (SortVal::Empty, SortVal::Empty) => Ordering::Equal,
            (SortVal::Empty, _) => return Ordering::Greater,
            (_, SortVal::Empty) => return Ordering::Less,
            _ => ka.cmp_asc(kb),
        };
        if o != Ordering::Equal {
            return if *asc { o } else { o.reverse() };
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn sort_rows_orders_by_keys_with_empty_last() {
        let mut h = ListHead::default();
        let rows = vec![
            ("b", 2.0),
            ("", 9.0),
            ("A", 10.0),
            ("a10", 1.0),
            ("a2", 1.0),
        ];
        let key = |r: &(&str, f64), c: usize| {
            if c == 0 {
                SortVal::text(r.0)
            } else {
                SortVal::Num(r.1)
            }
        };
        // 키 없음 = 그대로.
        assert_eq!(h.sort_rows(rows.clone(), key), rows);
        // 글자 오름차순(대소문자 무관 · 자연 정렬 a2 < a10) · 빈 값은 뒤.
        h.set_for_test(vec![(0, true)], Vec::new());
        let names: Vec<&str> = h.sort_rows(rows.clone(), key).iter().map(|r| r.0).collect();
        assert_eq!(names, vec!["A", "a2", "a10", "b", ""]);
        // 내림차순에서도 빈 값은 뒤.
        h.set_for_test(vec![(0, false)], Vec::new());
        let names: Vec<&str> = h.sort_rows(rows.clone(), key).iter().map(|r| r.0).collect();
        assert_eq!(names, vec!["b", "a10", "a2", "A", ""]);
        // 숫자 열 + 결합 키(숫자 오름 → 이름 내림) · 숫자는 글자가 아니라 값으로(2 < 10).
        h.set_for_test(vec![(1, true), (0, false)], Vec::new());
        let names: Vec<&str> = h.sort_rows(rows, key).iter().map(|r| r.0).collect();
        assert_eq!(names, vec!["a10", "a2", "b", "", "A"]);
    }

    #[test]
    fn value_filters_match_case_insensitively_on_every_column() {
        let mut h = ListHead::default();
        assert!(h.passes(|_| String::new()), "필터 없음 = 전부 통과");
        h.set_for_test(
            Vec::new(),
            vec![
                (0, vec!["tab".into(), "shared".into()]),
                (2, vec!["NUMBER".into()]),
            ],
        );
        let row = |a: &'static str, c: &'static str| {
            move |i: usize| if i == 0 { a.to_string() } else { c.to_string() }
        };
        assert!(h.passes(row("TAB", "number")));
        assert!(!h.passes(row("global", "number")));
        assert!(!h.passes(row("tab", "varchar2")));
    }

    #[test]
    fn sort_key_toggle_cycles_and_combines() {
        let mut k = Vec::new();
        toggle_sort_key(&mut k, 2, false);
        assert_eq!(k, vec![(2, true)]);
        toggle_sort_key(&mut k, 2, false);
        assert_eq!(k, vec![(2, false)]);
        toggle_sort_key(&mut k, 2, false);
        assert!(k.is_empty());
        toggle_sort_key(&mut k, 1, false);
        toggle_sort_key(&mut k, 3, true);
        assert_eq!(k, vec![(1, true), (3, true)]);
        toggle_sort_key(&mut k, 0, false);
        assert_eq!(k, vec![(0, true)], "보통 클릭 = 단일 키로");
    }
}
