//! 서버별 오브젝트 탐색기 묶음(docs/52 §2-2 · T-121) — **서버 하나 = 탐색기(트리 + 메타 세션) 하나**.
//!
//! 규칙(사용자 09-18): 탐색기의 메타가 인텔리센스·툴팁의 **단일 원천**이므로, 어떤 서버에 붙은 세션이 **하나라도** 있으면
//! 그 서버의 탐색기는 유지된다(공유·전용·개별 어느 세션이든). 세션이 0이 되면 메타 **접속만** 닫고 읽어 둔 트리는 남긴다
//! (오프라인 · 사용자가 목록에서 직접 지운다 — SSMS식). 메타 세션은 한동안 안 쓰면 유휴 회수하고 다음 요청 때 다시 연다.
//!
//! 화면에는 한 번에 한 서버의 트리만 보인다 — 활성 편집기 탭의 서버를 따라가고(재접속·재조회 없음 = 전환 비용 0),
//! 서버가 둘 이상이면 위에 머리줄(서버 이름 · i/n)이 생겨 눌러서 고른다. 호스트가 보는 API는 [`Explorer`]와 같다.

use crate::explorer::{Explorer, ExplorerAction, LiveReq, LiveResult};
use crate::worker::same_server;
use nexa_ctl::controls::ctxmenu::CtxItem;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::InputEvent;
use nsql_i18n::{t, Msg};
use nsql_script::ConnectSpec;
use std::sync::Arc;

struct Pane {
    /// 서버 키(방언·호스트·포트·DB·계정 · `None` = 아직 아무 서버도 아닌 빈 자리).
    key: Option<ConnectSpec>,
    ex: Explorer,
}

/// 세션 수 변화에 따른 탐색기 한 칸의 처리(순수 판정 · MC/DC 표 D11).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PaneMove {
    Keep,
    /// 붙은 세션이 0이 됐다 → 메타 접속만 닫는다(트리 유지).
    GoOffline,
}

pub(crate) fn pane_move(has_key: bool, offline: bool, live_sessions: usize) -> PaneMove {
    if has_key && !offline && live_sessions == 0 {
        PaneMove::GoOffline
    } else {
        PaneMove::Keep
    }
}

pub(crate) struct ExplorerSet {
    panes: Vec<Pane>,
    shown: usize,
    wake: Arc<dyn Fn() + Send + Sync>,
    visible: bool,
    icons: bool,
    font_px: f32,
    focused: bool,
    bounds: Rect,
    scale: f32,
    header: Rect,
    header_hover: bool,
    header_clicked: bool,
}

const HEADER_H: f32 = 24.0;

impl ExplorerSet {
    pub(crate) fn new(wake: Arc<dyn Fn() + Send + Sync>, visible: bool) -> Self {
        let mut s = ExplorerSet {
            panes: Vec::new(),
            shown: 0,
            wake,
            visible,
            icons: true,
            font_px: 17.0,
            focused: false,
            bounds: Rect::default(),
            scale: 1.0,
            header: Rect::default(),
            header_hover: false,
            header_clicked: false,
        };
        let p = s.new_pane(None);
        s.panes.push(p);
        s
    }

    fn new_pane(&self, key: Option<ConnectSpec>) -> Pane {
        let w = Arc::clone(&self.wake);
        let mut ex = Explorer::new(Box::new(move || w()), self.visible);
        ex.set_icons(self.icons);
        ex.set_font_px(self.font_px);
        Pane { key, ex }
    }

    fn cur(&self) -> &Explorer {
        &self.panes[self.shown].ex
    }

    fn cur_mut(&mut self) -> &mut Explorer {
        &mut self.panes[self.shown].ex
    }

    fn find(&self, spec: &ConnectSpec) -> Option<usize> {
        self.panes
            .iter()
            .position(|p| p.key.as_ref().is_some_and(|k| same_server(k, spec)))
    }

    fn keyed(&self) -> usize {
        self.panes.iter().filter(|p| p.key.is_some()).count()
    }

    // ── 서버 관리

    /// 세션이 이 서버에 붙었다 — 탐색기가 없으면 만들고(메타 접속), 있으면 그대로 둔다(오프라인이었으면 다시 붙인다).
    /// `show` = 이 서버의 트리를 앞으로(활성 탭의 세션일 때).
    pub(crate) fn connect(&mut self, spec: &ConnectSpec, name: &str, show: bool) {
        let i = match self.find(spec) {
            Some(i) => {
                if self.panes[i].ex.is_offline() {
                    self.panes[i].ex.connect(spec, name);
                }
                i
            }
            None => {
                let i = match self.panes.iter().position(|p| p.key.is_none()) {
                    Some(i) => i,
                    None => {
                        let p = self.new_pane(None);
                        self.panes.push(p);
                        self.panes.len() - 1
                    }
                };
                let mut key = spec.clone();
                key.password = None;
                self.panes[i].key = Some(key);
                self.panes[i].ex.connect(spec, name);
                i
            }
        };
        if show {
            self.show(i);
        }
    }

    /// 활성 탭의 서버를 앞으로(탐색기가 있는 서버일 때만 · 재접속 없음).
    pub(crate) fn show_for(&mut self, spec: Option<&ConnectSpec>) {
        if let Some(i) = spec.and_then(|s| self.find(s)) {
            self.show(i);
        }
    }

    fn show(&mut self, i: usize) {
        if i < self.panes.len() && i != self.shown {
            self.panes[self.shown].ex.set_focused(false);
            self.shown = i;
            let f = self.focused;
            self.panes[i].ex.set_focused(f);
        }
        self.relayout();
    }

    /// 지금 붙어 있는 세션들의 서버 목록으로 참조 수를 맞춘다 — 0이 된 서버는 오프라인(트리 유지).
    pub(crate) fn sync_refs(&mut self, live: &[ConnectSpec]) -> bool {
        let mut changed = false;
        for p in &mut self.panes {
            let n = p
                .key
                .as_ref()
                .map_or(0, |k| live.iter().filter(|s| same_server(k, s)).count());
            if pane_move(p.key.is_some(), p.ex.is_offline(), n) == PaneMove::GoOffline {
                p.ex.go_offline();
                changed = true;
            }
        }
        changed
    }

    /// 메타 세션 유휴 회수(세션 유휴 닫기와 같은 한도 · 0 = 끔).
    pub(crate) fn idle_tick(&mut self, limit_secs: u64) {
        for p in &mut self.panes {
            p.ex.suspend_if_idle(limit_secs);
        }
    }

    /// 머리줄이 눌렸다(1회성) → 호스트가 서버 메뉴를 그 아래에 연다.
    pub(crate) fn take_header_click(&mut self) -> Option<Rect> {
        std::mem::take(&mut self.header_clicked).then_some(self.header)
    }

    /// 서버 메뉴 항목 — `ex.show:i` · 오프라인 서버는 `ex.remove:i`(붙은 세션이 있는 서버는 지울 수 없다).
    pub(crate) fn server_menu(&self) -> Vec<CtxItem> {
        let mut items = Vec::new();
        let mut removable = Vec::new();
        for (i, p) in self.panes.iter().enumerate() {
            if p.key.is_none() {
                continue;
            }
            let (name, ep) = p.ex.title();
            let mark = if i == self.shown { "●" } else { "   " };
            let label = if p.ex.is_offline() {
                format!("{mark} {name}  {ep} · {}", t(Msg::ExpOffline))
            } else {
                format!("{mark} {name}  {ep}")
            };
            items.push(CtxItem::maybe(
                format!("ex.show:{i}"),
                label,
                i != self.shown,
            ));
            if p.ex.is_offline() {
                removable.push((i, name.to_string()));
            }
        }
        if !removable.is_empty() {
            items.push(CtxItem::Separator);
            for (i, name) in removable {
                items.push(CtxItem::item(
                    format!("ex.remove:{i}"),
                    format!("{} — {name}", t(Msg::ExpRemoveServer)),
                ));
            }
        }
        items
    }

    pub(crate) fn menu_pick(&mut self, id: &str) -> bool {
        if let Some(i) = id.strip_prefix("ex.show:").and_then(|v| v.parse().ok()) {
            self.show(i);
            true
        } else if let Some(i) = id
            .strip_prefix("ex.remove:")
            .and_then(|v| v.parse::<usize>().ok())
        {
            if i < self.panes.len() && self.panes[i].ex.is_offline() {
                self.panes[i].ex.disconnect();
                if self.panes.len() > 1 {
                    self.panes.remove(i);
                } else {
                    self.panes[i].key = None;
                }
                // 지운 칸보다 뒤를 보고 있었으면 한 칸 당긴다 · 보던 칸을 지웠으면 같은 자리(없으면 마지막).
                if i < self.shown {
                    self.shown -= 1;
                }
                self.shown = self.shown.min(self.panes.len() - 1);
                let (on, f) = (self.visible, self.focused);
                self.cur_mut().set_visible(on);
                self.cur_mut().set_focused(f);
                self.relayout();
            }
            true
        } else {
            false
        }
    }

    // ── `Explorer`와 같은 호스트 API

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        for p in &mut self.panes {
            p.ex.set_visible(on);
        }
    }

    pub(crate) fn set_icons(&mut self, on: bool) {
        self.icons = on;
        for p in &mut self.panes {
            p.ex.set_icons(on);
        }
    }

    pub(crate) fn set_font_px(&mut self, px: f32) {
        self.font_px = px;
        for p in &mut self.panes {
            p.ex.set_font_px(px);
        }
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        self.cur_mut().set_focused(on);
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.cur().menu_open()
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        self.relayout();
    }

    fn relayout(&mut self) {
        let b = self.bounds;
        let h = if self.keyed() > 1 {
            (HEADER_H * self.scale).round() as i32
        } else {
            0
        };
        self.header = Rect::new(b.x, b.y, b.w, h.min(b.h));
        let body = Rect::new(b.x, b.y + h, b.w, (b.h - h).max(0));
        let s = self.scale;
        self.cur_mut().set_bounds(body, s);
    }

    pub(crate) fn live_poll(&mut self, spec: Option<&ConnectSpec>, req: LiveReq) {
        if let Some(i) = spec.and_then(|s| self.find(s)) {
            self.panes[i].ex.live_poll(req);
        }
    }

    pub(crate) fn has_server(&self, spec: Option<&ConnectSpec>) -> bool {
        spec.and_then(|s| self.find(s))
            .is_some_and(|i| !self.panes[i].ex.is_offline())
    }

    pub(crate) fn take_live(&mut self) -> Vec<LiveResult> {
        self.panes
            .iter_mut()
            .flat_map(|p| p.ex.take_live())
            .collect()
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExplorerAction> {
        self.panes
            .iter_mut()
            .flat_map(|p| p.ex.take_actions())
            .collect()
    }

    /// 모든 서버의 메타 응답을 반영(보이지 않는 서버도 뒤에서 읽기가 끝난다).
    pub(crate) fn drain(&mut self) -> bool {
        let mut changed = false;
        for p in &mut self.panes {
            changed |= p.ex.drain();
        }
        changed
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.cur_mut().tick(now_ms)
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.cur().bars_visible()
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if self.header.h > 0 && !self.cur().menu_open() {
            match *ev {
                InputEvent::MouseMove { x, y } => {
                    let over = self.header.contains(Point { x, y });
                    if over != self.header_hover {
                        self.header_hover = over;
                        return true;
                    }
                    if over {
                        return false;
                    }
                }
                InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y }
                    if self.header.contains(Point { x, y }) =>
                {
                    self.header_clicked = true;
                    return true;
                }
                _ => {}
            }
        }
        self.cur_mut().on_event(ev)
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        self.cur_mut().paint(dc, th);
        let h = self.header;
        if h.h == 0 {
            return;
        }
        dc.fill_rect(h, th.chrome_bg);
        if self.header_hover {
            dc.fill_rect_alpha(h, th.text, 0.06);
        }
        dc.fill_rect(Rect::new(h.x, h.bottom() - 1, h.w, 1), th.border);
        dc.fill_rect(Rect::new(h.right() - 1, h.y, 1, h.h), th.border);
        dc.select_font(FontSlot::Base, true);
        let pad = (8.0 * self.scale).round() as i32;
        let ty = dc.text_center_y(h.y, h.h);
        let (name, ep) = self.cur().title();
        let pos = self
            .panes
            .iter()
            .filter(|p| p.key.is_some())
            .position(|p| std::ptr::eq(&p.ex, self.cur()))
            .map_or(1, |i| i + 1);
        let count = format!("{pos}/{}  ▾", self.keyed());
        let cw = dc.text_width(&count);
        let clip = Rect::new(h.x + pad, h.y, (h.w - pad * 3 - cw).max(0), h.h);
        let head = if name.is_empty() { ep } else { name };
        dc.text(clip.x, ty, clip, head, th.text);
        dc.select_font(FontSlot::Base, false);
        let cr = Rect::new(h.right() - pad - cw, h.y, cw, h.h);
        dc.text(cr.x, ty, cr, &count, th.text_dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D11 MC/DC: 서버 키 있음 · 온라인 · 세션 0 — 셋 다 참일 때만 오프라인으로. 각각 하나만 뒤집어 Keep.
    #[test]
    fn mcdc_pane_move() {
        assert_eq!(pane_move(true, false, 0), PaneMove::GoOffline);
        assert_eq!(pane_move(false, false, 0), PaneMove::Keep, "빈 자리");
        assert_eq!(pane_move(true, true, 0), PaneMove::Keep, "이미 오프라인");
        assert_eq!(
            pane_move(true, false, 1),
            PaneMove::Keep,
            "세션이 하나라도 있으면 유지"
        );
        assert_eq!(pane_move(true, false, 3), PaneMove::Keep);
    }
}
