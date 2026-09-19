//! **외부 파일 변경 처리**(docs/58 · T-140 · 사용자 09-19): 탭별 상태 · 순수 판정 · 확인 띠.
//!
//! 감지(서명·해시·스레드)는 `nexa_fs::watch` · 병합은 `nexa_ctl::merge3` · 언제 확인하고 무엇을 적용할지는 호스트(`main.rs ext_*`).
//! 이 파일은 ① 탭별 상태 [`ExtInfo`] ② **순수 판정** [`decide`](테스트 대상) ③ 비모달 확인 띠 [`Banner`](그리기·히트만).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::Merge3;
use nexa_fs::watch::FileSig;
use std::time::Instant;

/// 확인을 기다리는 디스크 내용(겹치는 변경 · 또는 `ask` 모드).
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    pub sig: FileSig,
    /// 디스크 본문(`\n` 정규화) · 줄끝 · 인코딩.
    pub text: String,
    pub eol: crate::eol::Eol,
    pub enc: String,
    /// 겹친 덩어리 수(0 = 병합을 시도하지 않았다 — 묻기 모드 · 크기 초과 · 줄어듦).
    pub conflicts: usize,
}

/// 탭 하나의 외부 변경 상태.
#[derive(Clone, Debug, Default)]
pub(crate) struct ExtInfo {
    /// 마지막으로 **우리 버퍼의 기준(base)과 맞춘** 디스크 서명(읽기·저장·반영 직후).
    pub sig: Option<FileSig>,
    /// 확인 대기.
    pub pending: Option<Pending>,
    /// "현재 내용 유지"를 고른 디스크 서명 — 디스크가 **다시** 바뀔 때만 재평가한다.
    pub ignored: Option<FileSig>,
    /// 디스크가 base와 다른 채 내 내용을 유지하고 있다 → 저장은 2단 확인(덮어쓰기).
    pub diverged: bool,
    /// 파일이 디스크에서 없어졌다(탭·버퍼는 유지 · 저장하면 되살린다).
    pub deleted: bool,
    /// 탭별 "조용히 따라가기" — 띠를 띄우지 않고(겹치면 내 내용 유지) 비활성 창에서도 확인한다.
    pub follow: bool,
    /// 최근 변경 시각(60초 안 3번 = 따라가기 제안).
    pub recent: Vec<Instant>,
}

impl ExtInfo {
    /// 감시 스레드에 줄 "아는 서명" — 확인 대기 중이면 그 서명(같은 변경으로 다시 깨우지 않게) · 유지했으면 그 서명.
    pub(crate) fn known(&self) -> Option<FileSig> {
        if self.deleted {
            return None;
        }
        self.pending
            .as_ref()
            .map(|p| p.sig)
            .or(self.ignored)
            .or(self.sig)
    }

    /// 변경 한 번 기록 → 60초 안에 3번째면 true(따라가기를 제안할 때).
    pub(crate) fn note_change(&mut self, now: Instant) -> bool {
        self.recent
            .retain(|t| now.duration_since(*t).as_secs() < 60);
        self.recent.push(now);
        self.recent.len() >= 3
    }
}

/// 디스크 내용을 받았을 때 할 일(순수 판정 · docs/58 §2-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// 내용이 기준과 같다(touch · 동기화 클라이언트 · 내가 저장한 것) — 서명만 갱신.
    Ignore,
    /// 디스크 == 버퍼(같은 수정을 양쪽에서) — 기준만 디스크로(탭은 clean이 된다).
    AdoptBase,
    /// 고치지 않은 탭 — 디스크 내용으로(편집 한 단계).
    Reload,
    /// 고친 탭 + 겹침 0 — 병합 결과를 넣는다(기준 = 디스크 · 탭은 dirty 유지).
    Merge(Merge3),
    /// 물어야 한다(겹친 덩어리 수 · 0 = 병합을 시도하지 않음).
    Ask(usize),
}

/// 판정 입력.
pub(crate) struct DecideIn<'a> {
    pub base: &'a str,
    pub buf: &'a str,
    pub disk: &'a str,
    /// `file.external_change` = `ask`(늘 묻기).
    pub ask_always: bool,
    /// `file.external_merge`.
    pub merge_on: bool,
    /// 병합 크기 상한 안인가(`file.external_merge_max_kb`).
    pub size_ok: bool,
    /// 인코딩·줄끝이 바뀌었는가(바뀌었으면 병합하지 않는다).
    pub format_changed: bool,
}

pub(crate) fn decide(i: &DecideIn) -> Decision {
    if i.disk == i.base {
        return Decision::Ignore;
    }
    if i.disk == i.buf {
        return Decision::AdoptBase;
    }
    if i.ask_always {
        return Decision::Ask(0);
    }
    if i.buf == i.base {
        return Decision::Reload;
    }
    // 고친 탭 — 병합 안전 조건(docs/58 §2-2 ④): 설정 · 크기 · 형식 · 절반 이하로 줄어듦.
    let shrunk = i.disk.len() * 2 < i.base.len();
    if !i.merge_on || !i.size_ok || i.format_changed || shrunk {
        return Decision::Ask(0);
    }
    match nexa_ctl::merge3(i.base, i.buf, i.disk) {
        Some(m) if m.conflicts == 0 => Decision::Merge(m),
        Some(m) => Decision::Ask(m.conflicts),
        None => Decision::Ask(0),
    }
}

/// 띠에서 고른 동작.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BannerHit {
    None,
    /// 띠 몸통(클릭을 편집기로 흘리지 않는다).
    Body,
    Diff,
    Reload,
    Keep,
    Follow,
    /// 저장 덮어쓰기(저장이 막힌 상태의 띠).
    Overwrite,
    /// 닫기(삭제 안내).
    Dismiss,
}

/// 띠 내용(호스트가 만든다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BannerView {
    pub text: String,
    pub buttons: Vec<(BannerHit, String)>,
    /// 위험색(삭제 · 저장 막힘) 아니면 경고색.
    pub danger: bool,
}

/// 편집기 위 비모달 확인 띠 — 입력·실행을 막지 않고 포커스를 가져오지 않는다.
#[derive(Default)]
pub(crate) struct Banner {
    view: Option<BannerView>,
    bounds: Rect,
    rects: Vec<Rect>,
    hover: Option<usize>,
}

impl Banner {
    /// 내용 교체 — 바뀌었으면 true.
    pub(crate) fn set(&mut self, view: Option<BannerView>) -> bool {
        if self.view == view {
            return false;
        }
        self.view = view;
        self.rects.clear();
        self.hover = None;
        true
    }

    #[cfg(test)]
    pub(crate) fn is_shown(&self) -> bool {
        self.view.is_some()
    }

    /// 띠 높이(논리 px) — 보일 때만.
    pub(crate) fn height(&self) -> f32 {
        if self.view.is_some() {
            30.0
        } else {
            0.0
        }
    }

    pub(crate) fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }

    pub(crate) fn hover(&mut self, p: Point) -> bool {
        let h = self.rects.iter().position(|r| r.contains(p));
        if h != self.hover {
            self.hover = h;
            return true;
        }
        false
    }

    pub(crate) fn click(&self, p: Point) -> BannerHit {
        let Some(v) = &self.view else {
            return BannerHit::None;
        };
        if let Some(i) = self.rects.iter().position(|r| r.contains(p)) {
            return v.buttons.get(i).map_or(BannerHit::Body, |b| b.0);
        }
        if self.bounds.contains(p) {
            BannerHit::Body
        } else {
            BannerHit::None
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let Some(v) = &self.view else {
            return;
        };
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        let px = |x: f32| (x * s).round() as i32;
        let accent = if v.danger { th.danger } else { th.warn };
        dc.fill_rect(b, th.panel_bg_alt);
        dc.fill_rect_alpha(b, accent, 0.10);
        dc.fill_rect(Rect::new(b.x, b.y, px(4.0), b.h), accent);
        dc.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), th.border);
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let btn_h = lh + px(6.0);
        let by = b.y + (b.h - btn_h) / 2;
        // 버튼은 오른쪽에서부터 · 글은 남은 자리에(넘치면 잘린다).
        self.rects = vec![Rect::default(); v.buttons.len()];
        let mut bx = b.right() - px(8.0);
        for (i, (_, label)) in v.buttons.iter().enumerate().rev() {
            let bw = dc.text_width(label) + px(18.0);
            bx -= bw;
            let r = Rect::new(bx, by, bw, btn_h);
            let hot = self.hover == Some(i);
            dc.fill_round_rect(r, px(4.0), th.field_bg);
            dc.stroke_round_rect(r, px(4.0), if hot { th.accent } else { th.border }, 1.0);
            let ty = dc.text_center_y(r.y, r.h);
            dc.text(r.x + px(9.0), ty, r, label, th.text);
            self.rects[i] = r;
            bx -= px(6.0);
        }
        let tx = b.x + px(12.0);
        let clip = Rect::new(tx, b.y, (bx - tx).max(0), b.h);
        let ty = dc.text_center_y(b.y, b.h);
        dc.text(tx, ty, clip, &v.text, th.text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "select a,\n       b\n  from t\n where x = 1\n order by a;\n";

    fn input<'a>(buf: &'a str, disk: &'a str) -> DecideIn<'a> {
        DecideIn {
            base: BASE,
            buf,
            disk,
            ask_always: false,
            merge_on: true,
            size_ok: true,
            format_changed: false,
        }
    }

    /// 판정표(docs/58 §2-1) — 조건 하나씩만 바꿔 결과가 뒤집히는 쌍.
    #[test]
    fn decide_table() {
        let mine = BASE.replace("       b\n", "       b2\n");
        let theirs = BASE.replace("order by a", "order by b");
        let clash = BASE.replace("       b\n", "       B\n");
        assert_eq!(
            decide(&input(BASE, BASE)),
            Decision::Ignore,
            "내용 같음 = touch"
        );
        assert_eq!(decide(&input(&mine, BASE)), Decision::Ignore);
        assert_eq!(decide(&input(&theirs, &theirs)), Decision::AdoptBase);
        assert_eq!(
            decide(&input(BASE, &theirs)),
            Decision::Reload,
            "고치지 않은 탭"
        );
        match decide(&input(&mine, &theirs)) {
            Decision::Merge(m) => {
                assert_eq!((m.conflicts, m.applied), (0, 1));
                assert!(m.text.contains("b2") && m.text.contains("order by b"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            decide(&input(&mine, &clash)),
            Decision::Ask(1),
            "같은 줄 = 묻는다"
        );
        // 안전 조건 각각이 혼자서 병합을 막는다.
        for f in [
            |i: &mut DecideIn| i.merge_on = false,
            |i: &mut DecideIn| i.size_ok = false,
            |i: &mut DecideIn| i.format_changed = true,
        ] {
            let mut i = input(&mine, &theirs);
            f(&mut i);
            assert_eq!(decide(&i), Decision::Ask(0));
        }
        let mut i = input(BASE, &theirs);
        i.ask_always = true;
        assert_eq!(decide(&i), Decision::Ask(0), "묻기 모드 = clean도 묻는다");
        // 절반 이하로 줄어든 파일은 병합하지 않는다(잘림·실수 방지).
        assert_eq!(decide(&input(&mine, "select 1;\n")), Decision::Ask(0));
    }

    #[test]
    fn known_sig_and_follow_suggestion() {
        let sig = |len| FileSig {
            len,
            mtime: None,
            id: None,
        };
        let mut e = ExtInfo {
            sig: Some(sig(1)),
            ..ExtInfo::default()
        };
        assert_eq!(e.known(), Some(sig(1)));
        e.ignored = Some(sig(2));
        assert_eq!(
            e.known(),
            Some(sig(2)),
            "유지한 서명 = 다시 바뀔 때만 깨운다"
        );
        e.pending = Some(Pending {
            sig: sig(3),
            text: String::new(),
            eol: crate::eol::Eol::Lf,
            enc: "utf8".into(),
            conflicts: 1,
        });
        assert_eq!(e.known(), Some(sig(3)));
        e.deleted = true;
        assert_eq!(e.known(), None, "없어진 파일 = 다시 나타나면 사건");
        let now = Instant::now();
        assert!(!e.note_change(now) && !e.note_change(now));
        assert!(e.note_change(now), "60초 안 3번째 = 따라가기 제안");
    }

    #[test]
    fn banner_hit_needs_paint_geometry() {
        let mut b = Banner::default();
        assert!(!b.is_shown() && b.height() == 0.0);
        assert!(b.set(Some(BannerView {
            text: "x".into(),
            buttons: vec![(BannerHit::Reload, "Reload".into())],
            danger: false,
        })));
        assert!(b.is_shown() && b.height() > 0.0);
        b.set_bounds(Rect::new(0, 0, 400, 30));
        b.rects = vec![Rect::new(300, 5, 80, 20)];
        assert_eq!(b.click(Point { x: 310, y: 10 }), BannerHit::Reload);
        assert_eq!(b.click(Point { x: 10, y: 10 }), BannerHit::Body);
        assert_eq!(b.click(Point { x: 10, y: 100 }), BannerHit::None);
        assert!(b.set(None) && !b.is_shown());
    }
}
