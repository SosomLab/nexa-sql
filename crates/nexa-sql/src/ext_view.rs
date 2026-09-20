//! **확장 상세 뷰**(사용자 09-19 "VS Code처럼 · 편집기 탭이 아니라 확장 탭으로"): 편집기 자리에 그리는 전용 페이지.
//!
//! 탭 줄에는 "Extension: <이름>" 탭이 생기지만 그 탭의 본문은 글 편집기가 아니라 이 뷰다 — 머리(이름 · 버전·종류·만든이 ·
//! 한 줄 설명) · 동작 버튼(설치 | 끄기/켜기 · 삭제) · 세부 정보 표 · 설명(줄 접기) · 세로 스크롤. 편집·실행 대상이 아니다.
//! 이 파일은 그리기·히트만 — 설치·켜기/끄기는 호스트가 확장 패널과 같은 경로(`ext_pick`)로 한다.

use crate::ext_panel::ExtRow;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::InputEvent;
use nsql_i18n::{t, Msg};

/// 상세 페이지 하나의 내용(호스트가 만든다 · 탭마다 하나).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExtDetail {
    pub row: ExtRow,
    /// (라벨, 값) — 값이 빈 줄은 호스트가 뺀다.
    pub fields: Vec<(String, String)>,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExtViewAction {
    Install(usize),
    Remove(String),
    Enable(String),
    Disable(String),
}

#[derive(Default)]
pub(crate) struct ExtView {
    bounds: Rect,
    scale: f32,
    scroll: i32,
    content_h: i32,
    hover: Option<usize>,
    /// 그릴 때 잡은 버튼 영역(동작 · 자리).
    buttons: Vec<(ExtViewAction, Rect)>,
    actions: Vec<ExtViewAction>,
}

impl ExtView {
    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
    }

    /// 다른 상세 탭으로 바뀌었다 — 스크롤·hover를 처음으로.
    pub(crate) fn reset(&mut self) {
        self.scroll = 0;
        self.hover = None;
        self.buttons.clear();
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExtViewAction> {
        std::mem::take(&mut self.actions)
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// 사건 처리(마우스는 뷰 영역 안일 때만 호스트가 넘긴다) — 다시 그릴 일이 있으면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                let h = self.buttons.iter().position(|(_, r)| r.contains(p));
                if h != self.hover {
                    self.hover = h;
                    return true;
                }
                false
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if let Some((a, _)) = self.buttons.iter().find(|(_, r)| r.contains(p)) {
                    self.actions.push(a.clone());
                }
                true
            }
            InputEvent::Wheel { delta } => {
                let max = (self.content_h - self.bounds.h).max(0);
                let next = (self.scroll - delta / 120 * self.s(60.0)).clamp(0, max);
                let moved = next != self.scroll;
                self.scroll = next;
                moved
            }
            _ => false,
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme, d: &ExtDetail) {
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        dc.fill_rect(b, th.field_bg);
        let pad = self.s(24.0);
        let x = b.x + pad;
        let w = (b.w - pad * 2).max(self.s(80.0));
        let clip = b;
        let mut y = b.y + pad - self.scroll;
        // ── 머리: 이름(크게) · 버전·종류·상태 · 한 줄 설명
        // 제목 = `Message` 자리(호스트가 UI 글꼴의 1.7배로 넣어 준다).
        dc.select_font(FontSlot::Message, true);
        let th_title = dc.text_height();
        dc.text(x, y, clip, &d.row.name, th.text);
        y += th_title + self.s(4.0);
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let state = t(if !d.row.installed {
            Msg::ExtStNotInstalled
        } else if d.row.enabled {
            Msg::ExtStEnabled
        } else {
            Msg::ExtStDisabled
        });
        let sub = format!(
            "{}  ·  {}  ·  {}  ·  {state}",
            d.row.id, d.row.version, d.row.kind
        );
        dc.text(x, y, clip, &sub, th.text_dim);
        y += lh + self.s(8.0);
        if !d.row.summary.is_empty() {
            for line in wrap(dc, &d.row.summary, w) {
                dc.text(x, y, clip, &line, th.text);
                y += lh + self.s(2.0);
            }
            y += self.s(8.0);
        }
        // ── 동작 버튼
        let labels: Vec<(ExtViewAction, String, bool)> = if d.row.installed {
            vec![
                (
                    if d.row.enabled {
                        ExtViewAction::Disable(d.row.id.clone())
                    } else {
                        ExtViewAction::Enable(d.row.id.clone())
                    },
                    t(if d.row.enabled {
                        Msg::ExtBtnDisable
                    } else {
                        Msg::ExtBtnEnable
                    })
                    .to_string(),
                    false,
                ),
                (
                    ExtViewAction::Remove(d.row.id.clone()),
                    t(Msg::ExtBtnRemove).to_string(),
                    false,
                ),
            ]
        } else {
            match d.row.catalog {
                Some(n) => vec![(
                    ExtViewAction::Install(n),
                    t(Msg::ExtBtnInstall).to_string(),
                    true,
                )],
                None => Vec::new(),
            }
        };
        self.buttons.clear();
        let btn_h = lh + self.s(12.0);
        let mut bx = x;
        for (i, (action, label, primary)) in labels.into_iter().enumerate() {
            let bw = dc.text_width(&label) + self.s(28.0);
            let r = Rect::new(bx, y, bw, btn_h);
            let hot = self.hover == Some(i);
            let ty = dc.text_center_y(r.y, r.h);
            if primary {
                dc.fill_round_rect(r, self.s(5.0), th.accent);
                if hot {
                    dc.fill_round_rect_alpha(r, self.s(5.0), th.text, 0.15);
                }
                dc.text(
                    r.x + self.s(14.0),
                    ty,
                    clip,
                    &label,
                    nexa_ctl::Color(0x00FF_FFFF),
                );
            } else {
                dc.fill_round_rect(r, self.s(5.0), th.panel_bg_alt);
                dc.stroke_round_rect(r, self.s(5.0), if hot { th.accent } else { th.border }, 1.0);
                dc.text(r.x + self.s(14.0), ty, clip, &label, th.text);
            }
            // 영역 밖으로 스크롤된 버튼은 누를 수 없다.
            if r.y >= b.y && r.bottom() <= b.bottom() {
                self.buttons.push((action, r));
            }
            bx += bw + self.s(8.0);
        }
        if !self.buttons.is_empty() || bx > x {
            y += btn_h + self.s(18.0);
        }
        dc.fill_rect(Rect::new(x, y, w, 1).intersection(&b), th.border);
        y += self.s(16.0);
        // ── 세부 정보(라벨 열 = 가장 긴 라벨 폭)
        dc.select_font(FontSlot::Base, true);
        dc.text(x, y, clip, t(Msg::ExtViewDetails), th.text);
        y += lh + self.s(8.0);
        dc.select_font(FontSlot::Base, false);
        let label_w = d
            .fields
            .iter()
            .map(|(l, _)| dc.text_width(l))
            .max()
            .unwrap_or(0)
            + self.s(20.0);
        for (label, value) in &d.fields {
            dc.text(x, y, clip, label, th.text_dim);
            let lines = wrap(dc, value, (w - label_w).max(self.s(80.0)));
            for line in lines {
                dc.text(x + label_w, y, clip, &line, th.text);
                y += lh + self.s(3.0);
            }
            y += self.s(3.0);
        }
        // ── 설명
        if !d.description.is_empty() {
            y += self.s(12.0);
            dc.select_font(FontSlot::Base, true);
            dc.text(x, y, clip, t(Msg::ExtViewDescription), th.text);
            y += lh + self.s(8.0);
            dc.select_font(FontSlot::Base, false);
            for para in d.description.split('\n') {
                for line in wrap(dc, para, w) {
                    dc.text(x, y, clip, &line, th.text);
                    y += lh + self.s(3.0);
                }
                y += self.s(6.0);
            }
        }
        self.content_h = y + self.scroll - b.y + pad;
        // 내용이 줄었으면 스크롤을 다시 맞춘다(다음 프레임에 반영).
        self.scroll = self.scroll.clamp(0, (self.content_h - b.h).max(0));
    }
}

/// 단어 단위 줄 접기(한 단어가 폭을 넘으면 그대로 둔다 — 잘려 보인다).
fn wrap(dc: &mut dyn DrawCtx, text: &str, max_w: i32) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let cand = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && dc.text_width(&cand) > max_w {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = cand;
        }
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 버튼 히트 → 동작 · 휠 = 내용 높이 안에서만 · 다른 탭으로 바뀌면 처음으로.
    #[test]
    fn click_wheel_and_reset() {
        let mut v = ExtView::default();
        v.set_bounds(Rect::new(100, 50, 600, 300), 1.0);
        v.content_h = 900;
        v.buttons = vec![
            (
                ExtViewAction::Disable("x".into()),
                Rect::new(124, 150, 80, 28),
            ),
            (
                ExtViewAction::Remove("x".into()),
                Rect::new(212, 150, 80, 28),
            ),
        ];
        let down = |x, y| InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        };
        v.on_event(&down(130, 160));
        v.on_event(&down(220, 160));
        v.on_event(&down(500, 250));
        assert_eq!(
            v.take_actions(),
            vec![
                ExtViewAction::Disable("x".into()),
                ExtViewAction::Remove("x".into())
            ]
        );
        assert!(v.on_event(&InputEvent::Wheel { delta: -240 }));
        assert_eq!(v.scroll, 120);
        for _ in 0..20 {
            v.on_event(&InputEvent::Wheel { delta: -240 });
        }
        assert_eq!(v.scroll, 600, "내용 높이 − 보이는 높이에서 멈춘다");
        assert!(!v.on_event(&InputEvent::Wheel { delta: -120 }));
        v.reset();
        assert_eq!((v.scroll, v.buttons.len()), (0, 0));
    }
}
