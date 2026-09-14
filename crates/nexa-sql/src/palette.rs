//! 명령 팔레트(사용자 09-14 · Sublime `Ctrl+⇧P`) — 입력 한 줄 + 퍼지 필터 목록.
//!
//! 명령 = 메뉴 항목 전부(`File: New` …) + `Set Syntax: <이름>`(레지스트리) — 호스트가 [`Palette::set_commands`]로 준다.
//! 키: ↑↓ 이동 · Enter 실행 · Esc 닫기 · 바깥 클릭 닫기 · 행 클릭 실행.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key, TextBox, Widget};
use nsql_i18n::{t, Msg};

pub(crate) enum PaletteAction {
    None,
    Pick(String),
    Close,
}

pub(crate) struct Palette {
    open: bool,
    input: TextBox,
    /// (id, 표시 라벨)
    cmds: Vec<(String, String)>,
    /// 필터 결과 — cmds index (점수순).
    matches: Vec<usize>,
    sel: usize,
    bounds: Rect,
    row_h: i32,
    scale: f32,
    last_query: String,
}

const MAX_ROWS: usize = 12;

impl Palette {
    pub(crate) fn new() -> Self {
        Palette {
            open: false,
            input: TextBox::new(t(Msg::PhPalette)),
            cmds: Vec::new(),
            matches: Vec::new(),
            sel: 0,
            bounds: Rect::new(0, 0, 0, 0),
            row_h: 26,
            scale: 1.0,
            last_query: String::new(),
        }
    }

    pub(crate) fn set_commands(&mut self, cmds: Vec<(String, String)>) {
        self.cmds = cmds;
        self.refilter(true);
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self, prefill: &str) {
        self.open = true;
        self.input = TextBox::new(t(Msg::PhPalette)).with_text(prefill);
        self.input.set_scale(self.scale);
        self.input.set_focused(true);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
        self.last_query.clear();
        self.refilter(true);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.input.set_focused(false);
    }

    /// 창 폭 · 상단 y(메뉴/툴바 아래) · 스케일.
    pub(crate) fn set_bounds(&mut self, win_w: i32, top: i32, scale: f32) {
        self.scale = scale;
        self.row_h = (26.0 * scale) as i32;
        let w = ((560.0 * scale) as i32)
            .min(win_w - (32.0 * scale) as i32)
            .max(200);
        let h = self.row_h * (MAX_ROWS as i32 + 1) + (16.0 * scale) as i32;
        self.bounds = Rect::new((win_w - w) / 2, top + (6.0 * scale) as i32, w, h);
        self.input.set_scale(scale);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    fn layout(&mut self, inv: &mut Invalidations) {
        let b = self.bounds;
        let p = (6.0 * self.scale) as i32;
        self.input
            .set_bounds(Rect::new(b.x + p, b.y + p, b.w - p * 2, self.row_h), inv);
    }

    fn rows_rect(&self) -> Rect {
        let b = self.bounds;
        let p = (6.0 * self.scale) as i32;
        let top = b.y + p + self.row_h + p / 2;
        Rect::new(b.x + p, top, b.w - p * 2, self.row_h * MAX_ROWS as i32)
    }

    fn refilter(&mut self, force: bool) {
        let q = self.input.text();
        if !force && q == self.last_query {
            return;
        }
        self.last_query = q.clone();
        let mut scored: Vec<(i32, usize)> = self
            .cmds
            .iter()
            .enumerate()
            .filter_map(|(i, (_, label))| fuzzy_score(&q, label).map(|s| (s, i)))
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| self.cmds[a.1].1.cmp(&self.cmds[b.1].1))
        });
        self.matches = scored.into_iter().map(|(_, i)| i).collect();
        self.sel = 0;
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> PaletteAction {
        if !self.open {
            return PaletteAction::None;
        }
        match *ev {
            InputEvent::Key {
                key: Key::Escape, ..
            } => return PaletteAction::Close,
            InputEvent::Key {
                key: Key::Enter, ..
            } => {
                return match self.matches.get(self.sel) {
                    Some(&i) => PaletteAction::Pick(self.cmds[i].0.clone()),
                    None => PaletteAction::Close,
                };
            }
            InputEvent::Key { key: Key::Up, .. } => {
                self.sel = self.sel.saturating_sub(1);
                return PaletteAction::None;
            }
            InputEvent::Key { key: Key::Down, .. } => {
                if self.sel + 1 < self.matches.len().min(MAX_ROWS) {
                    self.sel += 1;
                }
                return PaletteAction::None;
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if !self.bounds.contains(p) {
                    return PaletteAction::Close;
                }
                let rr = self.rows_rect();
                if rr.contains(p) {
                    let row = ((y - rr.y) / self.row_h.max(1)) as usize;
                    if let Some(&i) = self.matches.get(row) {
                        return PaletteAction::Pick(self.cmds[i].0.clone());
                    }
                    return PaletteAction::None;
                }
            }
            _ => {}
        }
        self.input.on_event(ev, inv);
        self.refilter(false);
        PaletteAction::None
    }

    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.open {
            return;
        }
        let b = self.bounds;
        // 그림자 느낌의 테두리 2겹
        dc.fill_rect(Rect::new(b.x - 1, b.y - 1, b.w + 2, b.h + 2), th.border);
        dc.fill_rect(b, th.chrome_bg);
        self.input.paint(dc, th);
        let rr = self.rows_rect();
        let pad = (8.0 * self.scale) as i32;
        let th_px = dc.text_height();
        if self.matches.is_empty() {
            dc.text(
                rr.x + pad,
                rr.y + (self.row_h - th_px) / 2,
                rr,
                t(Msg::PalNoMatch),
                th.text_dim,
            );
            return;
        }
        for (row, &i) in self.matches.iter().take(MAX_ROWS).enumerate() {
            let y = rr.y + row as i32 * self.row_h;
            let r = Rect::new(rr.x, y, rr.w, self.row_h);
            if row == self.sel {
                dc.fill_rect(r, th.sel_bg);
            }
            dc.text(
                r.x + pad,
                y + (self.row_h - th_px) / 2,
                r,
                &self.cmds[i].1,
                th.text,
            );
        }
    }
}

/// 퍼지 점수 — 대소문자 무관 부분열 매치. 연속 매치·단어 첫 글자 매치에 가산 · 없으면 None. 빈 질의 = 0.
pub(crate) fn fuzzy_score(query: &str, label: &str) -> Option<i32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let l: Vec<char> = label.to_lowercase().chars().collect();
    let mut score = 0i32;
    let mut qi = 0usize;
    let mut prev_hit = false;
    for (i, &c) in l.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            score += 1;
            if prev_hit {
                score += 3;
            }
            if i == 0 || !l[i - 1].is_alphanumeric() {
                score += 2;
            }
            prev_hit = true;
            qi += 1;
        } else {
            prev_hit = false;
        }
    }
    (qi == q.len()).then_some(score)
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn subsequence_and_ranking() {
        assert!(fuzzy_score("syntax", "Set Syntax: SQL").is_some());
        assert!(fuzzy_score("xyz", "Set Syntax: SQL").is_none());
        let a = fuzzy_score("sql", "Set Syntax: SQL").unwrap_or(0);
        let b = fuzzy_score("sql", "Set Syntax: Plain Text").unwrap_or(-1);
        assert!(a > b);
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }
}
