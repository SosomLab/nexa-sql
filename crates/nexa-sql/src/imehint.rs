//! 가린 입력란(비밀번호 · `ACCEPT … HIDE`)에 글자가 들어올 때 **입력 언어가 라틴이 아니면** 마우스 옆에 몇 초 뜨는 안내
//! (사용자 09-22 "보이지 않는 텍스트 입력 시 IME가 영어가 아니면 마우스 주변에 5초(설정) Floating 메시지"). 상태 판정은
//! [`crate::imestate`] · 그림은 nexa-ctl 공용 툴팁(`draw_tooltip_in` — 창 밖으로 안 나가는 안전망 · docs/61 §2-2) · 시간은
//! `ui.ime_hint_secs`(0 = 끔). 창마다 하나씩 들고(접속 창 · 입력 창) 글자마다 시계를 다시 맞춘다. 새 타이머 없음 — 창의 `tick`이
//! 만료 시각을 돌려준다.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::Rect;
use nexa_ctl::theme::Theme;
use nsql_i18n::{tf, Msg};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct ImeHint {
    text: String,
    at: (i32, i32),
    until: Option<Instant>,
    /// `ui.ime_hint_secs`(0 = 끔).
    secs: u64,
}

impl ImeHint {
    pub(crate) fn new() -> Self {
        ImeHint {
            secs: 5,
            ..Self::default()
        }
    }

    pub(crate) fn set_secs(&mut self, secs: i64) {
        self.secs = secs.clamp(0, 60) as u64;
        if self.secs == 0 {
            self.until = None;
        }
    }

    /// 가린 입력란에 글자가 들어왔다 — 지금 입력 상태를 읽어 라틴이 아니면 `at`(창 좌표 · 마우스) 옆에 띄운다. 띄웠으면(또는
    /// 시계를 다시 맞췄으면) true = 다시 그려야 한다.
    pub(crate) fn note_hidden_input(&mut self, hwnd: Option<isize>, at: (i32, i32)) -> bool {
        if self.secs == 0 {
            return false;
        }
        let Some(st) = crate::imestate::current(hwnd) else {
            return false;
        };
        if st.latin {
            return false;
        }
        self.text = tf(Msg::ImeHintText, &[st.glyph, st.lang]);
        self.at = at;
        self.until = Some(Instant::now() + Duration::from_secs(self.secs));
        true
    }

    /// 시험·자체 캡처용(`NSQL_STARTUP_CMD=conn.ime_hint:x/y`): 상태를 읽지 않고 글자를 정해 띄운다.
    pub(crate) fn show(&mut self, text: &str, at: (i32, i32)) {
        self.text = text.to_string();
        self.at = at;
        self.until = Some(Instant::now() + Duration::from_secs(self.secs.max(1)));
    }

    pub(crate) fn visible(&self, now: Instant) -> bool {
        self.until.is_some_and(|u| now < u)
    }

    /// 틱 — 만료됐으면 지우고 true(다시 그려야 한다) · 아니면 다음 깨울 시각.
    pub(crate) fn tick(&mut self, now: Instant) -> (bool, Option<Instant>) {
        match self.until {
            Some(u) if now >= u => {
                self.until = None;
                (true, None)
            }
            Some(u) => (false, Some(u)),
            None => (false, None),
        }
    }

    /// 마우스 옆(오른쪽 아래 14×18px)에 툴팁 카드 — 창의 **팝업 층**(맨 마지막)에서 부른다. `win` = 창 전체.
    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme, win: Rect, scale: f32) {
        if !self.visible(Instant::now()) {
            return;
        }
        let s = |v: f32| (v * scale).round() as i32;
        let anchor = Rect::new(self.at.0 + s(14.0), self.at.1 + s(12.0), 1, 1);
        nexa_ctl::draw::draw_tooltip_in(dc, th, anchor, (win.x, win.right()), &self.text, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 띄우면 보이고 · 시간이 지나면 틱이 지우며 · 0초 설정은 아무것도 띄우지 않는다.
    #[test]
    fn lifecycle_and_off_switch() {
        let mut h = ImeHint::new();
        h.set_secs(2);
        assert!(!h.visible(Instant::now()));
        h.show("가 KOR", (10, 10));
        let now = Instant::now();
        assert!(h.visible(now));
        let (changed, next) = h.tick(now);
        assert!(!changed && next.is_some());
        let later = now + Duration::from_secs(3);
        let (changed, next) = h.tick(later);
        assert!(changed && next.is_none() && !h.visible(later));
        let mut off = ImeHint::new();
        off.set_secs(0);
        assert!(!off.note_hidden_input(None, (0, 0)));
        assert!(!off.visible(Instant::now()));
    }
}
