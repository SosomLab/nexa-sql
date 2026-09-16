//! 토스트(우측 하단 · 쌓임 · 설정 시간 뒤 사라짐 · 반투명 — 사용자 09-16 "객체가 없어서 오류가 나면 유형과 대상 이름을 토스트로").
//!
//! 카드는 `ui.toast_alpha`(기본 85%)로 그려 아래 내용이 비친다 · 수명은 `ui.toast_secs`(기본 3초) · 마지막 300ms는 페이드 아웃 ·
//! 클릭하면 바로 사라진다 · 최대 5장(오래된 것부터 밀려남). 그리기는 창의 맨 마지막 층(팝업 규칙).

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nsql_i18n::{t, Msg};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Error,
    #[allow(dead_code)]
    Warn,
    #[allow(dead_code)]
    Info,
}

struct Toast {
    kind: ToastKind,
    title: String,
    body: String,
    born: Instant,
    rect: Rect,
}

const MAX_TOASTS: usize = 5;
const FADE_MS: u64 = 300;

pub(crate) struct Toasts {
    items: Vec<Toast>,
    ttl: Duration,
    /// 카드 불투명도(0.3~1.0).
    alpha: f32,
}

impl Toasts {
    pub(crate) fn new() -> Self {
        Toasts {
            items: Vec::new(),
            ttl: Duration::from_secs(3),
            alpha: 0.85,
        }
    }

    /// 설정 `ui.toast_secs` · `ui.toast_alpha`(%).
    pub(crate) fn configure(&mut self, secs: i64, alpha_pct: i64) {
        self.ttl = Duration::from_secs(secs.clamp(1, 60) as u64);
        self.alpha = (alpha_pct.clamp(30, 100) as f32) / 100.0;
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
        });
        while self.items.len() > MAX_TOASTS {
            self.items.remove(0);
        }
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

    /// 클릭으로 닫기 — 카드 위였으면 true(호출자는 그 클릭을 아래로 흘리지 않는다).
    pub(crate) fn click(&mut self, p: Point) -> bool {
        if let Some(i) = self.items.iter().position(|t| t.rect.contains(p)) {
            self.items.remove(i);
            return true;
        }
        false
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
        let mut y = bottom - gap;
        for t in self.items.iter_mut().rev() {
            let age = now.duration_since(t.born);
            let left = self.ttl.saturating_sub(age).as_millis() as u64;
            let fade = if left < FADE_MS {
                left as f32 / FADE_MS as f32
            } else {
                1.0
            };
            let a = self.alpha * fade;
            let h = pad * 2 + lh * 2 + px(2.0);
            let r = Rect::new(right - gap - w, y - h, w, h);
            t.rect = r;
            dc.fill_round_rect_alpha(r, px(6.0), th.panel_bg, a);
            dc.stroke_round_rect_alpha(r, px(6.0), th.border, 1.0, a);
            let color = match t.kind {
                ToastKind::Error => th.danger,
                ToastKind::Warn => th.warn,
                ToastKind::Info => th.accent,
            };
            dc.fill_round_rect_alpha(
                Rect::new(r.x + px(6.0), r.y + pad, bar, h - pad * 2),
                px(2.0),
                color,
                a,
            );
            let tx = r.x + px(6.0) + bar + pad;
            let clip = Rect::new(r.x, r.y, r.w - pad, r.h);
            dc.text(tx, r.y + pad, clip, &t.title, color);
            dc.text(tx, r.y + pad + lh + px(2.0), clip, &t.body, th.text);
            y = r.y - gap;
        }
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

    #[test]
    fn toasts_expire_and_click_dismisses() {
        let mut ts = Toasts::new();
        ts.configure(1, 85);
        ts.push(ToastKind::Error, "a", "b");
        assert!(ts.animating());
        assert!(ts.tick(Instant::now()));
        assert!(!ts.tick(Instant::now() + Duration::from_secs(2)));
        ts.push(ToastKind::Error, "a", "b");
        ts.items[0].rect = Rect::new(0, 0, 10, 10);
        assert!(ts.click(Point { x: 5, y: 5 }));
        assert!(!ts.animating());
    }
}
