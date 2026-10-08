//! 토스트(우측 하단 · 쌓임 · 설정 시간 뒤 사라짐 · 반투명 — 사용자 09-16 "객체가 없어서 오류가 나면 유형과 대상 이름을 토스트로").
//!
//! 카드는 `ui.toast_alpha`(기본 85%)로 그려 아래 내용이 비친다 · 수명은 `ui.toast_ms`(기본 3000 ms) · 마지막 300ms는 페이드 아웃 ·
//! 클릭하면 바로 사라진다 · 최대 5장(오래된 것부터 밀려남). 그리기는 창의 맨 마지막 층(팝업 규칙).
//!
//! ★ 남은 시간 표시(사용자 09-22 "진척에 따라 더 투명해지며 사라지는 느낌" → "이미 있는 왼쪽 세로선을 그대로 활용 · 위에서 아래로" →
//! "지나간 부분은 70% 투명한 같은 색" · `ui.toast_progress`): **왼쪽 색 막대**의 남은 시간만큼이 진하고([`bar_remaining`] · 아래 고정) 지나간
//! 위쪽은 같은 색을 `ui.toast_bar_spent`(기본 30%)로 · 카드 불투명도는 [`life_alpha`] 곡선(제곱 이징 — 앞부분은 또렷하고 끝으로 갈수록 빨리
//! `ui.toast_fade_to`까지) → 마지막 300ms에 0. 새 타이머 없음 — 카드가 있는 동안 도는 30ms 틱이 그대로 그린다. 실행 상태 카드
//! ([`crate::runtoast`])도 같은 두 함수를 쓴다.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nsql_i18n::{t, Msg};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Error,
    Warn,
    Info,
}

struct Toast {
    kind: ToastKind,
    title: String,
    body: String,
    born: Instant,
    rect: Rect,
    /// 클릭하면 실행할 명령 id(예 북마크 제거 되돌리기 · docs/69 C-28).
    action: Option<String>,
    /// ★ 카드 안 **위험색 버튼**(라벨 · 명령 id · 사용자 10-08 "운영 해제 토스트에 빨간 실행 버튼 — 마우스로는 3초 안에 다시 고르기 어렵다"):
    ///   누르면 그 명령 1회 · 카드 닫힘. 모양 = `TimeoutButton` 무장 상태(위험색 채움 · 밝은 글자).
    button: Option<(String, String)>,
    btn_rect: Rect,
}

const MAX_TOASTS: usize = 5;
const FADE_MS: u64 = 300;

/// 수명 진척 `p`(0 = 방금 · 1 = 만료)에 따른 불투명도 배율 — 1.0에서 `fade_to`(0..1)까지 **제곱 이징**(초반은 거의 그대로, 뒤로 갈수록
/// 빨리 투명해진다 = 읽을 시간은 지키고 "사라지는 느낌"은 뒤에 몰아 준다). `progress`가 꺼져 있으면 늘 1.0(종전 = 마지막 300ms만).
/// 마지막 300ms의 0으로 가는 페이드는 이 값에 곱한다(`paint`).
pub(crate) fn life_alpha(p: f32, fade_to: f32, progress: bool) -> f32 {
    if !progress {
        return 1.0;
    }
    let p = p.clamp(0.0, 1.0);
    let fade_to = fade_to.clamp(0.0, 1.0);
    1.0 - (1.0 - fade_to) * p * p
}

/// 왼쪽 막대의 남은(진한) 높이 — 진척 `life`(0..1)이 있으면 `full × (1 − life)`(반올림 · 끝에서는 0) · 없으면(진행 중 · 수동 닫기 · 꺼짐) 가득.
/// 호출자는 이 높이를 **아래에 붙여** 그리고 나머지는 옅은 같은 색으로(위에서부터 옅어지는 느낌).
pub(crate) fn bar_remaining(full: i32, life: Option<f32>) -> i32 {
    match life {
        Some(p) => ((1.0 - p.clamp(0.0, 1.0)) * full as f32).round() as i32,
        None => full,
    }
    .clamp(0, full)
}

pub(crate) struct Toasts {
    items: Vec<Toast>,
    ttl: Duration,
    /// 카드 불투명도(0.3~1.0).
    alpha: f32,
    /// 남은 시간 막대 + 진척 페이드(`ui.toast_progress`).
    progress: bool,
    /// 수명 끝의 불투명도 비율(`ui.toast_fade_to` % → 0..1).
    fade_to: f32,
    /// 지나간 부분의 막대 불투명도 비율(`ui.toast_bar_spent` % → 0..1).
    spent: f32,
    /// 클릭된 토스트의 동작 id(1회성 · `take_action`).
    taken: Option<String>,
}

impl Toasts {
    pub(crate) fn new() -> Self {
        Toasts {
            items: Vec::new(),
            ttl: Duration::from_secs(3),
            alpha: 0.85,
            progress: true,
            fade_to: 0.35,
            spent: 0.3,
            taken: None,
        }
    }

    /// 설정 `ui.toast_ms`(09-30 ms 단위) · `ui.toast_alpha`(%).
    pub(crate) fn configure(&mut self, ms: i64, alpha_pct: i64) {
        self.ttl = Duration::from_millis(ms.clamp(100, 60_000) as u64);
        self.alpha = (alpha_pct.clamp(30, 100) as f32) / 100.0;
    }

    /// 설정 `ui.toast_progress` · `ui.toast_fade_to`(%) · `ui.toast_bar_spent`(%).
    pub(crate) fn configure_progress(&mut self, on: bool, fade_to_pct: i64, spent_pct: i64) {
        self.progress = on;
        self.fade_to = (fade_to_pct.clamp(0, 100) as f32) / 100.0;
        self.spent = (spent_pct.clamp(0, 100) as f32) / 100.0;
    }

    pub(crate) fn push(
        &mut self,
        kind: ToastKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) {
        self.items.push(Toast {
            kind,
            title: title.into(),
            body: body.into(),
            born: Instant::now(),
            rect: Rect::default(),
            action: None,
            button: None,
            btn_rect: Rect::default(),
        });
        while self.items.len() > MAX_TOASTS {
            self.items.remove(0);
        }
    }

    /// 클릭하면 `action`(명령 id)을 실행하는 토스트 — 호스트가 `take_action`으로 거둔다.
    pub(crate) fn push_action(
        &mut self,
        kind: ToastKind,
        title: impl Into<String>,
        body: impl Into<String>,
        action: &str,
    ) {
        self.push(kind, title, body);
        if let Some(t) = self.items.last_mut() {
            t.action = Some(action.to_string());
        }
    }

    /// 카드 안 위험색 버튼이 있는 토스트(버튼 = `action` 1회 · 카드의 나머지 클릭 = 닫기만).
    pub(crate) fn push_button(
        &mut self,
        kind: ToastKind,
        title: impl Into<String>,
        body: impl Into<String>,
        label: impl Into<String>,
        action: &str,
    ) {
        self.push(kind, title, body);
        if let Some(t) = self.items.last_mut() {
            t.button = Some((label.into(), action.to_string()));
        }
    }

    pub(crate) fn take_action(&mut self) -> Option<String> {
        self.taken.take()
    }

    /// 만료 제거 — 남은 카드가 있으면(페이드 애니메이션) true = 30ms 틱 유지.
    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        let ttl = self.ttl;
        self.items.retain(|t| now.duration_since(t.born) < ttl);
        !self.items.is_empty()
    }

    pub(crate) fn animating(&self) -> bool {
        !self.items.is_empty()
    }

    /// 점 아래 카드(index) — 누름/놓음 짝 맞추기용(동작은 [`Self::pick`]).
    /// 누르는 동안 수명을 다시 준다(협업 V1 bin103 ⑥ 관찰 = 누르고 있는 사이 만료되면 놓아도 무동작) — 놓을 때 동작하는 규칙의 짝.
    pub(crate) fn hold(&mut self, i: usize) {
        if let Some(t) = self.items.get_mut(i) {
            t.born = Instant::now();
        }
    }

    pub(crate) fn hit(&self, p: Point) -> Option<usize> {
        self.items.iter().position(|t| t.rect.contains(p))
    }

    /// 카드 `i`를 `p`에서 놓았다 — 닫고, 버튼 위면 버튼 명령 · 버튼 없는 카드의 본문이면 카드 명령(`take_action`).
    pub(crate) fn pick(&mut self, i: usize, p: Point) {
        if i >= self.items.len() {
            return;
        }
        let t = self.items.remove(i);
        if let Some((_, a)) = t.button.as_ref().filter(|_| t.btn_rect.contains(p)) {
            self.taken = Some(a.clone());
        } else if t.action.is_some() && t.button.is_none() {
            self.taken = t.action;
        }
    }

    /// 우측 하단부터 위로 쌓아 그린다(`bottom` = 상태줄 위 y · `right` = 창 오른쪽).
    pub(crate) fn paint(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        right: i32,
        bottom: i32,
        s: f32,
    ) {
        if self.items.is_empty() {
            return;
        }
        let px = |v: f32| (v * s).round() as i32;
        let (w, pad, gap, bar) = (px(340.0), px(10.0), px(6.0), px(4.0));

        let lh = dc.text_height();
        let now = Instant::now();
        let ttl_ms = self.ttl.as_millis().max(1) as f32;
        let mut y = bottom - gap;
        for t in self.items.iter_mut().rev() {
            let age = now.duration_since(t.born);
            let left = self.ttl.saturating_sub(age).as_millis() as u64;
            let fade = if left < FADE_MS {
                left as f32 / FADE_MS as f32
            } else {
                1.0
            };
            let life = (age.as_millis() as f32 / ttl_ms).clamp(0.0, 1.0);
            let a = self.alpha * life_alpha(life, self.fade_to, self.progress) * fade;
            // ★ 본문 단어 단위 줄바꿈(사용자 10-07 "토스트 내용이 길면 word wrap") — 폭 안에서 · 긴 토큰은 글자 단위 · 최대 6줄(마지막 …) ·
            //   높이 = 제목 1줄 + 본문 줄 수.
            let text_x0 = px(6.0) + bar + pad;
            let avail = w - text_x0 - pad;
            let body_lines = wrap_words(
                &t.body,
                avail,
                &mut |s: &str| dc.text_width(s),
                MAX_BODY_LINES,
            );
            let n = body_lines.len().max(1) as i32;
            // 버튼 줄(있을 때만): 글자 높이 + 위아래 3px · 본문과 4px 틈.
            let btn_h = if t.button.is_some() { lh + px(6.0) } else { 0 };
            let h = pad * 2 + lh * (1 + n) + px(2.0) + if btn_h > 0 { btn_h + px(4.0) } else { 0 };
            let r = Rect::new(right - gap - w, y - h, w, h);
            t.rect = r;
            dc.fill_round_rect_alpha(r, px(6.0), th.panel_bg, a);
            dc.stroke_round_rect_alpha(r, px(6.0), th.border, 1.0, a);
            let color = match t.kind {
                ToastKind::Error => th.danger,
                ToastKind::Warn => th.warn,
                ToastKind::Info => th.accent,
            };
            // 왼쪽 색 막대 = 남은 시간(아래 고정 · 위에서부터 옅어짐 · 지나간 부분 = 같은 색 `ui.toast_bar_spent`) · 꺼져 있으면 가득.
            let full = h - pad * 2;
            let bar_h = bar_remaining(full, self.progress.then_some(life));
            let bar_x = r.x + px(6.0);
            if bar_h < full {
                dc.fill_round_rect_alpha(
                    Rect::new(bar_x, r.y + pad, bar, full),
                    px(2.0),
                    color,
                    a * self.spent,
                );
            }
            if bar_h > 0 {
                dc.fill_round_rect_alpha(
                    Rect::new(bar_x, r.y + pad + (full - bar_h), bar, bar_h),
                    px(2.0),
                    color,
                    a,
                );
            }
            let tx = r.x + text_x0;
            let clip = Rect::new(r.x, r.y, r.w - pad, r.h);
            dc.text(tx, r.y + pad, clip, &t.title, color);
            let mut by = r.y + pad + lh + px(2.0);
            for line in &body_lines {
                dc.text(tx, by, clip, line, th.text);
                by += lh;
            }
            // ★ 위험색 버튼 = 오른쪽 아래(제목 색이 아니라 늘 위험색 — "되돌릴 수 없는 실행" 신호 · `TimeoutButton` 무장 모양).
            if let Some((label, _)) = t.button.as_ref() {
                let bw = dc.text_width(label) + px(16.0);
                let br = Rect::new(r.right() - pad - bw, r.bottom() - pad - btn_h, bw, btn_h);
                t.btn_rect = br;
                dc.fill_round_rect_alpha(br, px(4.0), th.danger, a);
                let cy = dc.text_center_y(br.y, br.h);
                dc.text(br.x + px(8.0), cy, br, label, th.panel_bg);
            } else {
                t.btn_rect = Rect::default();
            }
            y = r.y - gap;
        }
    }
}

/// 토스트 본문 최대 줄 수(넘치면 마지막 줄 끝 `…`).
const MAX_BODY_LINES: usize = 6;

/// ★ 단어 단위 줄바꿈(순수 · 사용자 10-07): 공백으로 나눈 낱말을 폭 안에서 이어 붙이고, 한 낱말이 폭보다 길면 글자 단위로 끊는다 ·
/// `max_lines`를 넘으면 마지막 줄 끝에 `…`. 원문의 `\n`은 강제 줄바꿈. 빈 글 = 빈 줄 하나.
pub(crate) fn wrap_words(
    text: &str,
    max_w: i32,
    measure: &mut dyn FnMut(&str) -> i32,
    max_lines: usize,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut fits = |s: &str| measure(s) <= max_w.max(1);
    'outer: for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            if word.is_empty() {
                continue;
            }
            let cand = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if fits(&cand) {
                line = cand;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
                if out.len() >= max_lines {
                    break 'outer;
                }
            }
            // 낱말 하나가 폭보다 길다 → 글자 단위.
            let mut piece = String::new();
            for ch in word.chars() {
                let mut next = piece.clone();
                next.push(ch);
                if fits(&next) || piece.is_empty() {
                    piece = next;
                } else {
                    out.push(std::mem::take(&mut piece));
                    if out.len() >= max_lines {
                        break 'outer;
                    }
                    piece.push(ch);
                }
            }
            line = piece;
        }
        out.push(line);
        if out.len() >= max_lines {
            break;
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    // 넘쳤으면 마지막 줄에 말줄임(전체 글이 다 들어갔는지 = 줄 합 길이로 판정).
    let shown: usize = out.iter().map(|l| l.chars().count()).sum();
    let total: usize = text.chars().filter(|c| *c != ' ' && *c != '\n').count();
    let shown_ns: usize = out
        .iter()
        .map(|l| l.chars().filter(|c| *c != ' ').count())
        .sum();
    if out.len() >= max_lines && (shown_ns < total || shown == 0) {
        if let Some(last) = out.last_mut() {
            while measure(&format!("{last}…")) > max_w.max(1) && last.pop().is_some() {}
            last.push('…');
        }
    }
    out
}

#[cfg(test)]
mod wrap_tests {
    use super::wrap_words;

    /// 글자 수 = 폭(측정 = 글자 수)으로 규칙만 본다.
    #[test]
    fn wraps_words_breaks_long_tokens_and_ellipsizes() {
        let mut m = |s: &str| s.chars().count() as i32;
        assert_eq!(wrap_words("", 10, &mut m, 6), vec![String::new()]);
        assert_eq!(
            wrap_words("ab cd", 10, &mut m, 6),
            vec!["ab cd".to_string()]
        );
        assert_eq!(
            wrap_words("ab cd ef", 5, &mut m, 6),
            vec!["ab cd".to_string(), "ef".to_string()]
        );
        // 긴 낱말 = 글자 단위.
        assert_eq!(
            wrap_words("abcdefgh", 3, &mut m, 6),
            vec!["abc".to_string(), "def".to_string(), "gh".to_string()]
        );
        // 강제 줄바꿈.
        assert_eq!(
            wrap_words("a\nb", 10, &mut m, 6),
            vec!["a".to_string(), "b".to_string()]
        );
        // 상한 + 말줄임.
        let w = wrap_words("aa bb cc dd ee", 2, &mut m, 2);
        assert_eq!(w.len(), 2);
        assert!(w[1].ends_with('…'), "{w:?}");
    }
}

/// 공통 분류의 표시 문구(i18n).
pub(crate) fn class_label(class: nsql_core::ErrorClass) -> Option<&'static str> {
    use nsql_core::ErrorClass as C;
    Some(t(match class {
        C::NoTable => Msg::ErrClsNoTable,
        C::NoColumn => Msg::ErrClsNoColumn,
        C::NoObject => Msg::ErrClsNoObject,
        C::Syntax => Msg::ErrClsSyntax,
        C::Permission => Msg::ErrClsPermission,
        C::Login => Msg::ErrClsLogin,
        C::Connection => Msg::ErrClsConnection,
        C::Unique => Msg::ErrClsUnique,
        C::ForeignKey => Msg::ErrClsForeignKey,
        C::NotNull => Msg::ErrClsNotNull,
        C::Check => Msg::ErrClsCheck,
        C::Lock => Msg::ErrClsLock,
        C::Deadlock => Msg::ErrClsDeadlock,
        C::DataType => Msg::ErrClsDataType,
        C::Resource => Msg::ErrClsResource,
        C::Unknown => return None,
    }))
}

/// 오류 한 줄 요약 — `[코드 · 분류: 대상] 원문`(코드는 부각 · 사용자 09-16). 분류 안 되면 원문 그대로.
pub(crate) fn summarize(
    dialect: nsql_core::Dialect,
    code: Option<i64>,
    message: &str,
    stmt: &str,
) -> (nsql_core::Classified, String) {
    let c = nsql_core::classify(dialect, code, message, stmt);
    let mut head = String::new();
    if let Some(code) = &c.code {
        head.push_str(code);
    }
    if let Some(label) = class_label(c.class) {
        if !head.is_empty() {
            head.push_str(" · ");
        }
        head.push_str(label);
        if let Some(o) = &c.object {
            head.push_str(": ");
            head.push_str(o);
        }
    }
    let line = if head.is_empty() {
        message.to_string()
    } else {
        format!("[{head}] {message}")
    };
    (c, line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::Dialect;

    #[test]
    fn summary_highlights_code_and_class() {
        let (c, line) = summarize(
            Dialect::Oracle,
            Some(942),
            "ORA-00942: 테이블 또는 뷰가 존재하지 않습니다",
            "SELECT * FROM HR.EMP",
        );
        assert!(c.class.is_missing_object());
        assert!(line.starts_with("[ORA-00942 · "), "{line}");
        assert!(line.contains(": HR.EMP] ORA-00942"), "{line}");
        let (_, plain) = summarize(Dialect::Oracle, None, "something odd", "");
        assert_eq!(plain, "something odd");
    }

    /// 수명 곡선: 시작 1.0 · 끝 `fade_to` · 단조 감소 · 초반은 완만(제곱 이징 — 절반 지점에서 4분의 1만 내려옴) · 꺼져 있으면 늘 1.0.
    #[test]
    fn life_alpha_curve() {
        assert_eq!(life_alpha(0.0, 0.35, true), 1.0);
        assert!((life_alpha(1.0, 0.35, true) - 0.35).abs() < 1e-6);
        let half = life_alpha(0.5, 0.35, true);
        assert!((half - (1.0 - 0.65 * 0.25)).abs() < 1e-6, "{half}");
        let mut prev = 1.0;
        for i in 1..=20 {
            let v = life_alpha(i as f32 / 20.0, 0.35, true);
            assert!(v <= prev, "{i}: {v} > {prev}");
            prev = v;
        }
        // 범위 밖 입력은 잘라 준다 · 꺼짐 = 종전 동작.
        assert_eq!(life_alpha(2.0, 0.35, true), life_alpha(1.0, 0.35, true));
        assert_eq!(life_alpha(0.9, 0.35, false), 1.0);
        assert_eq!(life_alpha(1.0, 0.0, true), 0.0);
    }

    /// 막대의 진한 부분 높이: 진척 없음 = 가득 · 0 = 가득 · 0.5 = 절반 · 1 = 0 · 범위 밖은 잘라 준다.
    #[test]
    fn bar_remaining_shrinks_with_progress() {
        assert_eq!(bar_remaining(40, None), 40);
        assert_eq!(bar_remaining(40, Some(0.0)), 40);
        assert_eq!(bar_remaining(40, Some(0.5)), 20);
        assert_eq!(bar_remaining(40, Some(1.0)), 0);
        assert_eq!(bar_remaining(40, Some(3.0)), 0);
        assert_eq!(bar_remaining(40, Some(-1.0)), 40);
    }

    #[test]
    fn toasts_expire_and_click_dismisses() {
        let mut ts = Toasts::new();
        ts.configure(1000, 85);
        ts.push(ToastKind::Error, "a", "b");
        assert!(ts.animating());
        assert!(ts.tick(Instant::now()));
        assert!(!ts.tick(Instant::now() + Duration::from_secs(2)));
        ts.push(ToastKind::Error, "a", "b");
        ts.items[0].rect = Rect::new(0, 0, 10, 10);
        // 놓을 때 동작(10-09): 점 아래 카드 → pick.
        let i = ts.hit(Point { x: 5, y: 5 }).expect("카드 위");
        ts.pick(i, Point { x: 5, y: 5 });
        assert!(!ts.animating());
        assert!(ts.hit(Point { x: 5, y: 5 }).is_none());
    }
}
