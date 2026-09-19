//! **미커밋 경고 카드**(docs/56 L2 · 사용자 09-19): 유휴 미커밋 트랜잭션을 알리는 비모달 카드 — 편집기 우하단(실행 카드 위).
//!
//! 두 모드: **경고**(`[커밋] [롤백] [나중에]`) · **카운트다운**(`n초 뒤 자동 롤백` + `[지금 롤백] [커밋] [연장]`).
//! 카드는 입력을 막지 않는다(타이핑·실행은 그대로) — 그 세션에서 문장을 실행하면 호스트가 카드를 걷는다.
//! 판정(언제 띄우고 언제 자동 처리하는가)은 호스트의 순수 함수 [`crate::sessions::tx_guard_step`]가 하고, 이 파일은 그리기·히트만.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use std::time::Instant;

/// 카드에서 고른 동작.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TxWarnHit {
    None,
    /// 카드 몸통(아무 일 없음 · 클릭 통과 방지).
    Card,
    Commit,
    Rollback,
    /// 나중에 / 연장 — 재알림 간격만큼 미룬다.
    Later,
}

/// 카드 내용(호스트가 만든다).
#[derive(Clone, Debug)]
pub(crate) struct TxWarnView {
    /// 대상 세션.
    pub sess_id: u64,
    /// "미커밋 3문장 · 유휴 12분".
    pub title: String,
    /// 세션 설명 + 첫 대기 문장 요약.
    pub detail: String,
    /// 카운트다운 마감(없으면 경고 모드) + 문구 틀("{1}초 뒤 자동 {0}"의 {0} = 동작 낱말).
    pub countdown: Option<(Instant, String)>,
    /// 버튼(왼쪽부터 · 동작 + 라벨).
    pub buttons: Vec<(TxWarnHit, String)>,
}

#[derive(Default)]
pub(crate) struct TxWarn {
    view: Option<TxWarnView>,
    card: Rect,
    rects: Vec<Rect>,
    hover: Option<usize>,
}

impl TxWarn {
    pub(crate) fn show(&mut self, view: TxWarnView) {
        self.view = Some(view);
        self.hover = None;
    }

    pub(crate) fn hide(&mut self) {
        self.view = None;
        self.rects.clear();
        self.card = Rect::default();
        self.hover = None;
    }

    /// 지금 카드가 가리키는 세션(없으면 None).
    pub(crate) fn sess_id(&self) -> Option<u64> {
        self.view.as_ref().map(|v| v.sess_id)
    }

    /// 카운트다운을 보여 주는 중인가(1초 틱으로 다시 그린다).
    pub(crate) fn counting(&self) -> bool {
        self.view.as_ref().is_some_and(|v| v.countdown.is_some())
    }

    /// hover 갱신 — 바뀌었으면 true.
    pub(crate) fn hover(&mut self, p: Point) -> bool {
        let h = self.rects.iter().position(|r| r.contains(p));
        if h != self.hover {
            self.hover = h;
            return true;
        }
        false
    }

    pub(crate) fn click(&mut self, p: Point) -> TxWarnHit {
        let Some(v) = &self.view else {
            return TxWarnHit::None;
        };
        if let Some(i) = self.rects.iter().position(|r| r.contains(p)) {
            return v.buttons.get(i).map_or(TxWarnHit::Card, |b| b.0);
        }
        if self.card.contains(p) {
            return TxWarnHit::Card;
        }
        TxWarnHit::None
    }

    /// 그리기 — `right`/`bottom` = 놓일 자리의 오른쪽 아래. 돌려주는 값 = 다음 카드가 쌓일 bottom(카드가 없으면 그대로).
    pub(crate) fn paint(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        right: i32,
        bottom: i32,
        s: f32,
    ) -> i32 {
        let Some(v) = &self.view else {
            return bottom;
        };
        let px = |x: f32| (x * s).round() as i32;
        let (w, pad, gap, bar) = (px(440.0), px(10.0), px(6.0), px(4.0));
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let btn_h = lh + px(8.0);
        let lines = if v.countdown.is_some() { 3 } else { 2 };
        let h = pad * 2 + lh * lines + px(6.0) + btn_h;
        let card = Rect::new(right - gap - w, bottom - gap - h, w, h);
        self.card = card;
        let accent = if v.countdown.is_some() {
            th.danger
        } else {
            th.warn
        };
        dc.fill_round_rect(card, px(6.0), th.panel_bg_alt);
        dc.stroke_round_rect(card, px(6.0), th.border, 1.0);
        dc.fill_rect(
            Rect::new(card.x, card.y + px(3.0), bar, card.h - px(6.0)),
            accent,
        );
        let tx = card.x + bar + pad;
        let clip = Rect::new(tx, card.y, card.w - bar - pad * 2, card.h);
        let mut y = card.y + pad;
        dc.select_font(FontSlot::Base, true);
        dc.text(tx, y, clip, &v.title, th.text);
        y += lh;
        dc.select_font(FontSlot::Base, false);
        dc.text(tx, y, clip, &v.detail, th.text_dim);
        y += lh;
        if let Some((deadline, word)) = &v.countdown {
            let left = deadline.saturating_duration_since(Instant::now()).as_secs() + 1;
            let line = nsql_i18n::tf(nsql_i18n::Msg::TxWarnCountdown, &[word, &left.to_string()]);
            dc.text(tx, y, clip, &line, th.danger);
            y += lh;
        }
        y += px(6.0);
        // 버튼 줄(왼쪽부터 · 첫 버튼 = 강조).
        self.rects.clear();
        let mut bx = tx;
        for (i, (_, label)) in v.buttons.iter().enumerate() {
            let bw = dc.text_width(label) + px(20.0);
            let r = Rect::new(bx, y, bw, btn_h);
            let hot = self.hover == Some(i);
            let ty = dc.text_center_y(r.y, r.h);
            if i == 0 {
                dc.fill_round_rect(r, px(5.0), accent);
                if hot {
                    dc.fill_round_rect_alpha(r, px(5.0), th.text, 0.15);
                }
                dc.text(r.x + px(10.0), ty, r, label, th.panel_bg);
            } else {
                dc.fill_round_rect(r, px(5.0), th.field_bg);
                dc.stroke_round_rect(r, px(5.0), if hot { th.accent } else { th.border }, 1.0);
                dc.text(r.x + px(10.0), ty, r, label, th.text);
            }
            self.rects.push(r);
            bx += bw + px(8.0);
        }
        card.y
    }
}
