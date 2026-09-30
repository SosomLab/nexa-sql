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

pub(crate) struct OutputView {
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
    /// 두 번 클릭한 줄이 `N행:`/`line N:`이면 그 줄 번호(호스트가 거둬 편집기로 이동).
    goto: Option<usize>,
}

impl OutputView {
    pub(crate) fn new(max_lines: usize, timestamps: bool) -> Self {
        let mut body = TextBox::new("").with_multiline();
        body.set_read_only(true);
        body.set_wrap(true);
        body.set_line_numbers(false);
        body.set_gutter_marks(false);
        body.set_minimap(false);
        body.set_focus_ring(false);
        // 우클릭 메뉴는 팝업 층에서(`paint_popup` · 호스트가 맨 마지막에 부른다).
        body.set_popup_deferred(true);
        Self {
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
        }
    }

    /// 두 번 클릭 간격(설정 `ui.dblclick_ms`).
    pub(crate) fn set_dblclick_ms(&mut self, ms: u64) {
        self.dblclick_ms = ms.max(100);
    }

    /// 거두지 않은 새 메시지 수(거두면 0).
    pub(crate) fn take_unread(&mut self) -> usize {
        std::mem::take(&mut self.unread)
    }

    pub(crate) fn unread(&self) -> usize {
        self.unread
    }

    /// 두 번 클릭으로 고른 오류 줄(편집기 줄 번호 · 1부터).
    pub(crate) fn take_goto(&mut self) -> Option<usize> {
        self.goto.take()
    }

    /// 캐럿이 있는 줄의 본문.
    fn caret_line_text(&self) -> String {
        let b = self.body.buf();
        let line = b.line_of(self.body.caret());
        b.line_text(line).into_owned()
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
                        self.goto = parse_error_line(&self.caret_line_text());
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
            if matches!(a, nexa_ctl::EditCtxAction::Copy) {
                let text = self
                    .body
                    .copy_selection()
                    .unwrap_or_else(|| self.body.text());
                let _ = crate::clipboard::write_text(&text);
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
pub(crate) fn parse_error_line(line: &str) -> Option<usize> {
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
    if rest.starts_with("행:") || rest.starts_with(':') {
        digits.parse().ok().filter(|n: &usize| *n > 0)
    } else {
        None
    }
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

    #[test]
    fn error_line_number_is_parsed() {
        assert_eq!(parse_error_line("[12:00:00] ✖ 3행: Warning: x"), Some(3));
        assert_eq!(
            parse_error_line("[12:00:00] ✖ line 12: ORA-00942"),
            Some(12)
        );
        assert_eq!(parse_error_line("✖ 7행: boom"), Some(7));
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
