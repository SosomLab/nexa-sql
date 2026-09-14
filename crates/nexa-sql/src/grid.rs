//! 결과 그리드 — 최소 가상화(보이는 행만 그린다) · **픽셀 단위 스크롤**(사용자 09-14) · 오버레이 스크롤바(필요할 때만 ·
//! 호버 두껍게 · 자동 숨김 — nexa-ctl `ScrollBars` 공용) · 컬럼 폭은 앞 200행 실측 · 메시지 모드.
//! `nexa-grid` 크레이트(U-3 · nexa-ui 21)가 오면 교체한다. 고정폭 층에서 그려진다.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::theme::Theme;
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
        }
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
        self.rs = Some(rs);
        self.messages.clear();
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.col_w.clear();
        self.load = t.elapsed();
    }

    pub(crate) fn set_messages(&mut self, m: Vec<String>) {
        self.messages = m;
    }

    fn rows(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.rows.len())
    }

    /// 행 영역 높이(헤더 제외).
    fn body_h(&self) -> i32 {
        (self.bounds.h - self.header_h).max(0)
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
            (ch - self.bounds.h).max(0),
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

    /// 페이드 타이머 — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.bars.tick(now_ms)
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.bars.is_visible()
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent, scale: f32) {
        // 스크롤바가 먼저(휠 = 픽셀 · 썸 드래그 · 호버). 소비되면 키 처리로 흘리지 않는다.
        if self.row_h > 0 && self.rs.is_some() {
            let (cw, ch) = self.content_size();
            let b = self.bounds;
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
        let body = Rect::new(
            b.x,
            header.bottom(),
            b.w,
            (b.bottom() - header.bottom()).max(0),
        );
        let first = (self.scroll_y / self.row_h.max(1)) as usize;
        let sub = self.scroll_y % self.row_h.max(1);
        let mut y = body.y - sub;
        let mut last = first;
        for (ri, row) in rs.rows.iter().enumerate().skip(first) {
            if y >= body.bottom() {
                break;
            }
            last = ri + 1;
            let rr = Rect::new(b.x, y, b.w, self.row_h).intersection(&body);
            if ri % 2 == 1 {
                dc.fill_rect(rr, th.panel_bg_alt);
            }
            let cells = Rect::new(gx0, body.y, (b.right() - gx0).max(0), body.h);
            let mut x = gx0 - self.scroll_x;
            for (ci, v) in row.iter().enumerate() {
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
                let num = (ri + 1).to_string();
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
        for (i, c) in rs.columns.iter().enumerate() {
            let cw = self.col_w[i];
            let clip = Rect::new(x, header.y, cw, header.h).intersection(&hcells);
            dc.text(x + pad, header.y + pad / 2, clip, &c.name, th.text);
            dc.fill_rect(Rect::new(x + cw - 1, header.y, 1, header.h), th.border);
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
        dc.text(b.x + b.w - iw - pad, b.y + pad / 2, b, &info, th.text_dim);
        // 오버레이 스크롤바(필요할 때만 · 스크롤/호버 시 · 반투명)
        let (cw, ch) = self.content_size();
        self.bars.paint(
            dc,
            th,
            b,
            cw.max(b.w),
            ch.max(b.h),
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
