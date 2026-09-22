//! 가린 입력란(비밀번호 · `ACCEPT … HIDE`)의 **입력 언어 안내**(사용자 09-22 두 번: ① "IME가 영어가 아니면 Floating 메시지"
//! ② "입력란 바로 밑에 · 한글이면 떠 있고 영어로 바꾸면 바로 갱신"). 상태 판정 = [`crate::imestate`] · 그림 = nexa-ctl 공용
//! 툴팁(`draw_tooltip_in` — 창 밖으로 안 나가는 안전망 · docs/61 §2-2) · 켬/끔 = `ui.ime_hint`.
//!
//! 동작: 가린 칸에 포커스가 있는 동안 창의 `tick`이 **150 ms마다** 상태를 읽어(Win32 조회 둘 · 39 §3 등재) 라틴이 아니면 그 칸
//! **아래**에 카드를 두고, 라틴이면 **바로 지운다**. 글자·조합 사건 뒤에는 스로틀 없이 즉시 읽는다(`force`). 포커스가 떠나면 다음
//! 틱에 사라진다. 새 타이머 없음 — 창의 `tick`이 다음 깨울 시각을 돌려준다.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::Rect;
use nexa_ctl::theme::Theme;
use nsql_i18n::{tf, Msg};
use std::time::{Duration, Instant};

/// 상태 조회 주기(가린 칸에 포커스가 있을 때만).
const POLL: Duration = Duration::from_millis(150);

#[derive(Default)]
pub(crate) struct ImeHint {
    text: String,
    /// 붙을 칸(창 좌표) — 카드는 그 아래.
    anchor: Option<Rect>,
    shown: bool,
    enabled: bool,
    last_poll: Option<Instant>,
    /// 캡처용 `show`의 만료(상태와 무관하게 잠깐 보인다).
    pin_until: Option<Instant>,
}

impl ImeHint {
    pub(crate) fn new() -> Self {
        ImeHint {
            enabled: true,
            ..Self::default()
        }
    }

    pub(crate) fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.shown = false;
            self.pin_until = None;
        }
    }

    /// 지금 상태를 읽어 반영한다 — `bx` = 포커스인 가린 칸(없으면 숨김) · `force` = 스로틀 무시(글자 사건 직후).
    /// 돌려주는 값 = 보이는 것이 바뀌었다(다시 그려야 한다).
    pub(crate) fn poll(
        &mut self,
        hwnd: Option<isize>,
        bx: Option<Rect>,
        now: Instant,
        force: bool,
    ) -> bool {
        if self.pin_until.is_some_and(|u| now < u) {
            return false;
        }
        self.pin_until = None;
        let Some(bx) = bx.filter(|_| self.enabled) else {
            return self.hide();
        };
        if !force && self.last_poll.is_some_and(|t| now.duration_since(t) < POLL) {
            return false;
        }
        self.last_poll = Some(now);
        let Some(st) = crate::imestate::current(hwnd) else {
            return self.hide();
        };
        if st.latin {
            return self.hide();
        }
        let text = tf(Msg::ImeHintText, &[st.glyph, st.lang]);
        let changed = !self.shown || self.text != text || self.anchor != Some(bx);
        self.text = text;
        self.anchor = Some(bx);
        self.shown = true;
        changed
    }

    fn hide(&mut self) -> bool {
        let was = self.shown;
        self.shown = false;
        was
    }

    /// 시험·자체 캡처용(`NSQL_STARTUP_CMD=conn.ime_hint:x/y`): 상태를 읽지 않고 글자를 정해 그 자리 아래에 3초 띄운다.
    pub(crate) fn show(&mut self, text: &str, at: (i32, i32)) {
        self.text = text.to_string();
        self.anchor = Some(Rect::new(at.0, at.1, 1, 0));
        self.shown = true;
        self.pin_until = Some(Instant::now() + Duration::from_secs(3));
    }

    #[cfg(test)]
    pub(crate) fn visible(&self) -> bool {
        self.shown
    }

    /// 틱 — 가린 칸이 포커스면 주기 조회 · 아니면 숨김. 돌려주는 값 = (바뀜, 다음 깨울 시각).
    pub(crate) fn tick(
        &mut self,
        hwnd: Option<isize>,
        bx: Option<Rect>,
        now: Instant,
    ) -> (bool, Option<Instant>) {
        if let Some(u) = self.pin_until {
            if now < u {
                return (false, Some(u));
            }
            self.pin_until = None;
            let was = self.hide();
            return (was, None);
        }
        let changed = self.poll(hwnd, bx, now, false);
        let next = bx
            .filter(|_| self.enabled)
            .map(|_| self.last_poll.unwrap_or(now) + POLL);
        (changed, next)
    }

    /// 칸 **아래**(왼쪽 맞춤 · 4px 간격)에 툴팁 카드 — 창의 **팝업 층**(맨 마지막)에서 부른다. `win` = 창 전체.
    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme, win: Rect, scale: f32) {
        let Some(a) = self.anchor.filter(|_| self.shown) else {
            return;
        };
        let s = |v: f32| (v * scale).round() as i32;
        let anchor = Rect::new(a.x, a.bottom() + s(4.0), 1, 1);
        nexa_ctl::draw::draw_tooltip_in(dc, th, anchor, (win.x, win.right()), &self.text, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 캡처용 표시는 3초 뒤 틱이 지우고 · 가린 칸이 없으면 숨기며 · 끄면 아무것도 보이지 않는다 · 칸이 있으면 다음 틱을 예약한다.
    #[test]
    fn lifecycle_and_off_switch() {
        let mut h = ImeHint::new();
        assert!(!h.visible());
        h.show("가 KOR", (10, 10));
        assert!(h.visible());
        let now = Instant::now();
        let (changed, next) = h.tick(None, None, now);
        assert!(!changed && next.is_some(), "고정 표시 중");
        let later = now + Duration::from_secs(4);
        let (changed, _) = h.tick(None, None, later);
        assert!(changed && !h.visible(), "만료 = 지움");
        // 가린 칸이 없으면 조회하지 않고 숨긴 채 · 다음 틱 없음.
        let (changed, next) = h.tick(None, None, later);
        assert!(!changed && next.is_none());
        // 끔 = 칸이 있어도 아무것도 안 띄우고 예약도 없다.
        let mut off = ImeHint::new();
        off.set_enabled(false);
        let bx = Some(Rect::new(0, 0, 100, 20));
        assert!(!off.poll(None, bx, now, true));
        let (_, next) = off.tick(None, bx, now);
        assert!(!off.visible() && next.is_none());
    }
}
