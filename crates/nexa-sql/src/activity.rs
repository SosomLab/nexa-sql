//! 좌측 **활동 막대**(VS Code Activity Bar 차용 · 사용자 09-15) — 48px 폭의 아이콘 열.
//!
//! - 위: **패널 버튼**(오브젝트 탐색기 …) — 클릭 = 그 패널 펼침/접힘 토글 · 펼친 상태에서 다른 패널 버튼 = 패널 교체 ·
//!   같은 버튼 다시 = 접힘. 활성 패널은 왼쪽 2px 강조 띠 + 본문색 아이콘.
//! - 아래: **동작 버튼**(접속 · 환경 설정) — 패널이 아니라 명령(창 열기).
//! - 아이콘 = 코드 마스크(`toolicons` 계열 · 24px · 테마색 틴트) · hover = 본문색 · 나머지 = 흐리게.

use crate::toolicons;
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{IconImage, InputEvent, MenuIcon};

/// 활동 막대 폭(논리 px · VS Code 48).
pub(crate) const BAR_W: f32 = 48.0;
const ICON_PX: f32 = 24.0;
const SLOT_H: f32 = 48.0;

pub(crate) struct ActItem {
    pub id: &'static str,
    icon: MenuIcon,
    /// 패널 버튼인가(활성 표시 대상) — 아니면 동작 버튼.
    pub panel: bool,
    /// 아래쪽 정렬(동작 버튼).
    bottom: bool,
}

pub(crate) struct ActivityBar {
    bounds: Rect,
    scale: f32,
    items: Vec<ActItem>,
    /// 펼쳐진 패널 id(없으면 접힘).
    active: Option<&'static str>,
    hover: Option<usize>,
    pressed: Option<usize>,
    picked: Option<&'static str>,
}

impl ActivityBar {
    pub(crate) fn new() -> Self {
        ActivityBar {
            bounds: Rect::default(),
            scale: 1.0,
            items: vec![
                ActItem {
                    id: "view.explorer",
                    icon: toolicons::mi_files(),
                    panel: true,
                    bottom: false,
                },
                ActItem {
                    id: "view.search",
                    icon: toolicons::mi_search(),
                    panel: true,
                    bottom: false,
                },
                ActItem {
                    id: "edit.prefs",
                    icon: toolicons::mi_gear(),
                    panel: false,
                    bottom: true,
                },
            ],
            active: None,
            hover: None,
            pressed: None,
            picked: None,
        }
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    /// 펼쳐진 패널(호스트가 탐색기 표시 상태와 동기).
    pub(crate) fn set_active(&mut self, id: Option<&'static str>) {
        self.active = id;
    }

    /// 눌린 항목 id(1회성).
    pub(crate) fn take_picked(&mut self) -> Option<&'static str> {
        self.picked.take()
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn slot_rect(&self, i: usize) -> Rect {
        let b = self.bounds;
        let h = self.s(SLOT_H);
        let tops = self.items.iter().filter(|it| !it.bottom).count();
        let it = &self.items[i];
        if it.bottom {
            let bottoms_after = self.items[i + 1..].iter().filter(|x| x.bottom).count() as i32;
            Rect::new(b.x, b.bottom() - h * (bottoms_after + 1), b.w, h)
        } else {
            let idx = self.items[..i].iter().filter(|x| !x.bottom).count() as i32;
            let _ = tops;
            Rect::new(b.x, b.y + h * idx, b.w, h)
        }
    }

    fn hit(&self, p: Point) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.slot_rect(i).contains(p))
    }

    /// 마우스 사건(커서가 막대 안일 때 호스트가 넘긴다) — 다시 그려야 하면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let h = self.hit(Point { x, y });
                let changed = h != self.hover;
                self.hover = h;
                changed
            }
            InputEvent::MouseDown { x, y, .. } => {
                self.pressed = self.hit(Point { x, y });
                true
            }
            InputEvent::MouseUp { x, y } => {
                let up = self.hit(Point { x, y });
                if let Some(i) = self.pressed.take() {
                    if up == Some(i) {
                        self.picked = Some(self.items[i].id);
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// 커서가 막대를 떠났을 때(호스트가 부른다) — hover 정리.
    pub(crate) fn clear_hover(&mut self) -> bool {
        let had = self.hover.is_some();
        self.hover = None;
        had
    }

    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        let b = self.bounds;
        if b.w <= 0 {
            return;
        }
        dc.fill_rect(b, th.chrome_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        let isz = self.s(ICON_PX);
        for (i, it) in self.items.iter().enumerate() {
            let slot = self.slot_rect(i);
            let active = it.panel && self.active == Some(it.id);
            let hot = self.hover == Some(i) || self.pressed == Some(i);
            let color = if active || hot { th.text } else { th.text_dim };
            if active {
                // 선택 표시 세로줄(사용자 09-18): 두께 4(종전 2의 2배) · 위아래 3px씩 짧게(아이콘 기준 가운데 정렬은 그대로).
                let trim = self.s(3.0);
                dc.fill_rect(
                    Rect::new(
                        slot.x,
                        slot.y + trim,
                        self.s(4.0),
                        (slot.h - trim * 2).max(1),
                    ),
                    th.accent,
                );
            }
            let (r, g, bb) = color.rgb();
            let img =
                IconImage::from_alpha_tinted(it.icon.w, it.icon.h, &it.icon.alpha, (r, g, bb));
            let dy = i32::from(self.pressed == Some(i));
            let dst = Rect::new(
                slot.x + (slot.w - isz) / 2,
                slot.y + (slot.h - isz) / 2 + dy,
                isz,
                isz,
            );
            dc.image_scaled(dst, &img, slot);
        }
    }
}
