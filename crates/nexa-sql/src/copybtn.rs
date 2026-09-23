//! **복사 버튼 부품**(사용자 09-23 "복사 기능을 가진 버튼은 전부") — 누르면 ① 눌리는 효과(120 ms · 안으로 들어감) → ② 체크 표시로
//! 바뀌고 ③ 녹색 계열(`theme.ok`)로 "됐다"를 알린 뒤 ④ `ui.copy_feedback_ms`(기본 2초) 지나면 복사 아이콘으로 돌아온다.
//! 실행 카드([`crate::runtoast`]) · 접속 창 파일 이름([`crate::connect`])이 같은 부품을 쓴다(30 §2 부품 규칙 — 두 번째 만난 문제).
//!
//! 상태 = `hover` · `pressed_at`(누른 시각 · 없으면 평상시). 그리기는 `now`를 받아 순수하게 판정하고, 애니메이션 중에는
//! [`CopyBtn::next_tick`]이 다음 틱 시각을 돌려준다(호스트의 tick 루프가 그 시각에 다시 그린다 · 자체 타이머·스레드 없음).

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{IconImage, Theme};
use std::time::{Duration, Instant};

/// 눌리는 효과 길이.
pub(crate) const PRESS_MS: u64 = 120;
/// 기본 복귀 시간(설정 `ui.copy_feedback_ms`).
pub(crate) const DEFAULT_FEEDBACK_MS: u64 = 2000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Look {
    Idle,
    /// 눌림(안으로 들어감).
    Pressed,
    /// 체크 · 녹색(0 = 방금 · 1 = 복귀 직전).
    Done(f32),
}

#[derive(Clone, Debug)]
pub(crate) struct CopyBtn {
    pub rect: Rect,
    hover: bool,
    pressed_at: Option<Instant>,
    feedback: Duration,
}

impl Default for CopyBtn {
    fn default() -> Self {
        Self::new()
    }
}

impl CopyBtn {
    pub(crate) fn new() -> Self {
        CopyBtn {
            rect: Rect::default(),
            hover: false,
            pressed_at: None,
            feedback: Duration::from_millis(DEFAULT_FEEDBACK_MS),
        }
    }

    /// 설정 `ui.copy_feedback_ms`(300~10000).
    pub(crate) fn set_feedback_ms(&mut self, ms: i64) {
        self.feedback = Duration::from_millis(ms.clamp(300, 10_000) as u64);
    }

    pub(crate) fn hit(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    /// hover 갱신 — 바뀌었으면 true(다시 그린다).
    pub(crate) fn set_hover(&mut self, on: bool) -> bool {
        let changed = on != self.hover;
        self.hover = on;
        changed
    }

    /// 눌렀다(복사가 실제로 됐을 때 호스트가 부른다) — 눌림 → 체크 → 복귀의 시계를 시작한다.
    pub(crate) fn press(&mut self, now: Instant) {
        self.pressed_at = Some(now);
    }

    pub(crate) fn look(&self, now: Instant) -> Look {
        let Some(t0) = self.pressed_at else {
            return Look::Idle;
        };
        let el = now.saturating_duration_since(t0);
        if el < Duration::from_millis(PRESS_MS) {
            Look::Pressed
        } else if el < self.feedback {
            Look::Done(el.as_secs_f32() / self.feedback.as_secs_f32())
        } else {
            Look::Idle
        }
    }

    /// 애니메이션 중이면 다음에 다시 그릴 시각(눌림 = 매 프레임 · 체크 = 끝나는 시각) · 평상시 None.
    pub(crate) fn next_tick(&self, now: Instant) -> Option<Instant> {
        let t0 = self.pressed_at?;
        match self.look(now) {
            Look::Idle => None,
            Look::Pressed => Some(now + Duration::from_millis(16)),
            Look::Done(_) => Some(t0 + self.feedback),
        }
    }

    /// 그리기 — `alpha` = 담는 쪽의 투명도(카드 페이드) · `scale` = 배율. 상자(옅은 배경 + 테두리) + 아이콘.
    pub(crate) fn paint(
        &self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        alpha: f32,
        scale: f32,
        now: Instant,
    ) {
        let r = self.rect;
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let px = |v: f32| (v * scale).round() as i32;
        let look = self.look(now);
        let (bg_color, bg_alpha, border, icon_color, icon_px, inset) = match look {
            Look::Idle => (
                th.text,
                if self.hover { 0.16 } else { 0.06 },
                th.border,
                if self.hover { th.text } else { th.text_dim },
                16.0,
                0,
            ),
            // 눌림 = 배경 진하게 · 아이콘 작게 · 1px 아래로(안으로 들어가는 느낌).
            Look::Pressed => (th.text, 0.30, th.text_dim, th.text, 13.0, px(1.0)),
            // 체크 = 녹색 배경(처음 진하다가 옅어짐) + 녹색 테두리 + 녹색 체크.
            Look::Done(p) => (th.ok, 0.32 * (1.0 - 0.6 * p), th.ok, th.ok, 16.0, 0),
        };
        dc.fill_round_rect_alpha(r, px(3.0), bg_color, bg_alpha * alpha);
        dc.stroke_round_rect_alpha(r, px(3.0), border, 1.0, alpha);
        let (cr, cg, cb) = icon_color.rgb();
        let ic = match look {
            Look::Done(_) => crate::toolicons::mi_check(),
            _ => crate::toolicons::mi_copy(),
        };
        let img = IconImage::from_alpha_tinted(ic.w, ic.h, &ic.alpha, (cr, cg, cb));
        let sz = px(icon_px).min((r.w - px(2.0)).max(1));
        dc.image_scaled(
            Rect::new(r.x + (r.w - sz) / 2, r.y + (r.h - sz) / 2 + inset, sz, sz),
            &img,
            r,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 누름 → 120 ms 눌림 → 체크(진척 0→1) → 복귀 시간 뒤 평상시 · 다음 틱 시각.
    #[test]
    fn press_then_check_then_back() {
        let mut b = CopyBtn::new();
        b.set_feedback_ms(2000);
        let t0 = Instant::now();
        assert_eq!(b.look(t0), Look::Idle);
        assert_eq!(b.next_tick(t0), None);
        b.press(t0);
        assert_eq!(b.look(t0), Look::Pressed);
        assert_eq!(b.look(t0 + Duration::from_millis(119)), Look::Pressed);
        assert!(matches!(b.look(t0 + Duration::from_millis(120)), Look::Done(p) if p < 0.1));
        assert!(matches!(b.look(t0 + Duration::from_millis(1900)), Look::Done(p) if p > 0.9));
        assert_eq!(
            b.next_tick(t0 + Duration::from_millis(500)),
            Some(t0 + Duration::from_millis(2000))
        );
        assert_eq!(b.look(t0 + Duration::from_millis(2000)), Look::Idle);
        assert_eq!(b.next_tick(t0 + Duration::from_millis(2000)), None);
        // 설정 하한·상한.
        b.set_feedback_ms(10);
        assert_eq!(b.feedback, Duration::from_millis(300));
        b.set_feedback_ms(99_999);
        assert_eq!(b.feedback, Duration::from_millis(10_000));
    }

    #[test]
    fn hover_reports_change_only() {
        let mut b = CopyBtn::new();
        b.rect = Rect::new(10, 10, 20, 20);
        assert!(b.hit(Point { x: 15, y: 15 }) && !b.hit(Point { x: 5, y: 5 }));
        assert!(b.set_hover(true));
        assert!(!b.set_hover(true));
        assert!(b.set_hover(false));
    }
}
