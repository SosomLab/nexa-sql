//! 프로젝트 탐색기 패널(docs/67 §4 · 사용자 09-22) — 활동 막대 `view.project`.
//!
//! ```text
//! [▣ my-project                 ]   ← 헤더(프로젝트 이름 · 없으면 "No project")
//! [🔍 filter                    ]   ← 이름 필터(Enter 없이 즉시 · 폴더 깊이와 무관)
//!  ▾ sql                             ← 루트 폴더(프로젝트 파일 기준) · 지연 열거(펼칠 때만 `nexa_fs::list_opts`)
//!      a.sql
//!    ▸ archive
//!  ▸ D:/other
//! ```
//! - 클릭 = **미리보기 탭**(Sublime 차용 · 편집기 `open_preview` · 한 개만 재사용) · 더블클릭·Enter = 정식 탭 · Space = 미리보기.
//! - 프로젝트가 없으면 전 기능은 쓸 수 없지만 **클릭은 된다**: 안내 + "Open Project…"/"New Project…" 링크 행이 명령을 낸다.
//! - 필터가 비어 있지 않으면 펼치지 않은 폴더도 상한(`project.scan_max`)까지 열거해 이름을 찾는다 · 일치 항목과 그 조상만 보인다.
//!
//! 호스트가 하는 것: 매 틱 `tick` · 클릭 → [`ProjectPanel::take_open`] · 링크 → [`ProjectPanel::take_command`].

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 행을 열라는 요청(1회성).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenReq {
    pub path: PathBuf,
    /// 정식 탭으로(더블클릭 · Enter) · 아니면 미리보기.
    pub permanent: bool,
}

struct Node {
    path: PathBuf,
    name: String,
    is_dir: bool,
    depth: usize,
    parent: Option<usize>,
    children: Vec<usize>,
    expanded: bool,
    /// 자식을 열거했는가(폴더).
    loaded: bool,
    /// 열거 실패(없는 폴더 · 권한).
    error: bool,
}

pub(crate) struct ProjectPanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    filter: TextBox,
    /// 프로젝트 이름(없으면 None = 빈 상태).
    name: Option<String>,
    nodes: Vec<Node>,
    roots: Vec<usize>,
    /// 보이는 행 = 노드 인덱스.
    rows: Vec<usize>,
    scroll_y: i32,
    bars: ScrollBars,
    sel: Option<usize>,
    hover: Option<(usize, Instant)>,
    list_rect: Rect,
    header_rect: Rect,
    /// 빈 상태의 링크 행(명령 id · 영역).
    links: Vec<(&'static str, Rect)>,
    row_h: i32,
    open: Option<OpenReq>,
    command: Option<&'static str>,
    /// 최근 프로젝트가 있는가(빈 상태에 "프로젝트 전환…" 링크를 보일지 · 09-22).
    has_recent: bool,
    last_click: Option<(usize, Instant)>,
    dblclick_ms: u128,
    tooltip_ms: u128,
    clamp_w: i32,
    show_hidden: bool,
    show_dot: bool,
    /// 필터 때 열거할 최대 항목 수(설정 `project.scan_max`).
    scan_max: usize,
    /// 필터 열거가 상한에 걸렸다(안내 한 줄).
    scan_capped: bool,
    filter_text: String,
}

const ROW_H: f32 = 22.0;
const INPUT_H: f32 = 25.0;
const PAD: f32 = 8.0;
const INDENT: f32 = 14.0;

impl ProjectPanel {
    pub(crate) fn new() -> Self {
        let mut filter = TextBox::new(t(Msg::PhProjectFilter));
        filter.set_focus_ring(false);
        ProjectPanel {
            visible: false,
            bounds: Rect::default(),
            scale: 1.0,
            filter,
            name: None,
            nodes: Vec::new(),
            roots: Vec::new(),
            rows: Vec::new(),
            scroll_y: 0,
            bars: ScrollBars::new(),
            sel: None,
            hover: None,
            list_rect: Rect::default(),
            header_rect: Rect::default(),
            links: Vec::new(),
            row_h: 22,
            open: None,
            command: None,
            has_recent: false,
            last_click: None,
            dblclick_ms: 400,
            tooltip_ms: 600,
            clamp_w: i32::MAX / 2,
            show_hidden: false,
            show_dot: true,
            scan_max: 5000,
            scan_capped: false,
            filter_text: String::new(),
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.set_focused(false);
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }

    pub(crate) fn set_tooltip_delay(&mut self, ms: u128) {
        self.tooltip_ms = ms;
    }

    pub(crate) fn set_dblclick_ms(&mut self, ms: u128) {
        self.dblclick_ms = ms;
    }

    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
    }

    pub(crate) fn set_list_opts(&mut self, show_hidden: bool, show_dot: bool, scan_max: usize) {
        self.show_hidden = show_hidden;
        self.show_dot = show_dot;
        self.scan_max = scan_max.max(100);
    }

    #[cfg(test)]
    pub(crate) fn has_project(&self) -> bool {
        self.name.is_some()
    }

    /// 프로젝트를 바꾼다(없으면 `None`) — 트리를 다시 만든다(루트만 · 펼침은 사용자가).
    pub(crate) fn set_project(&mut self, name: Option<String>, folders: &[PathBuf]) {
        self.name = name;
        self.nodes.clear();
        self.roots.clear();
        self.sel = None;
        self.scroll_y = 0;
        self.hover = None;
        for f in folders {
            let label = f
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| nexa_fs::path::display(f));
            let i = self.nodes.len();
            self.nodes.push(Node {
                path: f.clone(),
                name: label,
                is_dir: true,
                depth: 0,
                parent: None,
                children: Vec::new(),
                expanded: false,
                loaded: false,
                error: false,
            });
            self.roots.push(i);
        }
        // 루트가 하나면 바로 펼친다(한 번 더 누르지 않게).
        if self.roots.len() == 1 {
            self.expand(self.roots[0]);
        }
        self.rebuild_rows();
    }

    /// 디스크가 바뀌었을 수 있다 — 펼친 폴더를 다시 열거(호스트: 파일 저장·새 파일 뒤).
    pub(crate) fn refresh(&mut self) {
        let expanded: Vec<PathBuf> = self
            .nodes
            .iter()
            .filter(|n| n.is_dir && n.expanded)
            .map(|n| n.path.clone())
            .collect();
        let name = self.name.clone();
        let folders: Vec<PathBuf> = self
            .roots
            .iter()
            .map(|&r| self.nodes[r].path.clone())
            .collect();
        let sel_path = self
            .sel
            .and_then(|r| self.rows.get(r))
            .map(|&n| self.nodes[n].path.clone());
        let scroll = self.scroll_y;
        self.set_project(name, &folders);
        for p in expanded {
            if let Some(i) = self.nodes.iter().position(|n| n.path == p) {
                self.expand(i);
            }
        }
        self.rebuild_rows();
        if let Some(p) = sel_path {
            self.sel = self.rows.iter().position(|&n| self.nodes[n].path == p);
        }
        self.scroll_y = scroll;
        self.clamp_scroll();
    }

    /// 활성 탭의 파일을 트리에서 골라 보인다(있으면 · 조상을 펼친다). 호스트 배선 = T-165 P4 잔여(`project.reveal_active`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn reveal(&mut self, path: &Path) -> bool {
        // 조상 루트를 찾아 경로를 따라 펼친다.
        let root = self
            .roots
            .iter()
            .copied()
            .find(|&r| path.starts_with(&self.nodes[r].path));
        let Some(mut cur) = root else {
            return false;
        };
        loop {
            if self.nodes[cur].path == path {
                break;
            }
            self.expand(cur);
            let next = self.nodes[cur]
                .children
                .iter()
                .copied()
                .find(|&c| path.starts_with(&self.nodes[c].path));
            match next {
                Some(c) => cur = c,
                None => return false,
            }
        }
        self.rebuild_rows();
        if let Some(r) = self.rows.iter().position(|&n| n == cur) {
            self.sel = Some(r);
            self.ensure_visible(r);
            return true;
        }
        false
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        if !on {
            self.filter.set_focused(false);
        } else if self.sel.is_none() && self.name.is_some() {
            self.filter.set_focused(true);
        }
    }

    pub(crate) fn focus_filter(&mut self) {
        if self.name.is_some() {
            self.filter.set_focused(true);
        }
    }

    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.filter.is_focused() {
            Some(&mut self.filter)
        } else {
            None
        }
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let px = |v: f32| (v * scale).round() as i32;
        let pad = px(PAD);
        let ih = px(INPUT_H);
        self.row_h = px(ROW_H);
        self.header_rect = Rect::new(b.x, b.y + px(4.0), b.w, self.row_h);
        let y1 = self.header_rect.bottom() + px(4.0);
        let mut inv = Invalidations::default();
        self.filter.set_scale(scale);
        self.filter.set_bounds(
            Rect::new(b.x + pad, y1, (b.w - pad * 2).max(px(80.0)), ih),
            &mut inv,
        );
        let list_top = y1 + ih + px(4.0);
        self.list_rect = Rect::new(b.x, list_top, b.w, (b.bottom() - list_top).max(0));
        self.clamp_scroll();
    }

    pub(crate) fn take_open(&mut self) -> Option<OpenReq> {
        self.open.take()
    }

    pub(crate) fn set_has_recent(&mut self, on: bool) {
        self.has_recent = on;
    }

    pub(crate) fn take_command(&mut self) -> Option<&'static str> {
        self.command.take()
    }

    // ───────────────────────── 트리 ──────────────────

    fn list_children(&mut self, i: usize) {
        if self.nodes[i].loaded || !self.nodes[i].is_dir {
            return;
        }
        self.nodes[i].loaded = true;
        let path = self.nodes[i].path.clone();
        let depth = self.nodes[i].depth + 1;
        match nexa_fs::list_opts(&path, self.show_hidden, self.show_dot) {
            Ok(mut entries) => {
                // 폴더 먼저 · 이름순(대소문자 무시).
                entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                let mut kids = Vec::with_capacity(entries.len());
                for e in entries {
                    let k = self.nodes.len();
                    self.nodes.push(Node {
                        path: e.path,
                        name: e.name,
                        is_dir: e.is_dir,
                        depth,
                        parent: Some(i),
                        children: Vec::new(),
                        expanded: false,
                        loaded: false,
                        error: false,
                    });
                    kids.push(k);
                }
                self.nodes[i].children = kids;
            }
            Err(_) => self.nodes[i].error = true,
        }
    }

    fn expand(&mut self, i: usize) {
        if !self.nodes[i].is_dir {
            return;
        }
        self.list_children(i);
        self.nodes[i].expanded = true;
    }

    fn toggle(&mut self, i: usize) {
        if !self.nodes[i].is_dir {
            return;
        }
        if self.nodes[i].expanded {
            self.nodes[i].expanded = false;
        } else {
            self.expand(i);
        }
        self.rebuild_rows();
    }

    /// 필터용 열거 — 아직 열거하지 않은 폴더를 너비 우선으로 상한까지.
    fn scan_for_filter(&mut self) {
        self.scan_capped = false;
        let mut queue: Vec<usize> = self.roots.clone();
        let mut qi = 0;
        while qi < queue.len() {
            let i = queue[qi];
            qi += 1;
            if self.nodes.len() >= self.scan_max {
                self.scan_capped = true;
                break;
            }
            self.list_children(i);
            for &c in &self.nodes[i].children {
                if self.nodes[c].is_dir {
                    queue.push(c);
                }
            }
        }
    }

    fn matches(&self, i: usize, needle: &str) -> bool {
        self.nodes[i].name.to_lowercase().contains(needle)
    }

    /// 일치하는 자손이 있는가(필터).
    fn has_match(&self, i: usize, needle: &str, memo: &mut Vec<Option<bool>>) -> bool {
        if let Some(v) = memo[i] {
            return v;
        }
        let mut v = self.matches(i, needle);
        if !v {
            for &c in &self.nodes[i].children {
                if self.has_match(c, needle, memo) {
                    v = true;
                    break;
                }
            }
        }
        memo[i] = Some(v);
        v
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        let needle = self.filter_text.trim().to_lowercase();
        if needle.is_empty() {
            let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
            while let Some(i) = stack.pop() {
                self.rows.push(i);
                if self.nodes[i].expanded {
                    for &c in self.nodes[i].children.iter().rev() {
                        stack.push(c);
                    }
                }
            }
        } else {
            let mut memo = vec![None; self.nodes.len()];
            let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
            while let Some(i) = stack.pop() {
                if !self.has_match(i, &needle, &mut memo) {
                    continue;
                }
                self.rows.push(i);
                for &c in self.nodes[i].children.iter().rev() {
                    stack.push(c);
                }
            }
        }
        if let Some(s) = self.sel {
            if s >= self.rows.len() {
                self.sel = None;
            }
        }
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    fn content_h(&self) -> i32 {
        let extra = if self.scan_capped { 1 } else { 0 };
        (self.rows.len() as i32 + extra) * self.row_h
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if !self.visible {
            return false;
        }
        self.filter.tick(now_ms) | self.bars.tick(now_ms)
    }

    pub(crate) fn animating(&self) -> bool {
        self.visible && self.filter.is_animating()
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) || self.row_h <= 0 {
            return None;
        }
        let i = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h) as usize;
        (i < self.rows.len()).then_some(i)
    }

    /// 행의 셰브론 x 범위.
    fn chevron_x(&self, node: usize) -> (i32, i32) {
        let px = |v: f32| (v * self.scale).round() as i32;
        let x0 = self.list_rect.x + px(PAD) + self.nodes[node].depth as i32 * px(INDENT);
        (x0, x0 + px(16.0))
    }

    fn activate_row(&mut self, r: usize, permanent: bool) {
        let Some(&n) = self.rows.get(r) else { return };
        if self.nodes[n].is_dir {
            self.toggle(n);
        } else {
            self.open = Some(OpenReq {
                path: self.nodes[n].path.clone(),
                permanent,
            });
        }
    }

    fn ensure_visible(&mut self, r: usize) {
        let top = r as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + self.list_rect.h {
            self.scroll_y = top + self.row_h - self.list_rect.h;
        }
        self.clamp_scroll();
    }

    /// 이벤트(마우스 = 패널 안 · 키 = 포커스일 때) — 다시 그려야 하면 `true`.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        if self.name.is_some() {
            let (_, ny, consumed) = self.bars.on_event(
                ev,
                self.list_rect,
                self.list_rect.w,
                self.content_h().max(self.list_rect.h),
                0,
                self.scroll_y,
                self.scale,
            );
            if ny != self.scroll_y {
                self.scroll_y = ny;
                return true;
            }
            if consumed {
                return true;
            }
        }
        match *ev {
            InputEvent::Key { key, .. } => {
                if self.filter.is_focused() {
                    if key == CtlKey::Down && !self.rows.is_empty() {
                        self.filter.set_focused(false);
                        self.sel = Some(0);
                        self.ensure_visible(0);
                        return true;
                    }
                } else if let Some(cur) = self.sel {
                    let n = self.rows.len();
                    match key {
                        CtlKey::Down | CtlKey::Up if n > 0 => {
                            let next = if key == CtlKey::Down {
                                (cur + 1).min(n - 1)
                            } else {
                                cur.saturating_sub(1)
                            };
                            self.sel = Some(next);
                            self.ensure_visible(next);
                            return true;
                        }
                        CtlKey::Right => {
                            if let Some(&node) = self.rows.get(cur) {
                                if self.nodes[node].is_dir && !self.nodes[node].expanded {
                                    self.expand(node);
                                    self.rebuild_rows();
                                }
                            }
                            return true;
                        }
                        CtlKey::Left => {
                            if let Some(&node) = self.rows.get(cur) {
                                if self.nodes[node].is_dir && self.nodes[node].expanded {
                                    self.nodes[node].expanded = false;
                                    self.rebuild_rows();
                                } else if let Some(p) = self.nodes[node].parent {
                                    if let Some(r) = self.rows.iter().position(|&x| x == p) {
                                        self.sel = Some(r);
                                        self.ensure_visible(r);
                                    }
                                }
                            }
                            return true;
                        }
                        CtlKey::Enter => {
                            self.activate_row(cur, true);
                            return true;
                        }
                        CtlKey::Space => {
                            self.activate_row(cur, false);
                            return true;
                        }
                        CtlKey::Up => {}
                        _ => {}
                    }
                }
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if self.name.is_none() {
                    // 빈 상태: 링크 행만 반응.
                    if let Some((id, _)) = self.links.iter().find(|(_, r)| r.contains(p)) {
                        self.command = Some(id);
                        return true;
                    }
                    return false;
                }
                // 헤더(프로젝트 이름) 클릭 = 프로젝트 전환(최근 목록 팔레트 · 사용자 09-22).
                if self.header_rect.contains(p) {
                    self.command = Some("project.switch");
                    return true;
                }
                let in_f = self.filter.bounds().contains(p);
                self.filter.set_focused(in_f);
                if let Some(r) = self.row_at(p) {
                    self.sel = Some(r);
                    let node = self.rows[r];
                    let (cx0, cx1) = self.chevron_x(node);
                    let now = Instant::now();
                    let dbl = matches!(self.last_click, Some((j, t0)) if j == r && now.duration_since(t0).as_millis() < self.dblclick_ms);
                    self.last_click = Some((r, now));
                    if self.nodes[node].is_dir {
                        if dbl || (x >= cx0 && x < cx1) {
                            self.last_click = None;
                            self.toggle(node);
                        }
                    } else if dbl {
                        self.last_click = None;
                        self.activate_row(r, true);
                    } else {
                        self.activate_row(r, false);
                    }
                    return true;
                }
            }
            InputEvent::MouseMove { x, y } => {
                let h = self.row_at(Point { x, y });
                match (h, self.hover) {
                    (Some(a), Some((b, _))) if a == b => {}
                    (Some(a), _) => self.hover = Some((a, Instant::now())),
                    (None, _) => self.hover = None,
                }
            }
            _ => {}
        }
        if self.name.is_some() {
            self.filter.on_event(ev, &mut inv);
            if let Some(text) = self.filter.take_changed() {
                self.filter_text = text;
                if !self.filter_text.trim().is_empty() {
                    self.scan_for_filter();
                } else {
                    self.scan_capped = false;
                }
                self.sel = None;
                self.scroll_y = 0;
                self.rebuild_rows();
                return true;
            }
        }
        true
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let s = self.scale;
        let px = |v: f32| (v * s).round() as i32;
        let b = self.bounds;
        let pad = px(PAD);
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        // 헤더.
        dc.select_font(FontSlot::Base, true);
        let hr = self.header_rect;
        let hy = dc.text_center_y(hr.y, hr.h);
        match &self.name {
            Some(n) => dc.text(hr.x + pad, hy, hr, n, th.text),
            None => dc.text(hr.x + pad, hy, hr, t(Msg::ProjNone), th.text_dim),
        }
        dc.select_font(FontSlot::Base, false);
        // 필터 상자(프로젝트 없으면 흐리게).
        let fb = self.filter.bounds();
        let r = px(6.0);
        dc.fill_round_rect(fb, r, th.field_bg);
        dc.stroke_round_rect(fb, r, th.border, 1.0);
        if self.name.is_some() {
            self.filter.paint(dc, th);
            if self.filter.is_focused() {
                dc.stroke_round_rect(fb, r, th.accent, 1.0);
            }
        } else {
            let ty = dc.text_center_y(fb.y, fb.h);
            dc.text(fb.x + px(6.0), ty, fb, t(Msg::PhProjectFilter), th.text_dim);
        }
        let lr = self.list_rect;
        let rh = self.row_h.max(1);
        self.links.clear();
        if self.name.is_none() {
            // 빈 상태: 안내 두 줄 + 링크 행 둘(클릭 = 명령).
            let mut y = lr.y + px(6.0);
            dc.select_font(FontSlot::Status, false);
            for line in t(Msg::ProjEmptyHint).split('\n') {
                let ty = dc.text_center_y(y, rh);
                dc.text(
                    lr.x + pad,
                    ty,
                    Rect::new(lr.x, y, lr.w, rh),
                    line,
                    th.text_dim,
                );
                y += rh;
            }
            dc.select_font(FontSlot::Base, false);
            y += px(4.0);
            let mut rows: Vec<(&'static str, Msg)> = vec![
                ("project.new", Msg::MnProjectNew),
                ("project.open", Msg::MnProjectOpen),
            ];
            if self.has_recent {
                rows.push(("project.switch", Msg::MnProjectSwitch));
            }
            for (id, m) in rows {
                let rr = Rect::new(lr.x, y, lr.w, rh);
                let ty = dc.text_center_y(y, rh);
                dc.text(lr.x + pad, ty, rr, t(m), th.accent);
                self.links.push((id, rr));
                y += rh;
            }
            return;
        }
        let first = (self.scroll_y / rh) as usize;
        let mut y = lr.y - self.scroll_y % rh;
        let hover_row = self.hover.map(|(r, _)| r);
        let chev_w = dc.text_width("▾");
        for r in first..self.rows.len() {
            if y >= lr.bottom() {
                break;
            }
            let node = self.rows[r];
            let n = &self.nodes[node];
            let row_rect = Rect::new(lr.x, y, lr.w, rh);
            if self.sel == Some(r) {
                dc.fill_rect(row_rect, th.sel_bg);
            } else if hover_row == Some(r) {
                dc.fill_rect_alpha(row_rect, th.text, 0.06);
            }
            let ty = dc.text_center_y(y, rh);
            let (cx0, _) = self.chevron_x(node);
            let mut tx = cx0;
            if n.is_dir {
                let chev = if n.expanded { "▾" } else { "▸" };
                dc.text(cx0, ty, row_rect, chev, th.text_dim);
            }
            tx += chev_w + px(6.0);
            let color = if n.error { th.danger } else { th.text };
            let clip = Rect::new(tx, y, (lr.right() - tx).max(0), rh);
            dc.text(tx, ty, clip, &n.name, color);
            y += rh;
        }
        if self.scan_capped && y < lr.bottom() {
            dc.select_font(FontSlot::Status, false);
            let ty = dc.text_center_y(y, rh);
            dc.text(
                lr.x + pad,
                ty,
                Rect::new(lr.x, y, lr.w, rh),
                &tf(Msg::ProjScanCapped, &[&self.scan_max.to_string()]),
                th.warn,
            );
            dc.select_font(FontSlot::Base, false);
        }
        if self.rows.is_empty() && !self.filter_text.trim().is_empty() {
            let ty = dc.text_center_y(lr.y + px(4.0), rh);
            dc.text(lr.x + pad, ty, lr, t(Msg::ProjNoMatch), th.text_dim);
        } else if self.rows.is_empty() {
            let ty = dc.text_center_y(lr.y + px(4.0), rh);
            dc.text(lr.x + pad, ty, lr, t(Msg::ProjNoFolders), th.text_dim);
        }
        self.bars.paint(
            dc,
            th,
            lr,
            lr.w,
            self.content_h().max(lr.h),
            0,
            self.scroll_y,
            s,
        );
        self.filter.paint_popup(dc, th);
    }

    /// 머문 행의 전체 경로 툴팁(팝업 층).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let Some((r, t0)) = self.hover else { return };
        if t0.elapsed().as_millis() < self.tooltip_ms {
            return;
        }
        let Some(&node) = self.rows.get(r) else {
            return;
        };
        let text = nexa_fs::path::display(&self.nodes[node].path);
        let y = self.list_rect.y - self.scroll_y % self.row_h.max(1)
            + (r as i32 - self.scroll_y / self.row_h.max(1)) * self.row_h;
        let anchor = Rect::new(
            self.list_rect.x + (8.0 * self.scale).round() as i32,
            y + self.row_h,
            0,
            0,
        );
        nexa_ctl::draw::draw_tooltip_in(dc, th, anchor, (0, self.clamp_w), &text, self.scale);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsql-ppanel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/one.sql"), "x").unwrap();
        std::fs::write(dir.join("a/b/two.sql"), "y").unwrap();
        std::fs::write(dir.join("top.txt"), "z").unwrap();
        dir
    }

    #[test]
    fn single_root_expands_and_filter_finds_deep_files() {
        let dir = fixture();
        let mut p = ProjectPanel::new();
        p.set_project(Some("t".into()), std::slice::from_ref(&dir));
        // 루트 하나 = 자동 펼침 · 폴더 먼저.
        let names: Vec<&str> = p.rows.iter().map(|&n| p.nodes[n].name.as_str()).collect();
        assert_eq!(
            names,
            vec![dir.file_name().unwrap().to_str().unwrap(), "a", "top.txt"]
        );
        // 필터 = 깊이와 무관하게 찾고 조상만 보인다.
        p.filter_text = "two".into();
        p.scan_for_filter();
        p.rebuild_rows();
        let names: Vec<&str> = p.rows.iter().map(|&n| p.nodes[n].name.as_str()).collect();
        assert_eq!(names[1..], ["a", "b", "two.sql"]);
        // 필터를 지우면 펼침 상태 그대로(a는 열거만 됐지 펼치지 않았다).
        p.filter_text.clear();
        p.rebuild_rows();
        assert_eq!(p.rows.len(), 3);
        // reveal = 조상을 펼치고 선택.
        assert!(p.reveal(&dir.join("a/b/two.sql")));
        let sel = p.sel.unwrap();
        assert_eq!(p.nodes[p.rows[sel]].name, "two.sql");
        assert!(!p.reveal(Path::new("/nowhere/x.sql")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_state_has_links_and_no_rows() {
        let mut p = ProjectPanel::new();
        p.set_project(None, &[]);
        assert!(!p.has_project());
        assert!(p.rows.is_empty());
        // 프로젝트 없는 상태에서 행 활성화는 아무것도 내지 않는다.
        p.activate_row(0, true);
        assert!(p.take_open().is_none());
    }
}
