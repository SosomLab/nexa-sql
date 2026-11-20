//! ★ **Output 탭**(09-30 · 사용자 "일반 편집 탭에서도 프로시저 실행·PRINT 메시지를 볼 수 있는 Output 탭 개념") — 결과 영역의
//! 메시지 탭. 다른 클라이언트 대조: SSMS **Messages**(문장마다 "(N rows affected)" · PRINT · 오류) · DBeaver **Output**(DBMS_OUTPUT ·
//! 서버 출력) · PL/SQL Developer **Output**(DBMS_OUTPUT · 컴파일 결과) · TablePlus **Messages**.
//!
//! 기본 기능 = 줄 누적(시각 `HH:MM:SS` · 종류 표식 `⚠`/`✖`) · 오래된 줄부터 버림(`output.max_lines`) · 새 줄이 오면 끝으로 스크롤 ·
//! 드래그 선택 · 복사(⌘/Ctrl+C · 우클릭 메뉴 · 선택 없으면 전체) · 전체 선택 · 머리 줄의 **Clear** · **Copy all** · 읽기 전용
//! 고정폭 `TextBox` 하나(뷰 탭 `ext_view`와 같은 부품 · 새 컨트롤 0). 편집기 탭마다 하나(그 탭의 결과 패널이 가진다 · 닫아도
//! 본문은 남는다 = View ▸ Output으로 다시 연다).
//!
//! 표시 정책은 호스트(`app/output.rs`): `output.show` off/auto/errors/always · `output.activate` never/no_results/always.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TextBox, Widget};
use nsql_i18n::{t, Msg};
use std::collections::VecDeque;
use std::time::Instant;

/// 줄 종류 — 표식·색은 글자로만(목록은 텍스트 색만 · 강조는 chip · 사용자 09-30 머티리얼 규칙).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutKind {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    Clear,
    CopyAll,
}

/// ★ 오류 줄의 닻(T-492 ① · 11-20): Output 글에 굳힌 줄 번호는 **오류가 온 순간**의 것이라, 그 뒤 위쪽에 줄을 넣거나 지우면 두 번
/// 클릭이 빗나갔다 → 글의 (줄, 열)마다 그때의 탭·버퍼 변경 번호를 기억해 두고, 이동할 때 그 뒤의 줄 변경을 접어 지금 줄을 구한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ErrAnchor {
    /// 실행한 편집기 탭 id.
    pub tab: u64,
    /// Output 글에 찍힌 줄(1부터) · 열(1부터 · 없으면 문장 시작 줄).
    pub line: usize,
    pub col: Option<usize>,
    /// 그 순간 버퍼의 세대·변경 번호(`TextBuf::epoch` · `change_seq`).
    pub epoch: u64,
    pub seq: u64,
}

/// 기억하는 닻의 상한(오래된 것부터 버린다 · 닻이 없으면 글의 숫자 그대로 = 종전 동작).
const ANCHOR_MAX: usize = 256;

/// 닻의 줄이 지금 어디인가(1부터) — 버퍼가 통째로 바뀌었거나(세대) 변경 기록이 끊겼으면 `None`(글의 숫자로).
/// 줄 단위 추적이다: 지워진 줄은 그 자리의 경계 줄로 · 같은 줄 안의 편집은 열을 옮기지 않는다(줄 끝에서 자른다).
pub(crate) fn follow_anchor(tb: &TextBox, a: &ErrAnchor) -> Option<usize> {
    let buf = tb.buf();
    if buf.epoch() != a.epoch || a.line == 0 {
        return None;
    }
    let line0 = buf.changes_since(a.seq)?.fold(a.line - 1, |l, c| {
        nsql_bookmarks::shift_line(l, c.first, c.removed, c.inserted).0
    });
    Some(line0 + 1)
}

pub(crate) struct OutputView {
    /// 오류 줄의 닻(새 것이 뒤 · [`ErrAnchor`]).
    anchors: Vec<ErrAnchor>,
    bounds: Rect,
    scale: f32,
    body: TextBox,
    lines: VecDeque<String>,
    max_lines: usize,
    timestamps: bool,
    /// 본문 상자에 아직 반영하지 않은 변경이 있다(그릴 때 한 번에).
    dirty: bool,
    buttons: Vec<(Btn, Rect)>,
    hover: Option<usize>,
    pressed: Option<usize>,
    head_bottom: i32,
    /// 마지막으로 호스트가 거둔 뒤 추가된 메시지 수(탭 제목 배지 `Output (N)` · 10-01).
    unread: usize,
    /// 본문 마지막 MouseDown(시각 · y) — 두 번 클릭 판정.
    last_down: Option<(Instant, i32)>,
    dblclick_ms: u64,
    /// 두 번 클릭한 줄이 `N행:`/`line N:`(또는 `N행 M열:`/`line N, col M:`)이면 그 줄·열(호스트가 거둬 편집기로 이동).
    goto: Option<(usize, Option<usize>)>,
    /// 우클릭 메뉴에서 고른 호스트 명령 id.
    cmd: Option<String>,
}

impl OutputView {
    pub(crate) fn new(max_lines: usize, timestamps: bool) -> Self {
        let mut body = TextBox::new("").with_multiline();
        body.set_read_only(true);
        body.set_wrap(true);
        body.set_line_numbers(false);
        body.set_gutter_marks(false);
        // ★ T-266 줄 단위 색(10-30 · 사용자 결정): 줄 앞 표식(✖ · ⚠)으로 오류 = danger · 경고 = warn · 여러 줄 메시지의 이어진 줄(들여씀)도 같은 색.
        body.set_highlighter(Some(std::rc::Rc::new(OutHighlighter)));
        body.set_minimap(false);
        body.set_focus_ring(false);
        // 우클릭 메뉴는 팝업 층에서(`paint_popup` · 호스트가 맨 마지막에 부른다).
        body.set_popup_deferred(true);
        // ★ 우클릭 "AI에게 수정 요청"(app/ai_editor.rs · 11-12) — 고른 id는 호스트가 `take_cmd`로 거둬 `menu_action`에.
        body.set_menu_extras(vec![nexa_ctl::controls::CtxItem::item(
            "ai.fix_error",
            t(Msg::MnAiFixError),
        )]);
        Self {
            anchors: Vec::new(),
            bounds: Rect::default(),
            scale: 1.0,
            body,
            lines: VecDeque::new(),
            max_lines: max_lines.max(1),
            timestamps,
            dirty: false,
            buttons: Vec::new(),
            hover: None,
            pressed: None,
            head_bottom: 0,
            unread: 0,
            last_down: None,
            dblclick_ms: 400,
            goto: None,
            cmd: None,
        }
    }

    /// 두 번 클릭 간격(설정 `ui.dblclick_ms`).
    /// ★ T-266 Output 안 찾기(10-30): 본문 상자(찾기 표식·선택·스크롤을 호스트의 찾기 막대가 그대로 쓴다).
    pub(crate) fn body(&self) -> &TextBox {
        &self.body
    }

    pub(crate) fn body_mut(&mut self) -> &mut TextBox {
        &mut self.body
    }

    pub(crate) fn set_dblclick_ms(&mut self, ms: u64) {
        self.dblclick_ms = ms.max(100);
    }

    /// 거두지 않은 새 메시지 수(거두면 0).
    pub(crate) fn take_unread(&mut self) -> usize {
        std::mem::take(&mut self.unread)
    }

    /// 두 번 클릭으로 고른 오류 줄(편집기 줄 번호 · 1부터).
    /// ★ 전체 선택(Ctrl+A · 사용자 11-12 "Output에서 Ctrl+A·Ctrl+C가 안 된다"): 호스트의 `edit.select_all`이 Output 활성일 때 부른다.
    pub(crate) fn select_all(&mut self) {
        let mut inv = Invalidations::default();
        let len = self.body.text().chars().count();
        self.body.select_range(0, len, &mut inv);
    }

    /// 복사할 글(Ctrl+C): 선택 영역 · 없으면 전체(포커스 여부와 무관).
    pub(crate) fn copy_text(&self) -> String {
        let text = self.body.text();
        match self.body.selection() {
            Some((a, b)) if a != b => {
                let (lo, hi) = (a.min(b), a.max(b));
                text.chars().skip(lo).take(hi - lo).collect()
            }
            _ => text,
        }
    }

    /// 우클릭 메뉴에서 고른 호스트 명령(1회성).
    pub(crate) fn take_cmd(&mut self) -> Option<String> {
        self.cmd.take()
    }

    /// 자체 시험(기동 명령 `output.goto_error:<n>` · 11-19): 뒤에서 `nth`번째(1부터) 오류 머리줄을 **두 번 클릭한 것과 같은** 이동 요청.
    pub(crate) fn goto_error_nth(&mut self, nth: usize) -> bool {
        self.goto = self
            .lines
            .iter()
            .rev()
            .filter_map(|l| parse_error_line(l))
            .nth(nth.saturating_sub(1));
        self.goto.is_some()
    }

    pub(crate) fn take_goto(&mut self) -> Option<(usize, Option<usize>)> {
        self.goto.take()
    }

    /// 방금 넣은 오류 줄의 닻을 기억한다(호스트 · T-492 ①).
    pub(crate) fn note_anchor(&mut self, a: ErrAnchor) {
        if self.anchors.len() >= ANCHOR_MAX {
            self.anchors.remove(0);
        }
        self.anchors.push(a);
    }

    /// 글에 찍힌 (줄, 열)의 닻 — 같은 글이 여럿이면 가장 최근 것.
    pub(crate) fn anchor_for(&self, line: usize, col: Option<usize>) -> Option<ErrAnchor> {
        self.anchors
            .iter()
            .rev()
            .find(|a| a.line == line && a.col == col)
            .copied()
    }

    /// ★ 캐럿 줄이 속한 오류 메시지의 위치(11-19): 그 줄이 오류 머리줄이 아니면(여러 줄 메시지의 이어지는 줄 · `위치:` 줄)
    /// **들여쓴 줄을 거슬러 올라가** 머리줄을 찾는다 — 메시지 어느 줄을 두 번 눌러도 같은 자리로.
    fn error_line_at_caret(&self) -> Option<(usize, Option<usize>)> {
        let b = self.body.buf();
        let mut line = b.line_of(self.body.caret());
        loop {
            let text = b.line_text(line);
            if let Some(hit) = parse_error_line(&text) {
                return Some(hit);
            }
            // 이어지는 줄 = 공백으로 시작(머리줄은 `[시각]` 또는 표식으로 시작).
            if line == 0 || !text.starts_with(' ') {
                return None;
            }
            line -= 1;
        }
    }

    /// 설정이 바뀌면(`output.max_lines` · `output.timestamps`).
    pub(crate) fn set_limits(&mut self, max_lines: usize, timestamps: bool) {
        self.max_lines = max_lines.max(1);
        self.timestamps = timestamps;
        self.trim();
    }

    /// 한 메시지(여러 줄이면 이어지는 줄은 시각 폭만큼 들여쓴다).
    pub(crate) fn push(&mut self, kind: OutKind, msg: &str) {
        self.push_at(kind, msg, now_hms());
    }

    fn push_at(&mut self, kind: OutKind, msg: &str, stamp: String) {
        let stamp = if self.timestamps {
            format!("[{stamp}] ")
        } else {
            String::new()
        };
        let mark = match kind {
            OutKind::Info => "",
            OutKind::Warn => "⚠ ",
            OutKind::Error => "✖ ",
        };
        let indent = " ".repeat(stamp.chars().count());
        let msg = msg.trim_end_matches(['\n', '\r']);
        let msg = if msg.is_empty() { " " } else { msg };
        for (i, l) in msg.lines().enumerate() {
            let l = l.trim_end_matches('\r');
            if i == 0 {
                self.lines.push_back(format!("{stamp}{mark}{l}"));
            } else {
                self.lines.push_back(format!("{indent}{l}"));
            }
        }
        self.unread += 1;
        self.trim();
        self.dirty = true;
    }

    fn trim(&mut self) {
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
            self.dirty = true;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.anchors.clear();
        self.dirty = true;
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// 본문 전체(줄바꿈 `\n`).
    pub(crate) fn text(&self) -> String {
        let mut s = String::new();
        for l in &self.lines {
            s.push_str(l);
            s.push('\n');
        }
        s
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        if (scale - self.scale).abs() > f32::EPSILON {
            self.body.set_scale(scale);
        }
        self.scale = scale;
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.body.set_focused(on);
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// 본문 상자에 줄을 반영하고 끝으로 스크롤(새 줄이 왔을 때만 — 읽는 중 위치는 그대로).
    fn sync(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let text = self.text();
        self.body.set_text_keep_view(&text);
        let n = self.lines.len().max(1);
        self.body.goto_line(n);
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        let mut inv = Invalidations::default();
        if self.body.popup_open() {
            self.body.on_event(ev, &mut inv);
            self.after_body();
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                let h = self.buttons.iter().position(|(_, r)| r.contains(p));
                let changed = h != self.hover;
                self.hover = h;
                self.body.on_event(ev, &mut inv);
                changed || !inv.is_empty()
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                self.pressed = self.buttons.iter().position(|(_, r)| r.contains(p));
                if self.pressed.is_none() && y >= self.head_bottom {
                    self.body.set_focused(true);
                    self.body.on_event(ev, &mut inv);
                    // ★ 두 번 클릭 = 오류 줄(`N행:`)이면 편집기 그 줄로(10-01 · T-266).
                    let now = Instant::now();
                    let dbl = self.last_down.is_some_and(|(t, ly)| {
                        now.duration_since(t).as_millis() < u128::from(self.dblclick_ms)
                            && (ly - y).abs() < 4
                    });
                    self.last_down = Some((now, y));
                    if dbl {
                        self.goto = self.error_line_at_caret();
                        self.last_down = None;
                    }
                }
                true
            }
            InputEvent::RightDown { y, .. } => {
                if y >= self.head_bottom {
                    self.body.set_focused(true);
                    self.body.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                if let Some(i) = self.pressed.take() {
                    if let Some((b, r)) = self.buttons.get(i).copied() {
                        if r.contains(p) {
                            match b {
                                Btn::Clear => self.clear(),
                                Btn::CopyAll => {
                                    let _ = crate::clipboard::write_text(&self.text());
                                }
                            }
                        }
                    }
                    return true;
                }
                self.body.on_event(ev, &mut inv);
                self.after_body();
                true
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => {
                self.body.on_event(ev, &mut inv);
                true
            }
            InputEvent::Key { .. } | InputEvent::SelectAll => {
                self.body.set_focused(true);
                self.body.on_event(ev, &mut inv);
                self.after_body();
                true
            }
            _ => false,
        }
    }

    fn after_body(&mut self) {
        if let Some(a) = self.body.take_edit_ctx() {
            match a {
                nexa_ctl::EditCtxAction::Copy => {
                    let text = self
                        .body
                        .copy_selection()
                        .unwrap_or_else(|| self.body.text());
                    let _ = crate::clipboard::write_text(&text);
                }
                nexa_ctl::EditCtxAction::Custom(id) => self.cmd = Some(id),
                _ => {}
            }
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        self.sync();
        dc.fill_rect(b, th.window_bg);
        // 머리 줄: `Output · N lines` 왼쪽 · Clear · Copy all 오른쪽(글자 버튼 · hover = 강조색).
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let pad = self.s(8.0);
        let head_h = lh + self.s(8.0);
        let head = Rect::new(b.x, b.y, b.w, head_h);
        dc.fill_rect(head, th.chrome_bg);
        dc.fill_rect(Rect::new(b.x, head.bottom() - 1, b.w, 1), th.border);
        let ty = dc.text_center_y(b.y, head_h);
        let label = format!("{} · {}", t(Msg::ResultTabOutput), self.lines.len());
        dc.text(b.x + pad, ty, head, &label, th.text_dim);
        self.buttons.clear();
        let mut x = b.right() - pad;
        for (i, (btn, msg)) in [(Btn::CopyAll, Msg::OutCopyAll), (Btn::Clear, Msg::OutClear)]
            .into_iter()
            .enumerate()
        {
            let s = t(msg);
            let w = dc.text_width(s);
            x -= w;
            let r = Rect::new(x - self.s(4.0), b.y, w + self.s(8.0), head_h);
            // 버튼 순서는 오른쪽부터 그리므로 hover 색인은 buttons 삽입 순(0 = Copy all · 1 = Clear).
            let color = if self.hover == Some(i) {
                th.accent
            } else {
                th.text
            };
            dc.text(x, ty, head, s, color);
            self.buttons.push((btn, r));
            x -= self.s(16.0);
        }
        self.head_bottom = head.bottom();
        let body_rect = Rect::new(b.x, head.bottom(), b.w, (b.h - head_h).max(lh));
        let mut inv = Invalidations::default();
        self.body.set_bounds(body_rect, &mut inv);
        self.body.paint(dc, th);
    }

    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.body.popup_open() {
            self.body.paint_popup(dc, th);
        }
    }
}

/// ★ Output 줄에서 편집기 줄 번호(10-01): `[hh:mm:ss] ✖ 3행: …` · `… line 3: …`(로그 요약 형식 `Msg::LogLineSummary` 두 언어) —
/// 시각·표식 뒤 첫 숫자 묶음이 `행:` 또는 `line N:` 꼴일 때만. 순수 함수(시험 대상).
#[must_use]
/// ★ T-266 Output 전용 하이라이터(10-30): 줄 종류는 글자로만 남아 있으므로(`[시각] ✖ 글` · `⚠ 글` · 이어진 줄 = 들여씀) 줄 머리로 판정한다 —
/// `✖` = [`TokenKind::Error`] · `⚠` = [`TokenKind::Warn`] · 그 밖 = Plain. 상태(`state`) = 직전 메시지의 종류(0 Plain · 1 Error · 2 Warn) ·
/// 공백으로 시작하는 줄(여러 줄 메시지의 이어진 줄)은 그 종류를 잇는다. 순수(시험).
#[derive(Debug)]
struct OutHighlighter;

/// 줄의 종류 판정 — `[hh:mm:ss] ` 시각 접두를 건너뛴 뒤 표식을 본다 · 들여쓴 줄 = 이전 상태 유지.
fn out_line_kind(line: &str, state: u32) -> u32 {
    let b = line.as_bytes();
    if line.starts_with(' ') && !line.trim().is_empty() {
        return state;
    }
    let rest = if b.len() >= 11 && b[0] == b'[' && b[9] == b']' && b[10] == b' ' {
        &line[11..]
    } else {
        line
    };
    if rest.starts_with('✖') {
        1
    } else if rest.starts_with('⚠') {
        2
    } else {
        0
    }
}

impl nexa_ctl::Highlighter for OutHighlighter {
    fn line_spans(&self, line: &str, state: &mut u32, out: &mut Vec<(usize, nexa_ctl::TokenKind)>) {
        let k = out_line_kind(line, *state);
        *state = k;
        let kind = match k {
            1 => nexa_ctl::TokenKind::Error,
            2 => nexa_ctl::TokenKind::Warn,
            _ => nexa_ctl::TokenKind::Plain,
        };
        out.push((line.chars().count(), kind));
    }
    fn name(&self) -> &str {
        "Output"
    }
    fn strings_span_lines(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod hl_tests {
    use super::out_line_kind;

    /// 시각 접두 유무 · ✖/⚠/일반 · 들여쓴 이어진 줄은 이전 종류 · 빈 줄은 Plain.
    #[test]
    fn output_line_kinds() {
        assert_eq!(out_line_kind("[12:34:56] ✖ 3행: 오류", 0), 1);
        assert_eq!(out_line_kind("✖ 오류", 2), 1);
        assert_eq!(out_line_kind("[12:34:56] ⚠ 경고", 0), 2);
        assert_eq!(out_line_kind("⚠ 경고", 0), 2);
        assert_eq!(out_line_kind("[12:34:56] 안내", 1), 0);
        assert_eq!(
            out_line_kind("           이어진 줄", 1),
            1,
            "들여쓴 줄 = 이전 종류"
        );
        assert_eq!(out_line_kind("   계속", 2), 2);
        assert_eq!(out_line_kind("", 1), 0, "빈 줄 = Plain");
        assert_eq!(out_line_kind("보통 글", 0), 0);
    }
}

/// 오류 머리줄(`[시각] ✖ line N: …` · `N행: …` · 열 포함 `line N, col M: …` · `N행 M열: …`) → (줄, 열) — 둘 다 1부터.
pub(crate) fn parse_error_line(line: &str) -> Option<(usize, Option<usize>)> {
    let mut s = line.trim_start();
    if s.starts_with('[') {
        s = s.split_once("] ").map_or(s, |(_, r)| r);
    }
    let s = s.trim_start_matches(|c: char| c == '✖' || c == '⚠' || c.is_whitespace());
    let s = s.strip_prefix("line ").unwrap_or(s);
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &s[digits.len()..];
    let n: usize = digits.parse().ok().filter(|n: &usize| *n > 0)?;
    if rest.starts_with("행:") || rest.starts_with(':') {
        return Some((n, None));
    }
    // 열: `, col M:` · `행 M열:`.
    let (tail, close) = if let Some(t) = rest.strip_prefix(", col ") {
        (t, ":")
    } else {
        (rest.strip_prefix("행 ")?, "열:")
    };
    let cd: String = tail.chars().take_while(char::is_ascii_digit).collect();
    if cd.is_empty() || !tail[cd.len()..].starts_with(close) {
        return None;
    }
    let col: usize = cd.parse().ok().filter(|c: &usize| *c > 0)?;
    Some((n, Some(col)))
}

/// `HH:MM:SS`(로컬).
fn now_hms() -> String {
    let st = nsql_log::now_local().stamp();
    st.get(11..19).unwrap_or(&st).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_trim_and_text() {
        let mut v = OutputView::new(3, true);
        v.push_at(OutKind::Info, "one", "10:00:00".into());
        v.push_at(OutKind::Warn, "two\nsecond line", "10:00:01".into());
        assert_eq!(
            v.text(),
            "[10:00:00] one\n[10:00:01] ⚠ two\n           second line\n"
        );
        assert_eq!(v.line_count(), 3);
        // 상한 3줄 → 오래된 줄부터 버림.
        v.push_at(OutKind::Error, "three", "10:00:02".into());
        assert_eq!(v.line_count(), 3);
        assert!(v.text().starts_with("[10:00:01] ⚠ two\n"), "{}", v.text());
        assert!(v.text().ends_with("[10:00:02] ✖ three\n"));
        v.clear();
        assert!(v.is_empty());
        assert_eq!(v.text(), "");
    }

    /// ★ T-492 ①: 닻은 실행 뒤의 줄 넣기·지우기를 따라간다 · 같은 글이면 최근 것 · 버퍼가 통째로 바뀌면 포기(글의 숫자로).
    #[test]
    fn error_anchor_follows_line_edits() {
        let mut tb = TextBox::new("").with_multiline();
        tb.set_text("a\nb\nc\nd\ne");
        // 포커스가 없으면 붙여넣기가 버려진다.
        tb.set_focused(true);
        let mut v = OutputView::new(100, false);
        let anchor = |tb: &TextBox, line: usize, col: Option<usize>| ErrAnchor {
            tab: 7,
            line,
            col,
            epoch: tb.buf().epoch(),
            seq: tb.buf().change_seq(),
        };
        v.note_anchor(anchor(&tb, 4, Some(1)));
        assert_eq!(v.anchor_for(4, None), None, "열까지 같아야 한다");
        let a = v.anchor_for(4, Some(1)).expect("anchor");
        assert_eq!(follow_anchor(&tb, &a), Some(4), "편집 없음");
        // 위에 두 줄을 넣으면 4 → 6.
        let mut inv = Invalidations::default();
        tb.select_range(0, 0, &mut inv);
        tb.paste("x\ny\n", &mut inv);
        assert_eq!(follow_anchor(&tb, &a), Some(6));
        assert_eq!(tb.buf().line_text(5).trim_end(), "d");
        // 아래쪽 편집은 영향 없음 · 위 한 줄을 지우면 5.
        let end = tb.buf().len();
        tb.select_range(end, end, &mut inv);
        tb.paste("\nz", &mut inv);
        assert_eq!(follow_anchor(&tb, &a), Some(6));
        let l1 = tb.buf().line_start(1);
        tb.select_range(0, l1, &mut inv);
        tb.paste(" ", &mut inv);
        assert_eq!(follow_anchor(&tb, &a), Some(5));
        // 같은 글의 새 닻이 이긴다(다시 실행).
        v.note_anchor(anchor(&tb, 4, Some(1)));
        let b = v.anchor_for(4, Some(1)).expect("anchor");
        assert_ne!(a.seq, b.seq);
        assert_eq!(follow_anchor(&tb, &b), Some(4));
        // 버퍼를 통째로 바꾸면(다시 읽기 등) 따라가지 않는다.
        tb.set_text("q");
        assert_eq!(follow_anchor(&tb, &a), None);
        // 지우기 = 닻도 버린다 · 상한을 넘으면 오래된 것부터.
        v.clear();
        assert_eq!(v.anchor_for(4, Some(1)), None);
        for i in 0..ANCHOR_MAX + 3 {
            v.note_anchor(anchor(&tb, i + 1, None));
        }
        assert_eq!(v.anchor_for(1, None), None);
        assert!(v.anchor_for(ANCHOR_MAX + 3, None).is_some());
    }

    #[test]
    fn error_line_number_is_parsed() {
        assert_eq!(
            parse_error_line("[12:00:00] ✖ 3행: Warning: x"),
            Some((3, None))
        );
        assert_eq!(
            parse_error_line("[12:00:00] ✖ line 12: ORA-00942"),
            Some((12, None))
        );
        assert_eq!(parse_error_line("✖ 7행: boom"), Some((7, None)));
        // ★ 11-19: 열 포함(문서 기준 줄·열).
        assert_eq!(
            parse_error_line("[00:50:02] ✖ line 70, col 54: ORA-00923: x"),
            Some((70, Some(54)))
        );
        assert_eq!(
            parse_error_line("✖ 70행 54열: ORA-00923: x"),
            Some((70, Some(54)))
        );
        assert_eq!(parse_error_line("✖ line 70, col : x"), None);
        assert_eq!(parse_error_line("✖ 70행 열: x"), None);
        assert_eq!(parse_error_line("           at: WITHIN GROUP"), None);
        assert_eq!(
            parse_error_line("[12:00:00] [1] 완료 · 0행 영향 · 12 ms"),
            None
        );
        assert_eq!(parse_error_line("[12:00:00] hello 3행:"), None);
        assert_eq!(parse_error_line(""), None);
    }

    #[test]
    fn no_timestamps_and_empty_message() {
        let mut v = OutputView::new(10, false);
        v.push_at(OutKind::Info, "", "x".into());
        v.push_at(OutKind::Error, "boom\r\n", "x".into());
        assert_eq!(v.text(), " \n✖ boom\n");
        v.set_limits(1, true);
        assert_eq!(v.line_count(), 1);
    }

    #[test]
    fn header_buttons_clear_and_copy_hit() {
        let mut v = OutputView::new(10, false);
        v.push_at(OutKind::Info, "a", "x".into());
        // 버튼 사각형은 그릴 때 정해진다 → 그리기 없이 눌러도 아무 일 없음(본문으로 간다).
        v.set_bounds(Rect::new(0, 0, 400, 200), 1.0);
        v.head_bottom = 20;
        assert!(v.on_event(&InputEvent::MouseDown {
            x: 10,
            y: 50,
            shift: false,
            primary: true
        }));
        assert!(v.on_event(&InputEvent::MouseUp { x: 10, y: 50 }));
        assert_eq!(v.line_count(), 1);
        // 버튼 사각형을 직접 놓고 누르면 Clear.
        v.buttons = vec![(Btn::Clear, Rect::new(300, 0, 50, 20))];
        v.on_event(&InputEvent::MouseDown {
            x: 310,
            y: 5,
            shift: false,
            primary: true,
        });
        v.on_event(&InputEvent::MouseUp { x: 310, y: 5 });
        assert!(v.is_empty());
    }
}
