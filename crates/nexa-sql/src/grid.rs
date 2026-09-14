//! 결과 그리드 — 최소 가상화(보이는 행만 그린다) · **픽셀 단위 스크롤**(사용자 09-14) · 오버레이 스크롤바(필요할 때만 ·
//! 호버 두껍게 · 자동 숨김 — nexa-ctl `ScrollBars` 공용) · 컬럼 폭은 앞 200행 실측 · 메시지 모드.
//! `nexa-grid` 크레이트(U-3 · nexa-ui 21)가 오면 교체한다. 고정폭 층에서 그려진다.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::tokens::{hover_alpha, HoverFade};
use nexa_ctl::{InputEvent, Key, ScrollBars};
use nsql_core::{fmt_bytes, fmt_dur, ResultSet, Value};

pub(crate) struct Grid {
    pub bounds: Rect,
    rs: Option<ResultSet>,
    messages: Vec<String>,
    /// 탑재(set_result)·마지막 렌더 소요 — 푸터에 표시(docs/26 Load·Render).
    load: std::time::Duration,
    render: std::time::Duration,
    approx_bytes: u64,
    /// 세로 스크롤(픽셀 · 0 = 첫 행 상단).
    scroll_y: i32,
    /// 가로 스크롤(픽셀).
    scroll_x: i32,
    col_w: Vec<i32>,
    row_h: i32,
    header_h: i32,
    /// 오버레이 스크롤바(필요할 때만 · 휠/호버 시 표시 · 호버 두껍게 · 자동 숨김).
    bars: ScrollBars,
    /// 설정 `grid.scroll` = row 이면 세로 스크롤을 행 경계에 맞춘다(기본 pixel · 사용자 09-14).
    row_snap: bool,
    /// 설정 `grid.row_numbers`(기본 켬) — 왼쪽 고정 행번호 열(가로 스크롤 무관).
    row_numbers: bool,
    gutter_w: i32,
    /// 표시 순서 → 원본 컬럼 index(드래그 이동 · 재조회 시 초기화 — 사용자 09-14).
    col_order: Vec<usize>,
    /// 표시 순서 → 원본 행 index(정렬은 인덱스 벡터 · 행 복제 0 — docs/26 §4-4).
    row_order: Vec<usize>,
    /// 결합 정렬 키(원본 컬럼 · 오름차순) — 클릭 = 단일 키 3단(▲→▼→해제) · Shift+클릭 = 키 추가/토글(dir2 방식).
    sort_keys: Vec<(usize, bool)>,
    /// 헤더 드래그(표시 위치 · 시작 x · 현재 x · 4px 이상 움직임 · Shift).
    hdr_drag: Option<(usize, i32, i32, bool, bool)>,
    /// 헤더 경계 드래그 = 컬럼 폭 조절(원본 컬럼 · 시작 x · 시작 폭 — 사용자 09-14).
    hdr_resize: Option<(usize, i32, i32)>,
    /// 마우스가 올라간 행(표시 index)의 **서서히 진해지는** 강조 — nexa-clip과 같은 `HoverFade`(진입 = `grid.hover_fade` · 사용자 09-14).
    hover: HoverFade,
}

impl Default for Grid {
    fn default() -> Self {
        Grid {
            bounds: Rect::new(0, 0, 0, 0),
            rs: None,
            messages: Vec::new(),
            load: std::time::Duration::ZERO,
            render: std::time::Duration::ZERO,
            approx_bytes: 0,
            scroll_y: 0,
            scroll_x: 0,
            col_w: Vec::new(),
            row_h: 0,
            header_h: 0,
            bars: ScrollBars::new(),
            row_snap: false,
            row_numbers: true,
            gutter_w: 0,
            col_order: Vec::new(),
            row_order: Vec::new(),
            sort_keys: Vec::new(),
            hdr_drag: None,
            hdr_resize: None,
            hover: HoverFade::default(),
        }
    }
}

/// 정렬 비교 — 숫자는 수치로 · 문자열은 대소문자 무관 · NULL은 항상 뒤.
fn cmp_value(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn num(v: &Value) -> Option<f64> {
        match v {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Decimal(s) => s.parse().ok(),
            Value::Bool(b) => Some(f64::from(*b)),
            _ => None,
        }
    }
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        _ => match (num(a), num(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => a.display().to_lowercase().cmp(&b.display().to_lowercase()),
        },
    }
}

impl Grid {
    pub(crate) fn set_row_numbers(&mut self, on: bool) {
        self.row_numbers = on;
    }

    /// 스크롤 단위 — `true` = 항목(행) 단위 · `false` = 픽셀(기본).
    pub(crate) fn set_row_snap(&mut self, on: bool) {
        self.row_snap = on;
        self.clamp();
    }

    pub(crate) fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
        self.clamp();
    }

    pub(crate) fn set_result(&mut self, rs: ResultSet) {
        let t = std::time::Instant::now();
        self.approx_bytes = rs.approx_bytes();
        // 재조회 = 조회 컬럼 순서·정렬 초기화(이동·정렬 결과 무시 — 사용자 09-14).
        self.col_order = (0..rs.columns.len()).collect();
        self.row_order = (0..rs.rows.len()).collect();
        self.sort_keys.clear();
        self.hdr_drag = None;
        self.hdr_resize = None;
        self.rs = Some(rs);
        self.messages.clear();
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.col_w.clear();
        self.load = t.elapsed();
    }

    /// 결합 정렬 적용(인덱스 벡터만 재배열 · 안정 정렬이라 같은 값은 원본 순서).
    fn apply_sort(&mut self) {
        let Some(rs) = self.rs.as_ref() else { return };
        let mut order: Vec<usize> = (0..rs.rows.len()).collect();
        if !self.sort_keys.is_empty() {
            let keys = self.sort_keys.clone();
            order.sort_by(|&a, &b| {
                for (col, asc) in &keys {
                    let (va, vb) = (&rs.rows[a][*col], &rs.rows[b][*col]);
                    let o = cmp_value(va, vb);
                    if o != std::cmp::Ordering::Equal {
                        return if *asc { o } else { o.reverse() };
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        self.row_order = order;
    }

    /// 헤더 클릭 정렬. `additive`(Shift) = 결합 키 추가/토글 · 아니면 단일 키 3단(▲ → ▼ → 해제).
    fn toggle_sort(&mut self, col: usize, additive: bool) {
        let pos = self.sort_keys.iter().position(|(c, _)| *c == col);
        match (additive, pos) {
            (false, Some(0)) if self.sort_keys.len() == 1 => {
                if self.sort_keys[0].1 {
                    self.sort_keys[0].1 = false;
                } else {
                    self.sort_keys.clear();
                }
            }
            (false, _) => self.sort_keys = vec![(col, true)],
            (true, Some(i)) => {
                if self.sort_keys[i].1 {
                    self.sort_keys[i].1 = false;
                } else {
                    self.sort_keys.remove(i);
                }
            }
            (true, None) => self.sort_keys.push((col, true)),
        }
        self.apply_sort();
    }

    /// 헤더 영역(x → 표시 위치). 행번호 열은 제외.
    fn header_pos_at(&self, x: i32) -> Option<usize> {
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            if x >= cx && x < cx + cw {
                return Some(pos);
            }
            cx += cw;
        }
        None
    }

    /// 헤더에서 컬럼 오른쪽 경계 ±4px 안이면 그 컬럼(원본 index) — 폭 조절 손잡이.
    fn header_edge_at(&self, x: i32) -> Option<usize> {
        let grip = 6;
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for &ci in &self.col_order {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            cx += cw;
            if (x - cx).abs() <= grip {
                return Some(ci);
            }
        }
        None
    }

    /// 커서가 헤더의 컬럼 경계(폭 조절 손잡이) 위인가 — 호스트가 커서 모양을 바꾼다.
    pub(crate) fn header_edge_hover(&self, x: i32, y: i32) -> bool {
        self.hdr_resize.is_some()
            || (self.rs.is_some()
                && self.row_h > 0
                && self.header_rect().contains(Point { x, y })
                && self.header_edge_at(x).is_some())
    }

    fn header_rect(&self) -> Rect {
        Rect::new(self.bounds.x, self.bounds.y + 1, self.bounds.w, self.row_h)
    }

    /// 드래그 목표 위치(현재 x 기준 · 컬럼 중앙을 넘으면 그 다음).
    fn drop_pos_at(&self, x: i32) -> usize {
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            if x < cx + cw / 2 {
                return pos;
            }
            cx += cw;
        }
        self.col_order.len().saturating_sub(1)
    }

    pub(crate) fn set_messages(&mut self, m: Vec<String>) {
        self.messages = m;
    }

    fn rows(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.rows.len())
    }

    /// 행 영역 높이(헤더·푸터 제외).
    fn body_h(&self) -> i32 {
        (self.bounds.h - self.header_h - self.row_h).max(0)
    }

    /// 콘텐츠 크기(스크롤 범위) — 헤더 + 전 행 · 컬럼 폭 합.
    fn content_size(&self) -> (i32, i32) {
        let w: i32 = self.col_w.iter().sum();
        let h = self.header_h + self.row_h * self.rows() as i32;
        (w, h)
    }

    fn max_scroll(&self) -> (i32, i32) {
        let (cw, ch) = self.content_size();
        (
            (cw - (self.bounds.w - self.gutter_w)).max(0),
            (ch - (self.bounds.h - self.row_h)).max(0),
        )
    }

    fn clamp(&mut self) {
        let (mx, my) = self.max_scroll();
        self.scroll_x = self.scroll_x.clamp(0, mx);
        self.scroll_y = self.scroll_y.clamp(0, my);
        if self.row_snap && self.row_h > 0 && self.scroll_y < my {
            self.scroll_y -= self.scroll_y % self.row_h;
        }
    }

    /// 페이드 타이머(스크롤바 · 호버 행) — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.bars.tick(now_ms);
        let b = self.hover.tick(now_ms);
        a || b
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.bars.is_visible()
    }

    /// 호버 페이드가 움직이는 중인가(호스트가 ≈30ms 프레임을 예약).
    pub(crate) fn hover_animating(&self) -> bool {
        self.hover.is_animating()
    }

    /// 마우스 아래 행(표시 index) — 본문 영역 안, 실제 행 범위 안일 때만.
    fn row_at_point(&self, x: i32, y: i32) -> Option<usize> {
        if self.row_h <= 0 || self.rs.is_none() {
            return None;
        }
        let b = self.bounds;
        let body = Rect::new(b.x, b.y + self.header_h, b.w, self.body_h());
        if !body.contains(Point { x, y }) {
            return None;
        }
        let di = ((y - body.y + self.scroll_y) / self.row_h) as usize;
        (di < self.rows()).then_some(di)
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent, scale: f32) {
        // 헤더: 클릭 = 정렬(Shift = 결합) · 드래그 = 컬럼 이동.
        if self.row_h > 0 && self.rs.is_some() && !self.col_order.is_empty() {
            let hdr = self.header_rect();
            match *ev {
                InputEvent::MouseDown { x, y, shift, .. } if hdr.contains(Point { x, y }) => {
                    if let Some(ci) = self.header_edge_at(x) {
                        let w0 = self.col_w.get(ci).copied().unwrap_or(80);
                        self.hdr_resize = Some((ci, x, w0));
                    } else if let Some(pos) = self.header_pos_at(x) {
                        self.hdr_drag = Some((pos, x, x, false, shift));
                    }
                    return;
                }
                InputEvent::MouseMove { x, .. } if self.hdr_resize.is_some() => {
                    if let Some((ci, x0, w0)) = self.hdr_resize {
                        if let Some(w) = self.col_w.get_mut(ci) {
                            *w = (w0 + (x - x0)).max(24);
                        }
                        self.clamp();
                    }
                    return;
                }
                InputEvent::MouseUp { .. } if self.hdr_resize.is_some() => {
                    self.hdr_resize = None;
                    return;
                }
                InputEvent::MouseMove { x, .. } if self.hdr_drag.is_some() => {
                    if let Some(d) = self.hdr_drag.as_mut() {
                        d.2 = x;
                        if (x - d.1).abs() > 4 {
                            d.3 = true;
                        }
                    }
                    return;
                }
                InputEvent::MouseUp { x, .. } if self.hdr_drag.is_some() => {
                    let (pos, _, _, moved, shift) =
                        self.hdr_drag.take().unwrap_or((0, 0, 0, false, false));
                    if moved {
                        let to = self.drop_pos_at(x);
                        if pos < self.col_order.len() && to != pos {
                            let c = self.col_order.remove(pos);
                            self.col_order.insert(to.min(self.col_order.len()), c);
                        }
                    } else if let Some(&col) = self.col_order.get(pos) {
                        self.toggle_sort(col, shift);
                    }
                    return;
                }
                _ => {}
            }
        }
        // 호버 행 — 목표만 바꾼다(진행도는 `tick`이 흘린다 · 드래그/폭 조절 중엔 위에서 이미 돌아갔다).
        if let InputEvent::MouseMove { x, y } = *ev {
            self.hover.set(self.row_at_point(x, y));
        }
        // 스크롤바가 먼저(휠 = 픽셀 · 썸 드래그 · 호버). 소비되면 키 처리로 흘리지 않는다.
        if self.row_h > 0 && self.rs.is_some() {
            let (cw, ch) = self.content_size();
            let b = self.bounds;
            let b = Rect::new(b.x, b.y, b.w, b.h - self.row_h);
            let (nx, ny, consumed) = self.bars.on_event(
                ev,
                b,
                cw.max(b.w),
                ch.max(b.h),
                self.scroll_x,
                self.scroll_y,
                scale,
            );
            self.scroll_x = nx;
            self.scroll_y = ny;
            self.clamp();
            if consumed {
                return;
            }
        }
        let page = self.body_h().max(self.row_h);
        match ev {
            InputEvent::Key {
                key: Key::PageDown, ..
            } => self.scroll_y += page,
            InputEvent::Key {
                key: Key::PageUp, ..
            } => self.scroll_y -= page,
            InputEvent::Key { key: Key::Home, .. } => self.scroll_y = 0,
            InputEvent::Key { key: Key::End, .. } => self.scroll_y = i32::MAX / 2,
            InputEvent::Key { key: Key::Down, .. } => self.scroll_y += self.row_h,
            InputEvent::Key { key: Key::Up, .. } => self.scroll_y -= self.row_h,
            InputEvent::Key { key: Key::Left, .. } => self.scroll_x -= self.row_h * 2,
            InputEvent::Key {
                key: Key::Right, ..
            } => self.scroll_x += self.row_h * 2,
            _ => {}
        }
        self.clamp();
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let t_render = std::time::Instant::now();
        self.paint_inner(dc, th, s);
        self.render = t_render.elapsed();
    }

    fn paint_inner(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let b = self.bounds;
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.x, b.y, b.w, 1), th.border);
        dc.select_font(FontSlot::Base, false);
        let pad = (6.0 * s).round() as i32;
        self.row_h = dc.text_height() + pad;
        // 메시지 모드(결과 없음 · 오류 · PRINT)
        let Some(rs) = self.rs.as_ref() else {
            let mut y = b.y + pad;
            for m in self
                .messages
                .iter()
                .rev()
                .take(200)
                .collect::<Vec<_>>()
                .iter()
                .rev()
            {
                if y > b.y + b.h {
                    break;
                }
                dc.text(b.x + pad, y, b, m, th.text);
                y += self.row_h;
            }
            return;
        };
        if self.col_w.is_empty() {
            self.col_w = rs
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let mut w = dc.text_width(&c.name);
                    for row in rs.rows.iter().take(200) {
                        if let Some(v) = row.get(i) {
                            w = w.max(dc.text_width(&cell_text(v)));
                        }
                    }
                    w.min((420.0 * s) as i32) + pad * 2
                })
                .collect();
        }
        let header = Rect::new(b.x, b.y + 1, b.w, self.row_h);
        self.header_h = header.h + 1;
        // 행번호 열 폭(자릿수 × 숫자 폭 + 여백) — 가로 스크롤과 무관한 고정 열.
        self.gutter_w = if self.row_numbers {
            let digits = rs.rows.len().max(1).to_string().len().max(2) as i32;
            digits * dc.text_width("0") + pad * 2
        } else {
            0
        };
        let gx0 = b.x + self.gutter_w;
        // 크기가 정해진 뒤 범위 재확인(창 리사이즈 · 첫 페인트).
        let (mx, my) = self.max_scroll();
        self.scroll_x = self.scroll_x.clamp(0, mx);
        self.scroll_y = self.scroll_y.clamp(0, my);

        // ── 행(픽셀 오프셋: 첫 행이 부분적으로 잘려 올라간다)
        // 푸터(위치·계측) 한 줄은 맨 아래 — 헤더와 겹치지 않는다(09-14 사용자 지적).
        let footer = Rect::new(b.x, b.bottom() - self.row_h, b.w, self.row_h);
        let body = Rect::new(
            b.x,
            header.bottom(),
            b.w,
            (footer.y - header.bottom()).max(0),
        );
        let first = (self.scroll_y / self.row_h.max(1)) as usize;
        let sub = self.scroll_y % self.row_h.max(1);
        let mut y = body.y - sub;
        let mut last = first;
        let n = rs.rows.len();
        for di in first..n {
            if y >= body.bottom() {
                break;
            }
            let ri = self.row_order.get(di).copied().unwrap_or(di);
            let Some(row) = rs.rows.get(ri) else { break };
            last = di + 1;
            let rr = Rect::new(b.x, y, b.w, self.row_h).intersection(&body);
            if di % 2 == 1 {
                dc.fill_rect(rr, th.panel_bg_alt);
            }
            // 호버 강조 — 색을 새로 만들지 않고 전경색을 알파로 덮는다(진행도 × 토큰 알파 · 서서히).
            let ha = hover_alpha(false, self.hover.value(di));
            if ha > 0.0 {
                dc.fill_rect_alpha(rr, th.text, ha);
            }
            let cells = Rect::new(gx0, body.y, (b.right() - gx0).max(0), body.h);
            let mut x = gx0 - self.scroll_x;
            for &ci in &self.col_order {
                let Some(v) = row.get(ci) else { continue };
                let cw = self.col_w.get(ci).copied().unwrap_or(80);
                let clip = Rect::new(x, y, cw - 1, self.row_h).intersection(&cells);
                if clip.w > 0 && clip.h > 0 {
                    let txt = cell_text(v);
                    let numeric = matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_));
                    let color = if matches!(v, Value::Null) {
                        th.text_dim
                    } else {
                        th.text
                    };
                    if numeric {
                        let tw = dc.text_width(&txt);
                        dc.text(x + cw - pad - tw, y + pad / 2, clip, &txt, color);
                    } else {
                        dc.text(x + pad, y + pad / 2, clip, &txt, color);
                    }
                }
                x += cw;
            }
            // 행번호(고정 열 · 우측 정렬 · 흐리게)
            if self.gutter_w > 0 {
                let num = (di + 1).to_string();
                let nw = dc.text_width(&num);
                let gclip = Rect::new(b.x, y, self.gutter_w, self.row_h).intersection(&body);
                dc.fill_rect(gclip, th.chrome_bg);
                dc.text(gx0 - pad - nw, y + pad / 2, gclip, &num, th.text_dim);
            }
            y += self.row_h;
        }
        if self.gutter_w > 0 {
            dc.fill_rect(Rect::new(gx0 - 1, body.y, 1, body.h), th.border);
        }
        // ── 헤더(행 위에 덮어 그린다 — 부분 스크롤된 첫 행이 헤더 아래로 들어간다)
        dc.fill_rect(header, th.chrome_bg);
        let hcells = Rect::new(gx0, header.y, (b.right() - gx0).max(0), header.h);
        let mut x = gx0 - self.scroll_x;
        let dragging = self.hdr_drag.filter(|d| d.3);
        let drop_pos = dragging.map(|d| self.drop_pos_at(d.2));
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let Some(c) = rs.columns.get(ci) else {
                continue;
            };
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            let clip = Rect::new(x, header.y, cw, header.h).intersection(&hcells);
            if dragging.is_some_and(|d| d.0 == pos) {
                dc.fill_rect(clip, th.sel_bg);
            }
            // 정렬 배지: ▲/▼ + 결합 순번(키가 2개 이상일 때)
            let badge = self.sort_keys.iter().position(|(k, _)| *k == ci).map(|i| {
                let arrow = if self.sort_keys[i].1 { "▲" } else { "▼" };
                if self.sort_keys.len() > 1 {
                    format!("{arrow}{}", i + 1)
                } else {
                    arrow.to_string()
                }
            });
            let name_clip = if let Some(bd) = &badge {
                let bw = dc.text_width(bd);
                dc.text(x + cw - pad - bw, header.y + pad / 2, clip, bd, th.accent);
                Rect::new(x, header.y, (cw - bw - pad * 2).max(0), header.h).intersection(&hcells)
            } else {
                clip
            };
            dc.text(x + pad, header.y + pad / 2, name_clip, &c.name, th.text);
            dc.fill_rect(Rect::new(x + cw - 1, header.y, 1, header.h), th.border);
            if let Some(dp) = drop_pos {
                if dp == pos {
                    dc.fill_rect(Rect::new(x, header.y, 2, header.h), th.accent);
                }
            }
            x += cw;
        }
        if self.gutter_w > 0 {
            dc.fill_rect(
                Rect::new(b.x, header.y, self.gutter_w, header.h),
                th.chrome_bg,
            );
            dc.text(b.x + pad, header.y + pad / 2, header, "#", th.text_dim);
        }
        dc.fill_rect(Rect::new(b.x, header.bottom() - 1, b.w, 1), th.border);
        // 위치 표시(우상단 헤더 줄)
        let info = format!(
            "{}–{} / {} · load {} · render {} · ~{}",
            if rs.rows.is_empty() { 0 } else { first + 1 },
            last,
            rs.rows.len(),
            fmt_dur(self.load),
            fmt_dur(self.render),
            fmt_bytes(self.approx_bytes)
        );
        let iw = dc.text_width(&info);
        dc.fill_rect(footer, th.chrome_bg);
        dc.fill_rect(Rect::new(b.x, footer.y, b.w, 1), th.border);
        dc.text(
            b.x + b.w - iw - pad,
            footer.y + pad / 2,
            footer,
            &info,
            th.text_dim,
        );
        // 오버레이 스크롤바(필요할 때만 · 스크롤/호버 시 · 반투명) — 푸터 위까지.
        let (cw, ch) = self.content_size();
        let vp = Rect::new(b.x, b.y, b.w, b.h - self.row_h);
        self.bars.paint(
            dc,
            th,
            vp,
            cw.max(vp.w),
            ch.max(vp.h),
            self.scroll_x,
            self.scroll_y,
            s,
        );
    }
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "(null)".into(),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        other => other.display(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Column, ResultSet};

    fn grid_with(cols: &[i32]) -> Grid {
        let mut g = Grid::default();
        let rs = ResultSet {
            columns: cols
                .iter()
                .enumerate()
                .map(|(i, _)| Column {
                    name: format!("c{i}"),
                    type_name: String::new(),
                })
                .collect(),
            rows: vec![vec![Value::Int(1); cols.len()]],
        };
        g.set_result(rs);
        g.bounds = Rect::new(0, 0, 400, 300);
        g.row_h = 20;
        g.header_h = 21;
        g.gutter_w = 30;
        g.col_w = cols.to_vec();
        g
    }

    #[test]
    fn header_edge_drag_resizes_column() {
        let mut g = grid_with(&[100, 80]);
        // 첫 컬럼 오른쪽 경계 = 30 + 100 = 130
        assert!(g.header_edge_hover(131, 5));
        assert!(!g.header_edge_hover(80, 5));
        let down = InputEvent::MouseDown {
            x: 129,
            y: 5,
            shift: false,
            primary: false,
        };
        g.on_event(&down, 1.0);
        assert!(g.hdr_resize.is_some(), "경계에서 폭 조절 시작");
        g.on_event(&InputEvent::MouseMove { x: 169, y: 5 }, 1.0);
        assert_eq!(g.col_w[0], 140);
        g.on_event(&InputEvent::MouseUp { x: 169, y: 5 }, 1.0);
        assert!(g.hdr_resize.is_none());
        // 최소 폭 24 (경계는 이제 30 + 140 = 170)
        g.on_event(
            &InputEvent::MouseDown {
                x: 170,
                y: 5,
                shift: false,
                primary: false,
            },
            1.0,
        );
        assert!(g.hdr_resize.is_some());
        g.on_event(&InputEvent::MouseMove { x: -500, y: 5 }, 1.0);
        assert_eq!(g.col_w[0], 24);
    }

    #[test]
    fn header_click_sorts_and_shift_adds_key() {
        let mut g = grid_with(&[100, 80]);
        let click = |g: &mut Grid, x: i32, shift: bool| {
            g.on_event(
                &InputEvent::MouseDown {
                    x,
                    y: 5,
                    shift,
                    primary: false,
                },
                1.0,
            );
            g.on_event(&InputEvent::MouseUp { x, y: 5 }, 1.0);
        };
        click(&mut g, 80, false);
        assert_eq!(g.sort_keys, vec![(0, true)]);
        click(&mut g, 80, false);
        assert_eq!(g.sort_keys, vec![(0, false)]);
        click(&mut g, 170, true);
        assert_eq!(g.sort_keys, vec![(0, false), (1, true)]);
        click(&mut g, 80, false);
        assert_eq!(g.sort_keys, vec![(0, true)], "일반 클릭 = 단일 키로");
    }
}
