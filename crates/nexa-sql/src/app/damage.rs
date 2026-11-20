//! App — 더티 영역 present(T-224 · D-375 · 설정 `gfx.partial_redraw` · 기본 끔 = 사용자 11-21).
//!
//! 종전 = 캐럿이 깜빡일 때마다(초당 2회) 창 전체를 다시 그리고 전체를 냈다. 켜면 **자기 사각형을 아는 요청**([`App::redraw_rect`] ·
//! 지금은 캐럿 깜빡임 = 편집기 영역)만 있는 프레임은 그 사각형으로 **클립해 그리고**(`RasterCtx::push_clip` · 세그먼트는 전부 돌되
//! 픽셀은 사각형 안만) 그 사각형만 낸다(softbuffer `present_with_damage` · IOSurface `present_damage`).
//!
//! 불변식(nexa-dir3 `app/paint.rs`에서 옮김 — 같은 판정 · 같은 시험):
//! - **모르면 전체**: 전체 요청([`App::redraw`]) · 사각형 없는 프레임(OS 노출 · 직접 `request_redraw`) · 버퍼 나이 0(새 표면 · 크기 변경) ·
//!   캡처 요청 = 전체. 부분은 "이 프레임의 이유가 사각형 요청뿐"일 때만.
//! - 버퍼 나이 n(다중 버퍼)이면 이번 사각형 ∪ 지난 n−1 프레임의 사각형 · 이력이 모자라면 전체.
//! - 이력에는 **그 프레임에서 바뀐 것**(자기 사각형)만 넣는다 — 그린 것(이력과의 합집합)을 넣으면 전체 프레임 하나가 영원히 번진다.
//! - 오버레이(팝업·툴팁·카드)는 따로 치지 않는다 — 세그먼트를 전부 돌리므로 사각형 안의 오버레이도 다시 그려지고, 오버레이가
//!   생기거나 사라지는 일은 그쪽이 `redraw()`(전체)를 부른다.

use crate::*;
use std::collections::VecDeque;

/// 한 프레임에 부분 그리기로 받는 사각형 상한 — 넘으면 합집합 하나.
const MAX_DAMAGE_RECTS: usize = 8;

/// 지난 프레임 사각형 이력 길이(IOSurface 삼중 회전 = 나이 3 · 건너뛴 프레임 여유) — 넘는 나이는 전체.
pub(crate) const DAMAGE_HISTORY: usize = 8;

/// 이번 프레임을 어떻게 그릴지.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FramePlan {
    /// 창 전체.
    Full,
    /// 이 사각형들만(클립 그리기 + 부분 present).
    Partial(Vec<Rect>),
}

/// 프레임 계획(순수 · MC/DC): 부분은 설정 on · 전체 요청 없음 · 버퍼 내용을 아는 때(`age` ≥ 1)일 때만 — `age` n이면
/// **이번 사각형 ∪ 지난 n−1 프레임의 사각형**(`prev` · 최신 먼저 · 이력이 모자라면 전체)을 창 안으로 자르고(빈 것·포함된 것 제거)
/// [`MAX_DAMAGE_RECTS`]를 넘으면 합집합 하나로. 남는 것이 없으면 전체(그릴 이유를 모른다).
pub(crate) fn frame_plan(
    enabled: bool,
    full: bool,
    age: u8,
    rects: &[Rect],
    win: Rect,
    prev: &VecDeque<Vec<Rect>>,
) -> FramePlan {
    if !enabled || full || age == 0 {
        return FramePlan::Full;
    }
    let back = usize::from(age) - 1;
    if back > prev.len() {
        return FramePlan::Full;
    }
    let mut out: Vec<Rect> = Vec::new();
    for r in rects
        .iter()
        .chain(prev.iter().take(back).flatten())
        .map(|r| r.intersection(&win))
        .filter(|r| !r.is_empty())
    {
        if out.iter().any(|k| k.intersection(&r) == r) {
            continue;
        }
        out.retain(|k| r.intersection(k) != *k);
        out.push(r);
    }
    if out.is_empty() {
        return FramePlan::Full;
    }
    if out.len() > MAX_DAMAGE_RECTS {
        let u = out.iter().skip(1).fold(out[0], |a, r| a.union(r));
        out = vec![u];
    }
    FramePlan::Partial(out)
}

/// 이력에 넣을 "이 프레임에서 **바뀐** 사각형"(순수): 내용이 전부 바뀐 전체 프레임(`content_full`) = 창 전체 · 그 밖(부분 · 나이 부족만으로
/// 전체를 그린 프레임) = 자기 사각형만.
pub(crate) fn history_entry(own: &[Rect], win: Rect, content_full: bool) -> Vec<Rect> {
    if content_full {
        return vec![win];
    }
    own.iter()
        .map(|r| r.intersection(&win))
        .filter(|r| !r.is_empty())
        .collect()
}

/// 주 창의 래스터 컨텍스트마다 이번 프레임의 클립을 건다(부분 프레임이 아니면 그대로).
pub(crate) trait ClipExt {
    #[must_use]
    fn clipped(self, clip: Option<Rect>) -> Self;
}

impl ClipExt for RasterCtx<'_, '_, '_> {
    fn clipped(mut self, clip: Option<Rect>) -> Self {
        if let Some(r) = clip {
            self.push_clip(r);
        }
        self
    }
}

impl App {
    /// **이 사각형만** 다시 그려 달라(자기 영역을 아는 요청 · 지금은 캐럿 깜빡임) — 설정이 꺼져 있거나 빈 사각형이면 전체([`App::redraw`]).
    pub(crate) fn redraw_rect(&self, r: Rect) {
        if r.is_empty() || !self.settings.flag("gfx.partial_redraw") {
            self.redraw();
            return;
        }
        let cur = self.dmg_rect.get();
        self.dmg_rect.set(Some(cur.map_or(r, |c| c.union(&r))));
        self.request_frame();
    }

    /// 이번 프레임의 계획을 정하고 요청 상태를 비운다 → (계획, 자기 사각형, 내용 전체 여부). `age` = 버퍼 나이 · `win` = 창 사각형.
    pub(crate) fn frame_plan_now(&mut self, age: u8, win: Rect) -> (FramePlan, Vec<Rect>, bool) {
        let full_req = self.dmg_full.replace(false);
        let own: Vec<Rect> = self.dmg_rect.take().into_iter().collect();
        // 사각형 없는 프레임(OS 노출 · 직접 요청) = 이유를 모른다 → 전체 · 캡처는 늘 온전한 프레임으로.
        let content_full = full_req || own.is_empty() || self.capture_req.is_some();
        let enabled = self.settings.flag("gfx.partial_redraw");
        let plan = frame_plan(enabled, content_full, age, &own, win, &self.dmg_prev);
        (plan, own, content_full)
    }

    /// 낸 프레임을 이력에(최신 먼저 · [`DAMAGE_HISTORY`]까지).
    pub(crate) fn frame_note(&mut self, own: &[Rect], win: Rect, content_full: bool) {
        self.dmg_prev
            .push_front(history_entry(own, win, content_full));
        self.dmg_prev.truncate(DAMAGE_HISTORY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Rect = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 60,
    };

    fn hist(v: &[&[Rect]]) -> VecDeque<Vec<Rect>> {
        v.iter().map(|r| r.to_vec()).collect()
    }

    /// 판정 조건마다 하나씩 뒤집어 본다(MC/DC): 설정 · 전체 요청 · 나이 0 · 이력 부족 · 남는 사각형 없음.
    #[test]
    fn frame_plan_mcdc() {
        let ed = Rect::new(10, 10, 50, 30);
        let none = VecDeque::new();
        let part = FramePlan::Partial(vec![ed]);
        assert_eq!(frame_plan(true, false, 1, &[ed], WIN, &none), part);
        assert_eq!(
            frame_plan(false, false, 1, &[ed], WIN, &none),
            FramePlan::Full
        );
        assert_eq!(
            frame_plan(true, true, 1, &[ed], WIN, &none),
            FramePlan::Full
        );
        assert_eq!(
            frame_plan(true, false, 0, &[ed], WIN, &none),
            FramePlan::Full
        );
        // 나이 3 = 지난 두 프레임이 필요하다 — 이력 1 = 전체 · 이력 2 = 부분(같은 사각형은 하나로).
        assert_eq!(
            frame_plan(true, false, 3, &[ed], WIN, &hist(&[&[ed]])),
            FramePlan::Full
        );
        assert_eq!(
            frame_plan(true, false, 3, &[ed], WIN, &hist(&[&[ed], &[ed]])),
            part
        );
        // 사각형이 없거나 창 밖이면 전체(이유를 모른다).
        assert_eq!(frame_plan(true, false, 1, &[], WIN, &none), FramePlan::Full);
        assert_eq!(
            frame_plan(true, false, 1, &[Rect::new(200, 200, 5, 5)], WIN, &none),
            FramePlan::Full
        );
    }

    /// 지난 프레임이 전체였으면(이력 = 창 전체) 나이 2 프레임은 창 전체를 다시 그린다 · 창 밖은 잘린다 · 포함된 사각형은 버린다.
    #[test]
    fn frame_plan_unions_history_and_clips() {
        let ed = Rect::new(10, 10, 50, 30);
        assert_eq!(
            frame_plan(true, false, 2, &[ed], WIN, &hist(&[&[WIN]])),
            FramePlan::Partial(vec![WIN])
        );
        let over = Rect::new(80, 40, 50, 50);
        assert_eq!(
            frame_plan(true, false, 1, &[over], WIN, &VecDeque::new()),
            FramePlan::Partial(vec![Rect::new(80, 40, 20, 20)])
        );
        let inner = Rect::new(20, 20, 5, 5);
        assert_eq!(
            frame_plan(true, false, 2, &[inner], WIN, &hist(&[&[ed]])),
            FramePlan::Partial(vec![ed])
        );
        // 상한을 넘으면 합집합 하나.
        let many: Vec<Rect> = (0..9).map(|i| Rect::new(i * 10, 0, 4, 4)).collect();
        assert_eq!(
            frame_plan(true, false, 1, &many, WIN, &VecDeque::new()),
            FramePlan::Partial(vec![Rect::new(0, 0, 84, 4)])
        );
    }

    /// 이력 = 그 프레임에서 **바뀐 것**: 내용 전체 = 창 전체 · 아니면 자기 사각형(창 안으로) — 전체가 이력을 타고 번지지 않는다.
    #[test]
    fn history_entry_is_own_damage() {
        let ed = Rect::new(10, 10, 50, 30);
        assert_eq!(history_entry(&[ed], WIN, true), vec![WIN]);
        assert_eq!(history_entry(&[ed], WIN, false), vec![ed]);
        assert_eq!(
            history_entry(&[Rect::new(300, 0, 5, 5)], WIN, false),
            vec![]
        );
        // 전체 한 번 뒤 깜빡임이 이어지면 나이 2 버퍼는 한 프레임만 전체이고 곧 사각형으로 수렴한다.
        let mut prev: VecDeque<Vec<Rect>> = VecDeque::new();
        prev.push_front(history_entry(&[], WIN, true));
        let mut plans = Vec::new();
        for _ in 0..3 {
            let plan = frame_plan(true, false, 2, &[ed], WIN, &prev);
            plans.push(plan == FramePlan::Partial(vec![ed]));
            prev.push_front(history_entry(&[ed], WIN, false));
        }
        assert_eq!(plans, vec![false, true, true]);
    }
}
