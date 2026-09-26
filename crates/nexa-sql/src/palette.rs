//! 명령 팔레트(사용자 09-14 · Sublime `Ctrl+⇧P`) — 입력 한 줄 + 퍼지 필터 목록.
//!
//! 명령 = 메뉴 항목 전부(`File: New` …) + `Set Syntax: <이름>`(레지스트리) — 호스트가 [`Palette::set_commands`]로 준다.
//! 키: ↑↓ 이동 · Enter 실행 · Esc 닫기 · 바깥 클릭 닫기 · 행 클릭 실행.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};

pub(crate) enum PaletteAction {
    None,
    Pick(String),
    /// 프롬프트 모드의 입력 확정(`open_prompt`의 id · 입력 글자).
    Prompt {
        id: String,
        text: String,
    },
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
    /// 보이는 첫 행(전체 결과 기준) — 결과가 `MAX_ROWS`보다 많으면 ↑/↓·휠로 굴린다(사용자 09-19 마우스 보완).
    top: usize,
    /// 마지막으로 본 마우스 위치 — 같은 자리의 MouseMove(키보드 이동 직후 재발행)로 선택이 되돌아가지 않게.
    last_mouse: (i32, i32),
    bounds: Rect,
    row_h: i32,
    scale: f32,
    last_query: String,
    /// `:` 로 시작하는 질의 = 줄 이동 모드(Goto Anything · T-96) — 숫자가 있으면 그 줄.
    goto: Option<Option<usize>>,
    /// 프롬프트 모드(탭 이름 바꾸기 등 · 09-17): 목록 없이 글자 입력만 · Enter = [`PaletteAction::Prompt`].
    prompt: Option<String>,
    /// 프롬프트 모드의 안내 글(입력란 아래 한 줄 — 입력란에 이미 글이 있어 자리표시자가 보이지 않는다).
    hint: String,
    /// 프롬프트가 **붙을 대상**(이름을 바꾸는 탭의 rect · 사용자 09-21 "요청한 탭 근처에 · 수정 중인 대상을 식별할 수 있게").
    /// 있으면 상자가 그 바로 아래(자리가 없으면 위)에 붙고 대상에 강조 테두리를 그린다 · 없으면 종전처럼 창 위 가운데.
    anchor: Option<Rect>,
    /// 창 크기(물리 px) — 붙는 상자를 창 안에 두는 데 쓴다.
    win: (i32, i32),
}

const MAX_ROWS: usize = 12;

impl Palette {
    pub(crate) fn new() -> Self {
        Palette {
            open: false,
            input: TextBox::new(t(Msg::PhPalette)).with_clearable(),
            cmds: Vec::new(),
            matches: Vec::new(),
            sel: 0,
            top: 0,
            last_mouse: (i32::MIN, i32::MIN),
            bounds: Rect::new(0, 0, 0, 0),
            row_h: 26,
            scale: 1.0,
            last_query: String::new(),
            goto: None,
            prompt: None,
            hint: String::new(),
            anchor: None,
            win: (0, 0),
        }
    }

    /// 프롬프트 모드로 열기 — `id`는 확정 때 그대로 돌려준다 · `placeholder` 안내 · `initial` 초기 글자(전체 선택).
    /// [`Self::open_prompt`] + **대상 옆에 붙이기** — `anchor` = 이름을 바꾸는 탭의 rect.
    pub(crate) fn open_prompt_at(
        &mut self,
        id: &str,
        placeholder: &str,
        initial: &str,
        anchor: Option<Rect>,
    ) {
        self.anchor = anchor;
        self.open_prompt(id, placeholder, initial);
    }

    /// 창 크기(붙는 상자의 경계).
    pub(crate) fn set_window(&mut self, w: i32, h: i32) {
        self.win = (w, h);
    }

    pub(crate) fn open_prompt(&mut self, id: &str, placeholder: &str, initial: &str) {
        self.open = true;
        self.prompt = Some(id.to_string());
        self.hint = placeholder.to_string();
        self.input = TextBox::new(placeholder).with_text(initial);
        self.input.set_scale(self.scale);
        self.input.set_focused(true);
        let mut inv = Invalidations::default();
        self.input.on_event(&InputEvent::SelectAll, &mut inv);
        self.layout(&mut inv);
        self.matches.clear();
        self.sel = 0;
    }

    pub(crate) fn set_commands(&mut self, cmds: Vec<(String, String)>) {
        self.cmds = cmds;
        self.refilter(true);
    }

    /// IME 조합 중 글자(preedit)를 입력 상자에 보인다 — 팔레트는 `Focus` 변형이 아니라 호스트가 IME 사건을 여기로 넘긴다(09-27 한글 버그).
    pub(crate) fn set_preedit(&mut self, text: &str, inv: &mut Invalidations) {
        self.input.set_preedit(text, inv);
    }

    /// 지금 검색어(시험).
    #[cfg(test)]
    pub(crate) fn query(&self) -> String {
        self.input.text()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self, prefill: &str) {
        self.open = true;
        // × 지우기 = 검색 입력란 공통(명령 검색 · 글이 있을 때만 · 사용자 09-23).
        self.input = TextBox::new(t(Msg::PhPalette))
            .with_clearable()
            .with_text(prefill);
        self.input.set_scale(self.scale);
        self.input.set_focused(true);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
        self.last_query.clear();
        self.refilter(true);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.prompt = None;
        self.anchor = None;
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

    /// ★ 지금 모드의 **실제 상자**(사용자 09-21 "이름 바꾸기는 간단한 형태로 — 불필요한 하단이 함께 표시된다"): 프롬프트(이름
    /// 바꾸기 · 저장소 추가)는 목록이 없으므로 **입력란 + 안내 한 줄**만 · 폭도 좁게. 명령 팔레트·줄 이동은 종전 크기.
    fn frame(&self) -> Rect {
        let b = self.bounds;
        if self.prompt.is_none() {
            return b;
        }
        let pad = (6.0 * self.scale) as i32;
        let w = ((380.0 * self.scale) as i32).min(b.w);
        let h = pad + self.row_h + pad / 2 + self.row_h * 3 / 4 + pad;
        match self.anchor {
            // 대상 탭 **바로 아래**(왼쪽 끝을 맞춘다) — 자리가 없으면(결과 탭이 창 아래쪽) **바로 위** · 그래도 안 되면 창 안으로
            // 밀어 넣는다(팝업 배치 규칙 · nexa-ctl `place_popup`과 같은 순서를 세로축에 · 가로는 밀어 넣기).
            Some(a) if self.win.0 > 0 && self.win.1 > 0 => {
                let gap = (3.0 * self.scale) as i32;
                let below = a.bottom() + gap;
                let y = if below + h <= self.win.1 {
                    below
                } else if a.y - gap - h >= 0 {
                    a.y - gap - h
                } else {
                    (self.win.1 - h).max(0)
                };
                let host = Rect::new(0, 0, self.win.0, self.win.1);
                nexa_ctl::geom::nudge_into(Rect::new(a.x, y, w, h), host)
            }
            _ => Rect::new(b.x + (b.w - w) / 2, b.y, w, h),
        }
    }

    fn layout(&mut self, inv: &mut Invalidations) {
        let b = self.frame();
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

    /// 점 아래 결과 행(전체 결과 기준 인덱스 · 목록 밖/빈 행 = None).
    fn row_at(&self, p: Point) -> Option<usize> {
        let rr = self.rows_rect();
        if self.goto.is_some() || self.prompt.is_some() || !rr.contains(p) {
            return None;
        }
        let row = ((p.y - rr.y) / self.row_h.max(1)) as usize;
        let i = self.top + row;
        (row < MAX_ROWS && i < self.matches.len()).then_some(i)
    }

    /// 선택 행이 보이도록 굴린다.
    fn reveal_sel(&mut self) {
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + MAX_ROWS {
            self.top = self.sel + 1 - MAX_ROWS;
        }
    }

    fn refilter(&mut self, force: bool) {
        if self.prompt.is_some() {
            self.matches.clear();
            return;
        }
        let q = self.input.text();
        if !force && q == self.last_query {
            return;
        }
        self.last_query = q.clone();
        // `:123` = 줄 이동(항목 필터 대신 안내 한 줄).
        if let Some(rest) = q.trim().strip_prefix(':') {
            self.goto = Some(rest.trim().parse::<usize>().ok().filter(|n| *n > 0));
            self.matches.clear();
            self.sel = 0;
            self.top = 0;
            return;
        }
        self.goto = None;
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
        self.top = 0;
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
                if let Some(id) = self.prompt.clone() {
                    let text = self.input.text().trim().to_string();
                    return if text.is_empty() {
                        PaletteAction::Close
                    } else {
                        PaletteAction::Prompt { id, text }
                    };
                }
                if let Some(g) = self.goto {
                    return match g {
                        Some(n) => PaletteAction::Pick(format!("goto.line:{n}")),
                        None => PaletteAction::None,
                    };
                }
                return match self.matches.get(self.sel) {
                    Some(&i) => PaletteAction::Pick(self.cmds[i].0.clone()),
                    None => PaletteAction::Close,
                };
            }
            InputEvent::Key { key: Key::Up, .. } => {
                self.sel = self.sel.saturating_sub(1);
                self.reveal_sel();
                return PaletteAction::None;
            }
            InputEvent::Key { key: Key::Down, .. } => {
                if self.sel + 1 < self.matches.len() {
                    self.sel += 1;
                }
                self.reveal_sel();
                return PaletteAction::None;
            }
            InputEvent::Key {
                key: Key::PageUp, ..
            } => {
                self.sel = self.sel.saturating_sub(MAX_ROWS);
                self.reveal_sel();
                return PaletteAction::None;
            }
            InputEvent::Key {
                key: Key::PageDown, ..
            } => {
                self.sel = (self.sel + MAX_ROWS).min(self.matches.len().saturating_sub(1));
                self.reveal_sel();
                return PaletteAction::None;
            }
            // ★ 마우스(사용자 09-19): 목록 위에서 움직이면 그 행이 선택(키보드 선택과 같은 강조 하나) · 휠 = 목록 굴리기 ·
            //   클릭 = 실행. 같은 자리의 MouseMove는 무시한다(키보드로 옮긴 선택이 커서 아래 행으로 튀지 않게).
            InputEvent::MouseMove { x, y } => {
                if (x, y) != self.last_mouse {
                    self.last_mouse = (x, y);
                    if let Some(i) = self.row_at(Point { x, y }) {
                        self.sel = i;
                    }
                }
                return PaletteAction::None;
            }
            InputEvent::Wheel { delta } => {
                let rows = if delta > 0 { -3isize } else { 3 };
                let max_top = self.matches.len().saturating_sub(MAX_ROWS);
                self.top = (self.top as isize + rows).clamp(0, max_top as isize) as usize;
                // 선택은 보이는 범위 안으로(커서가 목록 위에 있으면 그 행).
                let under = self.row_at(Point {
                    x: self.last_mouse.0,
                    y: self.last_mouse.1,
                });
                self.sel = under.unwrap_or_else(|| {
                    self.sel
                        .clamp(self.top, (self.top + MAX_ROWS).saturating_sub(1))
                        .min(self.matches.len().saturating_sub(1))
                });
                return PaletteAction::None;
            }
            // 바깥 **우클릭**도 취소(사용자 09-21 — 좌클릭만 닫히고 우클릭은 상자가 남았다). 상자 안의 우클릭은 입력란의 편집 메뉴로.
            InputEvent::RightDown { x, y } if !self.frame().contains(Point { x, y }) => {
                return PaletteAction::Close;
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if !self.frame().contains(p) {
                    return PaletteAction::Close;
                }
                if self.rows_rect().contains(p) {
                    if let Some(i) = self.row_at(p) {
                        return PaletteAction::Pick(self.cmds[self.matches[i]].0.clone());
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
        let b = self.frame();
        // 그림자 느낌의 테두리 2겹
        dc.fill_rect(Rect::new(b.x - 1, b.y - 1, b.w + 2, b.h + 2), th.border);
        dc.fill_rect(b, th.chrome_bg);
        self.input.paint(dc, th);
        if let (Some(a), true) = (self.anchor, self.prompt.is_some()) {
            // ★ 수정 중인 대상 강조: 대상 탭에 **강조색 테두리**(2px) + 상자의 그쪽 변도 같은 색 — 어느 탭의 이름을 바꾸는지
            //   테두리 색이 이어 준다(탭이 여럿이어도 헷갈리지 않는다).
            let w2 = (2.0 * self.scale).round().max(2.0) as i32;
            for r in [
                Rect::new(a.x, a.y, a.w, w2),
                Rect::new(a.x, a.bottom() - w2, a.w, w2),
                Rect::new(a.x, a.y, w2, a.h),
                Rect::new(a.right() - w2, a.y, w2, a.h),
            ] {
                dc.fill_rect(r, th.accent);
            }
            let edge_y = if b.y >= a.bottom() {
                b.y - 1
            } else {
                b.bottom() - w2 + 1
            };
            dc.fill_rect(Rect::new(b.x - 1, edge_y, b.w + 2, w2), th.accent);
        }
        if self.prompt.is_some() {
            // 간단한 입력 상자: 입력란 아래 안내 한 줄(Enter 적용 · Esc 취소)만 — 목록 영역은 그리지 않는다.
            let pad = (6.0 * self.scale) as i32;
            let th_px = dc.text_height();
            let y = b.y + pad + self.row_h + pad / 2;
            dc.text(
                b.x + pad + (4.0 * self.scale) as i32,
                y + (self.row_h * 3 / 4 - th_px) / 2,
                b,
                &self.hint,
                th.text_dim,
            );
            return;
        }
        let rr = self.rows_rect();
        let pad = (8.0 * self.scale) as i32;
        let th_px = dc.text_height();
        if let Some(g) = self.goto {
            let (text, color) = match g {
                Some(n) => (tf(Msg::PalGotoLine, &[&n.to_string()]), th.text),
                None => (t(Msg::PalGotoLineHint).to_string(), th.text_dim),
            };
            let r = Rect::new(rr.x, rr.y, rr.w, self.row_h);
            if g.is_some() {
                dc.fill_rect(r, th.sel_bg);
            }
            // 긴 항목(경로 · 최근 프로젝트)은 가운데 … — Alt = 전체(사용자 09-22).
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &text, r.w - pad * 2);
            dc.text(r.x + pad, rr.y + (self.row_h - th_px) / 2, r, &shown, color);
            return;
        }
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
        for (row, &i) in self
            .matches
            .iter()
            .skip(self.top)
            .take(MAX_ROWS)
            .enumerate()
        {
            let y = rr.y + row as i32 * self.row_h;
            let r = Rect::new(rr.x, y, rr.w, self.row_h);
            if self.top + row == self.sel {
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
    use super::*;
    /// 09-27(사용자 Linux 실기): 팔레트 열림 중 IME 확정 글자(`Ime::Commit`)가 편집기로 새고 팔레트엔 안 들어갔다 → 호스트가
    /// `Char` 사건으로 팔레트에 넘긴다. 여기서는 팔레트가 한글 글자를 검색어로 받고 그 글자로 거르는지 본다.
    #[test]
    fn hangul_chars_reach_the_query_and_filter() {
        let mut p = Palette::new();
        p.set_commands(vec![
            ("view.log".into(), "보기: 로그 창".into()),
            ("file.new".into(), "새 편집기".into()),
        ]);
        p.open("");
        let mut inv = Invalidations::default();
        for c in "로그".chars() {
            p.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
        }
        assert_eq!(p.query(), "로그");
        assert_eq!(p.matches.len(), 1, "'로그'로 거른 결과 = 로그 창 하나");
        assert_eq!(p.cmds[p.matches[0]].0, "view.log");
        p.set_preedit("ㅊ", &mut inv);
        assert_eq!(p.query(), "로그", "조합 중 글자는 본문이 아니다");
    }

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

    /// 이름 바꾸기 상자(프롬프트 · 사용자 09-21): 대상 탭 바로 아래에 붙는다(아래에 자리가 없으면 위) · 상자는 입력란 + 안내 한 줄 ·
    /// **바깥 좌클릭도 우클릭도 취소** · 상자 안의 클릭은 취소가 아니다.
    #[test]
    fn prompt_is_anchored_compact_and_outside_clicks_cancel() {
        use super::{Palette, PaletteAction};
        use nexa_ctl::geom::Rect;
        use nexa_ctl::{InputEvent, Invalidations};
        let mut p = Palette::new();
        p.set_bounds(1000, 40, 1.0);
        p.set_window(1000, 700);
        let mut inv = Invalidations::default();
        let tab = Rect::new(120, 90, 110, 28);
        p.open_prompt_at("tab.rename:1", "hint", "Script_1", Some(tab));
        let f = p.frame();
        assert_eq!(
            (f.x, f.y),
            (tab.x, tab.bottom() + 3),
            "탭 바로 아래 · 왼쪽 끝 맞춤"
        );
        assert!(f.h < 26 * 4, "목록 영역이 없다: {f:?}");
        let inside = InputEvent::RightDown {
            x: f.x + 10,
            y: f.y + 10,
        };
        assert!(!matches!(
            p.on_event(&inside, &mut inv),
            PaletteAction::Close
        ));
        let outside = InputEvent::RightDown { x: 900, y: 600 };
        assert!(matches!(
            p.on_event(&outside, &mut inv),
            PaletteAction::Close
        ));
        let left = InputEvent::MouseDown {
            x: 900,
            y: 600,
            shift: false,
            primary: false,
        };
        assert!(matches!(p.on_event(&left, &mut inv), PaletteAction::Close));
        // 창 아래쪽의 탭(결과 탭) = 상자가 탭 위로.
        p.close();
        let low = Rect::new(80, 680, 90, 18);
        p.open_prompt_at("result.rename:7", "hint", "Result 1", Some(low));
        let f = p.frame();
        assert!(f.bottom() <= low.y, "아래에 자리가 없으면 위로: {f:?}");
        // 대상을 모르면 종전처럼 위 가운데.
        p.close();
        p.open_prompt_at("x", "hint", "", None);
        assert_eq!(p.frame().y, p.bounds.y);
    }

    /// 마우스(사용자 09-19): 목록 위에서 움직이면 그 행 선택 · 같은 자리 MouseMove는 무시 · 휠 = 굴리기 · 클릭 = 실행 ·
    /// ↓는 보이는 행 수를 넘어 끝까지(목록이 따라 굴러간다).
    #[test]
    fn mouse_hover_wheel_click_and_scrolling() {
        use super::{Palette, PaletteAction, MAX_ROWS};
        use nexa_ctl::{InputEvent, Invalidations, Key};
        let mut p = Palette::new();
        p.set_commands(
            (0..40)
                .map(|i| (format!("cmd.{i}"), format!("Command {i:02}")))
                .collect(),
        );
        p.set_bounds(1000, 40, 1.0);
        p.open("");
        let mut inv = Invalidations::default();
        let rr = p.rows_rect();
        let at = |row: i32| InputEvent::MouseMove {
            x: rr.x + 10,
            y: rr.y + row * p.row_h + 3,
        };
        let ev = at(3);
        p.on_event(&ev, &mut inv);
        assert_eq!(p.sel, 3, "hover = 선택");
        // 키보드로 옮긴 뒤 같은 자리 MouseMove가 와도 선택은 그대로.
        p.on_event(
            &InputEvent::Key {
                key: Key::Down,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert_eq!(p.sel, 4);
        p.on_event(&ev, &mut inv);
        assert_eq!(p.sel, 4, "같은 자리 = 무시");
        // 휠 아래 = 3행 굴림 · 선택은 커서 아래 행.
        p.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv);
        assert_eq!(p.top, 3);
        assert_eq!(p.sel, 6, "커서 아래 행(3행째) = top 3 + 3");
        // ↓를 끝까지 — 보이는 행 수를 넘어 마지막 항목까지.
        for _ in 0..100 {
            p.on_event(
                &InputEvent::Key {
                    key: Key::Down,
                    shift: false,
                    primary: false,
                },
                &mut inv,
            );
        }
        assert_eq!(p.sel, 39);
        assert_eq!(p.top, 40 - MAX_ROWS);
        // 클릭 = 그 행 실행(굴린 위치 기준).
        let click = InputEvent::MouseDown {
            x: rr.x + 10,
            y: rr.y + 3,
            shift: false,
            primary: false,
        };
        assert!(matches!(
            p.on_event(&click, &mut inv),
            PaletteAction::Pick(id) if id == format!("cmd.{}", 40 - MAX_ROWS)
        ));
    }
}
