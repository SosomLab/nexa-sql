//! 서버별 오브젝트 탐색기 묶음(docs/52 §2-2 · T-121) — **서버 하나 = 탐색기(트리 + 메타 세션) 하나**.
//!
//! 규칙(사용자 09-18): 탐색기의 메타가 인텔리센스·툴팁의 **단일 원천**이므로, 어떤 서버에 붙은 세션이 **하나라도** 있으면
//! 그 서버의 탐색기는 유지된다(공유·전용·개별 어느 세션이든). 세션이 0이 되면 메타 **접속만** 닫고 읽어 둔 트리는 남긴다
//! (오프라인 · 사용자가 목록에서 직접 지운다 — SSMS식). 메타 세션은 한동안 안 쓰면 유휴 회수하고 다음 요청 때 다시 연다.
//!
//! 화면(사용자 09-18): 서버마다 루트 노드가 접속 순으로 추가되고 **한 트리처럼 이어진다** — 첫 서버의 내용이 끝나는 바로 아래에
//! 다음 서버의 루트가 온다("한 폴더 안의 내부 폴더 둘을 펼친 모습" · 나뉜 패널이 아님). 각 서버의 트리는 자기 내용 전체 높이로 놓이고
//! **스크롤은 전체에 하나**(공용 뷰포트 = 탐색기 영역 · 각 트리는 `set_clip`으로 보이는 부분만 그린다).
//! 마우스는 커서 아래 트리로 · 키는 마지막으로 누른 트리로 · 선택은 전체에 하나. 호스트가 보는 API는 [`Explorer`]와 같다.

use crate::explorer::{Explorer, ExplorerAction, LiveReq, LiveResult};
use crate::worker::same_server;
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::InputEvent;
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
    /// 마지막 마우스 위치.
    cursor: Point,
    /// 이어 붙인 트리 전체의 세로 스크롤(px) · 공용 오버레이 스크롤바.
    scroll: i32,
    bars: nexa_ctl::controls::ScrollBars,
}

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
            cursor: Point { x: -1, y: -1 },
            scroll: 0,
            bars: nexa_ctl::controls::ScrollBars::new(),
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

    fn cur_mut(&mut self) -> &mut Explorer {
        &mut self.panes[self.shown].ex
    }

    fn find(&self, spec: &ConnectSpec) -> Option<usize> {
        self.panes
            .iter()
            .position(|p| p.key.as_ref().is_some_and(|k| same_server(k, spec)))
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

    /// 키보드 대상 칸을 옮긴다 — 지금 대상이 빈 자리(서버 아님)일 때만(사용자가 눌러 고른 칸을 탭 전환이 빼앗지 않는다).
    fn show(&mut self, i: usize) {
        if i < self.panes.len() && i != self.shown && self.panes[self.shown].key.is_none() {
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

    /// 오프라인 서버 제거(루트 우클릭 "탐색기에서 제거") — 붙은 세션이 있는 서버는 지울 수 없다(메뉴에 나오지 않는다).
    fn remove(&mut self, i: usize) {
        if i >= self.panes.len() || !self.panes[i].ex.is_offline() {
            return;
        }
        self.panes[i].ex.disconnect();
        if self.panes.len() > 1 {
            self.panes.remove(i);
        } else {
            self.panes[i].key = None;
        }
        if i < self.shown {
            self.shown -= 1;
        }
        self.shown = self.shown.min(self.panes.len() - 1);
        self.relayout();
    }

    /// 화면에 놓이는 칸(접속 순) — 서버가 하나라도 있으면 서버 칸만, 없으면 빈 자리("Not connected") 하나.
    fn laid(&self) -> Vec<usize> {
        let keyed: Vec<usize> = (0..self.panes.len())
            .filter(|&i| self.panes[i].key.is_some())
            .collect();
        if keyed.is_empty() {
            vec![0]
        } else {
            keyed
        }
    }

    fn pane_at(&self, p: Point) -> Option<usize> {
        self.laid()
            .into_iter()
            .find(|&i| self.panes[i].ex.visible_rect().contains(p))
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
        self.panes.iter().any(|p| p.ex.menu_open())
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        self.relayout();
    }

    /// 이어 붙인 전체 높이.
    fn total_h(&self) -> i32 {
        self.laid()
            .iter()
            .map(|&i| self.panes[i].ex.content_height())
            .sum()
    }

    fn relayout(&mut self) {
        let b = self.bounds;
        let s = self.scale;
        let laid = self.laid();
        self.scroll = self.scroll.clamp(0, (self.total_h() - b.h).max(0));
        let mut y = b.y - self.scroll;
        for &i in &laid {
            let h = self.panes[i].ex.content_height();
            let ex = &mut self.panes[i].ex;
            ex.set_bounds(Rect::new(b.x, y, b.w, h), s);
            ex.set_clip(b);
            ex.set_menu_host(b);
            y += h;
        }
        // 놓이지 않은 칸(빈 자리)은 영역 0.
        for i in 0..self.panes.len() {
            if !laid.contains(&i) {
                self.panes[i].ex.set_bounds(Rect::new(b.x, b.y, 0, 0), s);
            }
        }
    }

    /// 키보드로 옮긴 선택이 보이도록 공용 스크롤을 맞춘다.
    fn reveal_selection(&mut self) {
        let b = self.bounds;
        if let Some((y, h)) = self.panes[self.shown].ex.selected_span() {
            if y < b.y {
                self.scroll -= b.y - y;
            } else if y + h > b.bottom() {
                self.scroll += y + h - b.bottom();
            }
            self.bars.show();
            self.relayout();
        }
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
        let mut out = Vec::new();
        let mut remove = None;
        for (i, p) in self.panes.iter_mut().enumerate() {
            for a in p.ex.take_actions() {
                if a == ExplorerAction::RemoveServer {
                    remove = Some(i);
                } else {
                    out.push(a);
                }
            }
        }
        if let Some(i) = remove {
            self.remove(i);
        }
        out
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
        let mut any = self.bars.tick(now_ms);
        for p in &mut self.panes {
            any |= p.ex.tick(now_ms);
        }
        any
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.visible && (self.bars.is_visible() || self.panes.iter().any(|p| p.ex.bars_visible()))
    }

    /// 마우스 라우팅 규칙(CLAUDE.md): 누름·휠은 **커서 아래 칸에만** · 이동은 전 칸(hover 해제용) · 키는 마지막으로 누른 칸 ·
    /// 메뉴가 열린 칸이 있으면 그 칸이 먼저(모달).
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if let Some(i) = self.panes.iter().position(|p| p.ex.menu_open()) {
            return self.panes[i].ex.on_event(ev);
        }
        // 공용 스크롤(휠 · 스크롤바 끌기)이 먼저.
        let total = self.total_h();
        let (_, ny, consumed) = self.bars.on_event(
            ev,
            self.bounds,
            self.bounds.w,
            total.max(self.bounds.h),
            0,
            self.scroll,
            self.scale,
        );
        if ny != self.scroll {
            self.scroll = ny;
            self.relayout();
        }
        if consumed {
            return true;
        }
        let at = match *ev {
            InputEvent::MouseDown { x, y, .. }
            | InputEvent::RightDown { x, y }
            | InputEvent::MouseUp { x, y } => Some(Point { x, y }),
            _ => None,
        };
        if let Some(p) = at {
            let Some(i) = self.pane_at(p) else {
                return false;
            };
            if !matches!(ev, InputEvent::MouseUp { .. }) && i != self.shown {
                // 다른 서버의 트리를 눌렀다 — 선택·키보드 대상은 전체에 하나.
                let f = self.focused;
                self.panes[self.shown].ex.set_focused(false);
                self.panes[self.shown].ex.clear_selection();
                self.shown = i;
                self.panes[i].ex.set_focused(f);
            }
            return self.panes[i].ex.on_event(ev);
        }
        match ev {
            InputEvent::MouseMove { x, y } => {
                self.cursor = Point { x: *x, y: *y };
                let mut any = false;
                for i in self.laid() {
                    any |= self.panes[i].ex.on_event(ev);
                }
                any
            }
            // (휠은 위의 공용 스크롤이 처리한다.)
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => false,
            _ => {
                let r = self.cur_mut().on_event(ev);
                // 키로 선택을 옮겼으면 공용 스크롤이 따라간다(펼침·접힘으로 높이도 바뀔 수 있다).
                self.relayout();
                self.reveal_selection();
                r
            }
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        // 트리가 펼쳐지거나 접히면 칸 높이가 달라진다 → 그릴 때마다 다시 놓는다(칸 수만큼의 덧셈).
        self.relayout();
        dc.fill_rect(self.bounds, th.panel_bg);
        let laid = self.laid();
        for &i in &laid {
            self.panes[i].ex.paint(dc, th);
        }
        let b = self.bounds;
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        self.bars.paint(
            dc,
            th,
            b,
            b.w,
            self.total_h().max(b.h),
            0,
            self.scroll,
            self.scale,
        );
        // 팝업 층 — 모든 칸을 그린 뒤.
        for &i in &laid {
            self.panes[i].ex.paint_menu(dc, th);
        }
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
