//! 결과 그리드 — 최소 가상화(보이는 행만 그린다) · 휠 스크롤 · 컬럼 폭은 앞 200행 실측 · 메시지 모드.
//! `nexa-grid` 크레이트(U-3)가 오면 교체한다. 고정폭 층에서 그려진다.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Key};
use nsql_core::{ResultSet, Value};

#[derive(Default)]
pub(crate) struct Grid {
    pub bounds: Rect,
    rs: Option<ResultSet>,
    messages: Vec<String>,
    /// 첫 표시 행.
    top: usize,
    col_w: Vec<i32>,
    row_h: i32,
    scroll_x: i32,
}

impl Grid {
    pub(crate) fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }

    pub(crate) fn set_result(&mut self, rs: ResultSet) {
        self.rs = Some(rs);
        self.messages.clear();
        self.top = 0;
        self.scroll_x = 0;
        self.col_w.clear();
    }

    pub(crate) fn set_messages(&mut self, m: Vec<String>) {
        self.messages = m;
    }

    fn rows(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.rows.len())
    }

    fn page(&self) -> usize {
        if self.row_h <= 0 {
            return 1;
        }
        ((self.bounds.h / self.row_h).max(2) - 1) as usize
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) {
        let n = self.rows();
        match ev {
            InputEvent::Wheel { delta } => {
                let lines = (-delta / 40).clamp(-20, 20);
                let t = self.top as i64 + i64::from(lines);
                self.top = t.clamp(0, n.saturating_sub(1) as i64) as usize;
            }
            InputEvent::HWheel { delta } => self.scroll_x = (self.scroll_x + delta).max(0),
            InputEvent::Key {
                key: Key::PageDown, ..
            } => self.top = (self.top + self.page()).min(n.saturating_sub(1)),
            InputEvent::Key {
                key: Key::PageUp, ..
            } => self.top = self.top.saturating_sub(self.page()),
            InputEvent::Key { key: Key::Home, .. } => self.top = 0,
            InputEvent::Key { key: Key::End, .. } => self.top = n.saturating_sub(self.page()),
            InputEvent::Key { key: Key::Down, .. } => {
                self.top = (self.top + 1).min(n.saturating_sub(1))
            }
            InputEvent::Key { key: Key::Up, .. } => self.top = self.top.saturating_sub(1),
            _ => {}
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
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
        dc.fill_rect(header, th.chrome_bg);
        let mut x = b.x - self.scroll_x;
        for (i, c) in rs.columns.iter().enumerate() {
            let cw = self.col_w[i];
            let clip = Rect::new(x, header.y, cw, header.h).intersection(&b);
            dc.text(x + pad, header.y + pad / 2, clip, &c.name, th.text);
            dc.fill_rect(Rect::new(x + cw - 1, header.y, 1, header.h), th.border);
            x += cw;
        }
        dc.fill_rect(Rect::new(b.x, header.y + header.h - 1, b.w, 1), th.border);
        let mut y = header.y + header.h;
        let bottom = b.y + b.h;
        for (ri, row) in rs.rows.iter().enumerate().skip(self.top) {
            if y + self.row_h > bottom {
                break;
            }
            let rr = Rect::new(b.x, y, b.w, self.row_h);
            if ri % 2 == 1 {
                dc.fill_rect(rr, th.panel_bg_alt);
            }
            let mut x = b.x - self.scroll_x;
            for (ci, v) in row.iter().enumerate() {
                let cw = self.col_w.get(ci).copied().unwrap_or(80);
                let clip = Rect::new(x, y, cw - 1, self.row_h).intersection(&b);
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
                x += cw;
            }
            y += self.row_h;
        }
        // 위치 표시
        let info = format!(
            "{}–{} / {}",
            self.top + 1,
            (self.top + self.page()).min(rs.rows.len()),
            rs.rows.len()
        );
        let iw = dc.text_width(&info);
        dc.text(b.x + b.w - iw - pad, b.y + pad / 2, b, &info, th.text_dim);
    }
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "(null)".into(),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        other => other.display(),
    }
}
