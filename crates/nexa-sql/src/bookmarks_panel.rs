//! 북마크 관리자 패널([docs/69 §6-3~6-9](../../../docs/69-bookmarks.md) · T-167 B4a~B4c · 사용자 09-22) — 활동 막대 `view.bookmarks`.
//!
//! 뼈대 = 프로젝트 탐색기 패널과 같다(30 §2): 머리(제목 + 개수) · 필터 상자 · 행 목록(가상 스크롤).
//! 묶음 = **그룹 → 문서 → 항목**(D-172 · 그룹/문서 행은 접기/펼치기) · 정렬 = 줄 번호 · 필터 = 이름·줄 원문·문서 이름(공백 AND) ·
//! 무효 = 회색 + 사유 · 꺼진 그룹 = 흐리게. 클릭/Enter = 이동 · Ctrl+Enter = 문서 전부 멀티커서 · Delete = 제거(5초 되돌리기 토스트는
//! 호스트) · 제자리 이름 편집(항목 · 그룹) · ↑↓←→ Home End Space · Esc = 필터 비우기 → 편집기 · **우클릭 메뉴는 자리마다 다르다**(69 §6-6:
//! 항목 · 문서 · 그룹 · 빈 곳) — `ContextMenu` 하나(팝업 배치 규칙) · 바깥 클릭 = 닫고 통과.
//! 미리보기 탭·드래그 이동·메모 편집은 다음 단계(69 §11).

use crate::filterbar::{FilterBar, FilterEvent, GAP_Y};
use nexa_ctl::controls::{ContextMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_bookmarks::{Bookmark, DocKey, Group, Reason, State, Store};
use nsql_i18n::{t, tf, Msg};
use std::time::Instant;

/// 패널이 낸 요청.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BmAction {
    /// 북마크로 이동 · `permanent` = 정식 탭으로(더블클릭 · Enter · 메뉴 Open) · false = 미리보기 탭(한 번 클릭 · 프로젝트 탐색기와 같은 규칙 · 사용자 09-23).
    Goto(u64, bool),
    Remove(u64),
    Rename(u64, String),
    SelectAllInDoc(DocKey),
    Mnemonic(u64, Option<u8>),
    MoveGroup(u64, u32),
    RemoveDoc(DocKey),
    NewGroup,
    RenameGroup(u32, String),
    ToggleGroup(u32),
    DefaultGroup(u32),
    /// 그룹 삭제 — `keep_items` = 항목을 기본 그룹으로.
    DeleteGroup(u32, bool),
    RemoveInvalid,
    OpenSettings,
}

#[derive(Clone, Debug)]
enum Row {
    Group {
        id: u32,
        name: String,
        enabled: bool,
        expanded: bool,
        count: usize,
    },
    Doc {
        key: DocKey,
        name: String,
        count: usize,
        expanded: bool,
        enabled: bool,
    },
    Item {
        id: u64,
        enabled: bool,
    },
}

#[derive(Clone, Copy, Debug)]
enum RenameKind {
    Item(u64),
    Group(u32),
}

pub(crate) struct BookmarksPanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    filter: FilterBar,
    filter_text: String,
    rows: Vec<Row>,
    collapsed_groups: Vec<u32>,
    collapsed_docs: Vec<(u32, String)>,
    /// 저장소 스냅샷(패널은 저장소를 소유하지 않는다 — `sync`로 받는다).
    items: Vec<Bookmark>,
    groups: Vec<Group>,
    total_live: usize,
    scroll_y: i32,
    bars: ScrollBars,
    sel: Option<usize>,
    /// 선택 없음일 때의 캐럿 행(빈 곳 클릭 · 테두리만 · 키 이동 시작점 · 사용자 09-23).
    caret: Option<usize>,
    hover: Option<(usize, Instant)>,
    /// 더블클릭 감지(행 · 첫 클릭 시각) · 간격 = `ui.dblclick_ms`(프로젝트 탐색기와 같은 부품 규칙).
    last_click: Option<(usize, Instant)>,
    dblclick_ms: u128,
    list_rect: Rect,
    header_rect: Rect,
    row_h: i32,
    actions: Vec<BmAction>,
    /// 제자리 이름 편집(행 · 상자 · 대상).
    rename: Option<(usize, TextBox, RenameKind)>,
    menu: ContextMenu,
    menu_row: Option<usize>,
    clamp_w: i32,
    focused: bool,
}

const ROW_H: f32 = 22.0;
const INPUT_H: f32 = 25.0;
const PAD: f32 = 8.0;
const INDENT: f32 = 14.0;

impl BookmarksPanel {
    pub(crate) fn new() -> Self {
        let filter = FilterBar::new(t(Msg::PhBookmarkFilter), &[]);
        BookmarksPanel {
            visible: false,
            bounds: Rect::default(),
            scale: 1.0,
            filter,
            filter_text: String::new(),
            rows: Vec::new(),
            collapsed_groups: Vec::new(),
            collapsed_docs: Vec::new(),
            items: Vec::new(),
            groups: Vec::new(),
            total_live: 0,
            scroll_y: 0,
            bars: ScrollBars::new(),
            sel: None,
            caret: None,
            hover: None,
            last_click: None,
            dblclick_ms: 400,
            list_rect: Rect::default(),
            header_rect: Rect::default(),
            row_h: 22,
            actions: Vec::new(),
            rename: None,
            menu: ContextMenu::new(),
            menu_row: None,
            clamp_w: i32::MAX / 2,
            focused: false,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }
    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.rename = None;
            self.menu.close();
        }
    }
    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }
    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
    }
    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if !on {
            self.filter.set_focused(false);
            self.rename = None;
        }
    }
    pub(crate) fn focus_filter(&mut self) {
        self.filter.set_focused(true);
    }
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if let Some((_, tb, _)) = self.rename.as_mut() {
            return Some(tb);
        }
        if self.filter.is_focused() {
            Some(self.filter.tb_mut())
        } else {
            None
        }
    }
    pub(crate) fn take_action(&mut self) -> Option<BmAction> {
        if self.actions.is_empty() {
            None
        } else {
            Some(self.actions.remove(0))
        }
    }
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }
    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
    }
    /// 메뉴(팝업 층 · 창의 맨 마지막에).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.visible {
            self.filter.paint_popup(dc, th);
        }
        if self.visible && self.menu.is_open() {
            self.menu.paint(dc, th);
        }
    }

    /// 선택한 항목/그룹의 이름을 제자리에서 편집(명령 `bookmark.label` · 메뉴) — 상자를 열었으면 true.
    pub(crate) fn begin_rename(&mut self) -> bool {
        let Some(r) = self.sel else { return false };
        let (kind, cur) = match self.rows.get(r) {
            Some(Row::Item { id, .. }) => (
                RenameKind::Item(*id),
                self.item(*id)
                    .and_then(|b| b.label.clone())
                    .unwrap_or_default(),
            ),
            Some(Row::Group { id, name, .. }) => (RenameKind::Group(*id), name.clone()),
            _ => return false,
        };
        let mut inv = Invalidations::default();
        let mut tb = TextBox::new("");
        tb.set_scale(self.scale);
        tb.set_text(&cur);
        tb.set_focused(true);
        let y = self.list_rect.y + r as i32 * self.row_h - self.scroll_y;
        let ind = self.px(INDENT);
        tb.set_bounds(
            Rect::new(
                self.list_rect.x + ind,
                y,
                self.list_rect.w - ind - 2,
                self.row_h,
            ),
            &mut inv,
        );
        tb.on_event(&InputEvent::SelectAll, &mut inv);
        self.rename = Some((r, tb, kind));
        true
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let px = |v: f32| (v * scale).round() as i32;
        let pad = px(PAD);
        let ih = px(INPUT_H);
        self.row_h = px(ROW_H);
        self.header_rect = Rect::new(b.x, b.y + px(4.0), b.w, self.row_h);
        let y1 = self.header_rect.bottom() + px(GAP_Y);
        self.filter.set_bounds(
            Rect::new(b.x + pad, y1, (b.w - pad * 2).max(px(80.0)), ih),
            scale,
        );
        let list_top = y1 + ih + px(GAP_Y);
        self.list_rect = Rect::new(b.x, list_top, b.w, (b.bottom() - list_top).max(0));
        self.menu.set_scale(scale);
        self.clamp_scroll();
    }

    /// 저장소 스냅샷 → 행.
    pub(crate) fn sync(&mut self, store: &Store) {
        self.items = store.items.clone();
        self.groups = store.groups.clone();
        self.total_live = store.counts().0;
        self.rebuild();
    }

    fn group_label(&self, g: &Group) -> String {
        if g.name.trim().is_empty() {
            t(Msg::BmGroupDefaultName).to_string()
        } else {
            g.name.clone()
        }
    }

    fn rebuild(&mut self) {
        let sel_id = self.sel.and_then(|r| match self.rows.get(r) {
            Some(Row::Item { id, .. }) => Some(*id),
            _ => None,
        });
        let filtering = !self.filter.is_empty();
        let mut rows: Vec<Row> = Vec::new();
        let groups = self.groups.clone();
        for g in &groups {
            let mut docs: Vec<(DocKey, String, Vec<Bookmark>)> = Vec::new();
            for b in self.items.iter().filter(|b| b.group == g.id) {
                let name = b.doc.short_name();
                let hay = format!("{} {} {}", b.display(), b.anchor.text, name);
                if filtering && !self.filter.matches(&hay) {
                    continue;
                }
                match docs.iter_mut().find(|(k, _, _)| *k == b.doc) {
                    Some((_, _, v)) => v.push(b.clone()),
                    None => docs.push((b.doc.clone(), name, vec![b.clone()])),
                }
            }
            docs.sort_by_key(|d| d.1.to_lowercase());
            let count: usize = docs.iter().map(|d| d.2.len()).sum();
            if count == 0 && filtering {
                continue;
            }
            let g_expanded = filtering || !self.collapsed_groups.contains(&g.id);
            rows.push(Row::Group {
                id: g.id,
                name: self.group_label(g),
                enabled: g.enabled,
                expanded: g_expanded,
                count,
            });
            if !g_expanded {
                continue;
            }
            for (key, name, mut v) in docs {
                v.sort_by_key(|b| b.anchor.line);
                let d_expanded = filtering
                    || !self
                        .collapsed_docs
                        .iter()
                        .any(|(gid, n)| *gid == g.id && *n == name);
                rows.push(Row::Doc {
                    key,
                    name,
                    count: v.len(),
                    expanded: d_expanded,
                    enabled: g.enabled,
                });
                if d_expanded {
                    for b in v {
                        rows.push(Row::Item {
                            id: b.id,
                            enabled: g.enabled,
                        });
                    }
                }
            }
        }
        self.rows = rows;
        self.sel = sel_id.and_then(|id| {
            self.rows
                .iter()
                .position(|r| matches!(r, Row::Item { id: x, .. } if *x == id))
        });
        self.clamp_scroll();
    }

    fn item(&self, id: u64) -> Option<&Bookmark> {
        self.items.iter().find(|b| b.id == id)
    }

    fn content_h(&self) -> i32 {
        self.rows.len() as i32 * self.row_h
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) {
            return None;
        }
        let r = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h.max(1)) as usize;
        (r < self.rows.len()).then_some(r)
    }

    fn reveal(&mut self, r: usize) {
        let top = r as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + self.list_rect.h {
            self.scroll_y = top + self.row_h - self.list_rect.h;
        }
        self.clamp_scroll();
    }

    /// 접기/펼치기 토글(그룹·문서).
    fn toggle_fold(&mut self, r: usize) {
        match self.rows.get(r).cloned() {
            Some(Row::Group { id, .. }) => {
                if let Some(i) = self.collapsed_groups.iter().position(|g| *g == id) {
                    self.collapsed_groups.remove(i);
                } else {
                    self.collapsed_groups.push(id);
                }
                self.rebuild();
            }
            Some(Row::Doc { name, .. }) => {
                let gid = self.group_of_row(r);
                if let Some(i) = self
                    .collapsed_docs
                    .iter()
                    .position(|(g, n)| *g == gid && *n == name)
                {
                    self.collapsed_docs.remove(i);
                } else {
                    self.collapsed_docs.push((gid, name));
                }
                self.rebuild();
            }
            _ => {}
        }
    }

    fn group_of_row(&self, r: usize) -> u32 {
        (0..=r)
            .rev()
            .find_map(|k| match &self.rows[k] {
                Row::Group { id, .. } => Some(*id),
                _ => None,
            })
            .unwrap_or(nsql_bookmarks::DEFAULT_GROUP)
    }

    /// 행 활성화 — 항목 = 이동(`permanent` = 정식 탭 · 아니면 미리보기) · 그룹/문서 = 접기 토글.
    fn activate(&mut self, r: usize, permanent: bool) {
        match self.rows.get(r).cloned() {
            Some(Row::Item { id, .. }) => self.actions.push(BmAction::Goto(id, permanent)),
            Some(Row::Group { .. } | Row::Doc { .. }) => self.toggle_fold(r),
            None => {}
        }
    }

    pub(crate) fn set_dblclick_ms(&mut self, ms: u128) {
        self.dblclick_ms = ms.max(1);
    }

    /// 접은 그룹 id들(프로젝트 파일에 저장 · 사용자 09-23 "좌측 기능별 복원").
    pub(crate) fn collapsed_groups(&self) -> Vec<u32> {
        self.collapsed_groups.clone()
    }

    pub(crate) fn set_collapsed_groups(&mut self, ids: Vec<u32>) {
        if self.collapsed_groups != ids {
            self.collapsed_groups = ids;
            self.rebuild();
        }
    }

    // ───────────── 우클릭 메뉴(69 §6-6) ─────────────

    fn open_menu(&mut self, p: Point) {
        let row = self.row_at(p);
        self.menu_row = row;
        if let Some(r) = row {
            self.sel = Some(r);
        }
        let mut items: Vec<CtxItem> = Vec::new();
        match row.and_then(|r| self.rows.get(r).cloned()) {
            Some(Row::Item { id, .. }) => {
                let bm = self.item(id).cloned();
                items.push(CtxItem::item("open", t(Msg::MnBmOpen)));
                items.push(CtxItem::Separator);
                items.push(CtxItem::item("rename", t(Msg::MnBmLabel)));
                let mut mn: Vec<CtxItem> = (0..10u8)
                    .map(|n| {
                        let label = if bm.as_ref().and_then(|b| b.mnemonic) == Some(n) {
                            format!("{n} ✓")
                        } else {
                            n.to_string()
                        };
                        CtxItem::item(format!("mn:{n}"), label)
                    })
                    .collect();
                mn.push(CtxItem::Separator);
                mn.push(CtxItem::maybe(
                    "mn:clear",
                    t(Msg::MnBmMnemonicClear),
                    bm.as_ref().is_some_and(|b| b.mnemonic.is_some()),
                ));
                items.push(CtxItem::submenu("mn", t(Msg::MnBmMnemonic), mn));
                let cur_g = bm.as_ref().map_or(0, |b| b.group);
                let mut gs: Vec<CtxItem> = self
                    .groups
                    .iter()
                    .map(|g| {
                        CtxItem::maybe(format!("grp:{}", g.id), self.group_label(g), g.id != cur_g)
                    })
                    .collect();
                gs.push(CtxItem::Separator);
                gs.push(CtxItem::item("new_group", t(Msg::MnBmNewGroup)));
                items.push(CtxItem::submenu("grp", t(Msg::MnBmMoveGroup), gs));
                items.push(CtxItem::Separator);
                items.push(CtxItem::item("remove", t(Msg::MnBmRemove)));
            }
            Some(Row::Doc { .. }) => {
                items.push(CtxItem::item("doc.open", t(Msg::MnBmDocOpen)));
                items.push(CtxItem::Separator);
                items.push(CtxItem::item("doc.select", t(Msg::MnBmDocSelectAll)));
                items.push(CtxItem::Separator);
                items.push(CtxItem::item("doc.remove", t(Msg::MnBmDocRemoveAll)));
            }
            Some(Row::Group { id, enabled, .. }) => {
                let is_default = self
                    .groups
                    .iter()
                    .find(|g| g.id == id)
                    .is_some_and(|g| g.default);
                items.push(CtxItem::item("grp.rename", t(Msg::MnBmGroupRename)));
                items.push(CtxItem::maybe(
                    "grp.default",
                    t(Msg::MnBmGroupDefault),
                    !is_default,
                ));
                items.push(CtxItem::item(
                    "grp.toggle",
                    if enabled {
                        t(Msg::MnBmGroupDisable)
                    } else {
                        t(Msg::MnBmGroupEnable)
                    },
                ));
                items.push(CtxItem::Separator);
                items.push(CtxItem::item("new_group", t(Msg::MnBmNewGroup)));
                items.push(CtxItem::Separator);
                items.push(CtxItem::maybe(
                    "grp.delete",
                    t(Msg::MnBmGroupDelete),
                    !is_default,
                ));
                items.push(CtxItem::maybe(
                    "grp.delete_all",
                    t(Msg::MnBmGroupDeleteAll),
                    !is_default,
                ));
            }
            None => {
                items.push(CtxItem::item("new_group", t(Msg::MnBmNewGroup)));
                items.push(CtxItem::item("remove_invalid", t(Msg::MnBmRemoveInvalid)));
                items.push(CtxItem::Separator);
                items.push(CtxItem::item("settings", t(Msg::MnBmSettings)));
            }
        }
        let text_w = (self.row_h * 10).max(180);
        // host = 아는 한 창 전체(팝업 배치 규칙 ③) — 패널은 창 높이를 모르므로 폭만 clamp · 세로는 그리는 시점의 표면 안전망에 기댄다.
        let host = Rect::new(0, 0, self.clamp_w, i32::MAX / 4);
        self.menu.open_at(p.x, p.y, items, host, text_w);
    }

    fn menu_pick(&mut self, id: &str) {
        let row = self.menu_row.and_then(|r| self.rows.get(r).cloned());
        let item_id = match &row {
            Some(Row::Item { id, .. }) => Some(*id),
            _ => None,
        };
        match id {
            "open" => {
                if let Some(i) = item_id {
                    self.actions.push(BmAction::Goto(i, true));
                }
            }
            "rename" | "grp.rename" => {
                if let Some(r) = self.menu_row {
                    self.sel = Some(r);
                    self.begin_rename();
                }
            }
            "mn:clear" => {
                if let Some(i) = item_id {
                    self.actions.push(BmAction::Mnemonic(i, None));
                }
            }
            "remove" => {
                if let Some(i) = item_id {
                    self.actions.push(BmAction::Remove(i));
                }
            }
            "doc.open" | "doc.select" | "doc.remove" => {
                if let Some(Row::Doc { key, .. }) = row {
                    let act = match id {
                        "doc.open" => match self.items.iter().find(|b| b.doc == key) {
                            Some(b) => BmAction::Goto(b.id, true),
                            None => return,
                        },
                        "doc.select" => BmAction::SelectAllInDoc(key),
                        _ => BmAction::RemoveDoc(key),
                    };
                    self.actions.push(act);
                }
            }
            "grp.default" | "grp.toggle" | "grp.delete" | "grp.delete_all" => {
                if let Some(Row::Group { id: gid, .. }) = row {
                    self.actions.push(match id {
                        "grp.default" => BmAction::DefaultGroup(gid),
                        "grp.toggle" => BmAction::ToggleGroup(gid),
                        "grp.delete" => BmAction::DeleteGroup(gid, true),
                        _ => BmAction::DeleteGroup(gid, false),
                    });
                }
            }
            "new_group" => self.actions.push(BmAction::NewGroup),
            "remove_invalid" => self.actions.push(BmAction::RemoveInvalid),
            "settings" => self.actions.push(BmAction::OpenSettings),
            other => {
                if let Some(n) = other.strip_prefix("mn:").and_then(|n| n.parse::<u8>().ok()) {
                    if let Some(i) = item_id {
                        self.actions.push(BmAction::Mnemonic(i, Some(n)));
                    }
                } else if let Some(g) = other
                    .strip_prefix("grp:")
                    .and_then(|g| g.parse::<u32>().ok())
                {
                    if let Some(i) = item_id {
                        self.actions.push(BmAction::MoveGroup(i, g));
                    }
                }
            }
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if !self.visible {
            return false;
        }
        let mut r = self.filter.tick(now_ms) | self.bars.tick(now_ms);
        if let Some((_, tb, _)) = self.rename.as_mut() {
            r |= tb.tick(now_ms);
        }
        r
    }

    pub(crate) fn animating(&self) -> bool {
        self.visible && self.filter.is_animating()
    }

    /// 이벤트(마우스 = 패널 안 · 키 = 포커스일 때 · 메뉴가 열려 있으면 메뉴가 먼저) — 다시 그려야 하면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        if matches!(
            ev,
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
        ) && self.filter.on_event(ev, &mut inv) == FilterEvent::Changed
        {
            self.filter_text = self.filter.text();
            self.rebuild();
            return true;
        }
        if self.menu.is_open() {
            let outside = self.menu.is_outside_click(ev);
            let consumed = self.menu.on_event(ev) && !outside;
            if let Some(id) = self.menu.take_picked() {
                self.menu_pick(&id);
                return true;
            }
            if consumed {
                return true;
            }
        }
        if let Some((_, tb, kind)) = self.rename.as_mut() {
            match ev {
                InputEvent::Key {
                    key: CtlKey::Enter, ..
                } => {
                    let text = tb.text();
                    let act = match kind {
                        RenameKind::Item(id) => BmAction::Rename(*id, text),
                        RenameKind::Group(g) => BmAction::RenameGroup(*g, text),
                    };
                    self.actions.push(act);
                    self.rename = None;
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                } => {
                    self.rename = None;
                    return true;
                }
                InputEvent::MouseDown { x, y, .. }
                    if !tb.bounds().contains(Point { x: *x, y: *y }) =>
                {
                    self.rename = None;
                }
                _ => {
                    tb.on_event(ev, &mut inv);
                    return true;
                }
            }
        }
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
            self.clamp_scroll();
        }
        if consumed {
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let h = self.row_at(Point { x, y });
                let changed = h != self.hover.map(|(r, _)| r);
                self.hover = h.map(|r| (r, Instant::now()));
                changed
            }
            InputEvent::RightDown { x, y } => {
                self.filter.set_focused(false);
                self.open_menu(Point { x, y });
                true
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                let in_f = self.filter.bounds().contains(p);
                self.filter.set_focused(in_f);
                if in_f {
                    if self.filter.on_event(ev, &mut inv) == FilterEvent::Changed {
                        self.filter_text = self.filter.text();
                        self.rebuild();
                    }
                    return true;
                }
                if let Some(r) = self.row_at(p) {
                    self.sel = Some(r);
                    // 같은 행을 `dblclick_ms` 안에 다시 = 더블클릭 → 정식 탭(프로젝트 탐색기와 같은 규칙 · 사용자 09-23) · 세 번째는 새 시작.
                    let now = Instant::now();
                    let dbl = matches!(self.last_click, Some((j, t0)) if j == r && now.duration_since(t0).as_millis() < self.dblclick_ms);
                    self.last_click = if dbl { None } else { Some((r, now)) };
                    match self.rows.get(r) {
                        Some(Row::Item { .. }) => self.activate(r, dbl),
                        _ => {
                            // 셰브론 자리(왼쪽)를 누르면 접기 · 그 밖은 선택만.
                            if x < self.list_rect.x + self.px(22.0) + self.row_indent(r) {
                                self.toggle_fold(r);
                            }
                        }
                    }
                } else if self.list_rect.contains(p) {
                    // 빈 곳 클릭 = 선택 해제 · 캐럿(테두리)만 남김(사용자 09-23).
                    self.caret = self.sel.or(self.caret);
                    self.sel = None;
                }
                true
            }
            InputEvent::Wheel { delta } => {
                self.scroll_y -= delta / 120 * self.row_h * 3;
                self.clamp_scroll();
                true
            }
            InputEvent::Key { key, primary, .. } => {
                if self.filter.is_focused() {
                    match key {
                        CtlKey::Down
                        | CtlKey::Up
                        | CtlKey::Enter
                        | CtlKey::Delete
                        | CtlKey::Home
                        | CtlKey::End
                            if !self.rows.is_empty() => {}
                        CtlKey::Escape => {
                            if self.filter_text.is_empty() {
                                return false;
                            }
                            self.filter.set_text("");
                            self.filter_text.clear();
                            self.rebuild();
                            return true;
                        }
                        _ => {
                            let evt = self.filter.on_event(ev, &mut inv);
                            let now = self.filter.text();
                            if evt == FilterEvent::Changed || now != self.filter_text {
                                self.filter_text = now;
                                self.rebuild();
                            }
                            return true;
                        }
                    }
                }
                let n = self.rows.len();
                match key {
                    CtlKey::Down if n > 0 => {
                        let r = self.sel.or(self.caret).map_or(0, |s| (s + 1).min(n - 1));
                        self.sel = Some(r);
                        self.reveal(r);
                        true
                    }
                    CtlKey::Up if n > 0 => {
                        let r = self.sel.or(self.caret).map_or(0, |s| s.saturating_sub(1));
                        self.sel = Some(r);
                        self.reveal(r);
                        true
                    }
                    CtlKey::Home if n > 0 => {
                        self.sel = Some(0);
                        self.reveal(0);
                        true
                    }
                    CtlKey::End if n > 0 => {
                        self.sel = Some(n - 1);
                        self.reveal(n - 1);
                        true
                    }
                    CtlKey::Enter => {
                        if let Some(r) = self.sel {
                            if primary {
                                if let Some(Row::Item { id, .. }) = self.rows.get(r) {
                                    if let Some(b) = self.item(*id) {
                                        self.actions.push(BmAction::SelectAllInDoc(b.doc.clone()));
                                    }
                                }
                            } else {
                                // Enter = 정식 탭(프로젝트 탐색기 Enter와 같음).
                                self.activate(r, true);
                            }
                        }
                        true
                    }
                    CtlKey::Delete => {
                        match self.sel.and_then(|r| self.rows.get(r).cloned()) {
                            Some(Row::Item { id, .. }) => self.actions.push(BmAction::Remove(id)),
                            Some(Row::Doc { key, .. }) => {
                                self.actions.push(BmAction::RemoveDoc(key))
                            }
                            _ => {}
                        }
                        true
                    }
                    CtlKey::Space => {
                        if let Some(r) = self.sel {
                            match self.rows.get(r) {
                                Some(Row::Group { id, .. }) => {
                                    self.actions.push(BmAction::ToggleGroup(*id));
                                }
                                Some(Row::Doc { .. }) => self.toggle_fold(r),
                                _ => {}
                            }
                        }
                        true
                    }
                    CtlKey::Left => {
                        if let Some(r) = self.sel {
                            match self.rows.get(r) {
                                Some(Row::Item { .. }) => {
                                    if let Some(d) = (0..r)
                                        .rev()
                                        .find(|&k| matches!(self.rows[k], Row::Doc { .. }))
                                    {
                                        self.sel = Some(d);
                                        self.reveal(d);
                                    }
                                }
                                Some(Row::Doc { expanded: true, .. })
                                | Some(Row::Group { expanded: true, .. }) => self.toggle_fold(r),
                                Some(Row::Doc { .. }) => {
                                    if let Some(g) = (0..r)
                                        .rev()
                                        .find(|&k| matches!(self.rows[k], Row::Group { .. }))
                                    {
                                        self.sel = Some(g);
                                        self.reveal(g);
                                    }
                                }
                                _ => {}
                            }
                        }
                        true
                    }
                    CtlKey::Right => {
                        if let Some(r) = self.sel {
                            if let Some(
                                Row::Doc {
                                    expanded: false, ..
                                }
                                | Row::Group {
                                    expanded: false, ..
                                },
                            ) = self.rows.get(r)
                            {
                                self.toggle_fold(r);
                            }
                        }
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn px(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn row_indent(&self, r: usize) -> i32 {
        match self.rows.get(r) {
            Some(Row::Group { .. }) => 0,
            Some(Row::Doc { .. }) => self.px(INDENT),
            Some(Row::Item { .. }) => self.px(INDENT) * 2,
            None => 0,
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let px = |v: f32| (v * self.scale).round() as i32;
        let b = self.bounds;
        let pad = px(PAD);
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        dc.select_font(FontSlot::Base, true);
        let hr = self.header_rect;
        let hy = dc.text_center_y(hr.y, hr.h);
        let title = tf(Msg::BmPanelTitle, &[&self.total_live.to_string()]);
        dc.text(hr.x + pad, hy, hr, &title, th.text);
        dc.select_font(FontSlot::Base, false);
        self.filter.paint(dc, th, true);
        let lr = self.list_rect;
        let rh = self.row_h.max(1);
        if self.rows.is_empty() {
            dc.select_font(FontSlot::Status, false);
            let mut y = lr.y + px(6.0);
            let msg = if self.filter_text.trim().is_empty() {
                t(Msg::BmEmptyHint)
            } else {
                t(Msg::BmNoMatch)
            };
            for line in msg.split('\n') {
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
            return;
        }
        let first = (self.scroll_y / rh) as usize;
        let mut y = lr.y - self.scroll_y % rh;
        let hover_row = self.hover.map(|(r, _)| r);
        // 셰브론 = 객체/프로젝트 탐색기와 같은 부품(`draw_chevron_90_in` · 글꼴 높이 · 접힘 흐림 · 펼침/호버 본문색 · 사용자 09-22).
        let chev_w = dc.text_height().max(10);
        for r in first..self.rows.len() {
            if y >= lr.bottom() {
                break;
            }
            let row_rect = Rect::new(lr.x, y, lr.w, rh);
            if self.sel == Some(r) {
                dc.fill_rect(
                    row_rect,
                    if self.focused {
                        th.sel_bg
                    } else {
                        th.sel_bg_inactive
                    },
                );
            } else if self.sel.is_none() && self.caret == Some(r) {
                dc.stroke_round_rect(row_rect, 0, th.text_dim, 1.0);
            } else if hover_row == Some(r) {
                dc.fill_rect_alpha(row_rect, th.text, 0.06);
            }
            if let Some((rr, tb, _)) = &self.rename {
                if *rr == r {
                    tb.paint(dc, th);
                    y += rh;
                    continue;
                }
            }
            let ty = dc.text_center_y(y, rh);
            let indent = self.row_indent(r);
            match &self.rows[r] {
                Row::Group {
                    name,
                    enabled,
                    expanded,
                    count,
                    ..
                } => {
                    let color = if *expanded || hover_row == Some(r) {
                        th.text
                    } else {
                        th.text_dim
                    };
                    nexa_ctl::controls::draw_chevron_90_in(
                        dc,
                        Rect::new(lr.x + pad, y + (rh - chev_w) / 2, chev_w, chev_w),
                        color,
                        *expanded,
                        Some(row_rect),
                    );
                    let tx = lr.x + pad + chev_w + px(4.0);
                    let sw = dc.text_width("■");
                    dc.text(
                        tx,
                        ty,
                        row_rect,
                        if *enabled { "■" } else { "□" },
                        if *enabled { th.accent } else { th.text_dim },
                    );
                    let tx = tx + sw + px(6.0);
                    dc.select_font(FontSlot::Base, true);
                    let cnt = count.to_string();
                    let cw = dc.text_width(&cnt);
                    let clip = Rect::new(tx, y, (lr.right() - tx - cw - pad * 2).max(0), rh);
                    dc.text(
                        tx,
                        ty,
                        clip,
                        name,
                        if *enabled { th.text } else { th.text_dim },
                    );
                    dc.select_font(FontSlot::Base, false);
                    dc.text(lr.right() - pad - cw, ty, row_rect, &cnt, th.text_dim);
                }
                Row::Doc {
                    name,
                    count,
                    expanded,
                    enabled,
                    ..
                } => {
                    let color = if *expanded || hover_row == Some(r) {
                        th.text
                    } else {
                        th.text_dim
                    };
                    nexa_ctl::controls::draw_chevron_90_in(
                        dc,
                        Rect::new(lr.x + pad + indent, y + (rh - chev_w) / 2, chev_w, chev_w),
                        color,
                        *expanded,
                        Some(row_rect),
                    );
                    let tx = lr.x + pad + indent + chev_w + px(4.0);
                    let cnt = count.to_string();
                    let cw = dc.text_width(&cnt);
                    let clip = Rect::new(tx, y, (lr.right() - tx - cw - pad * 2).max(0), rh);
                    dc.text(
                        tx,
                        ty,
                        clip,
                        name,
                        if *enabled { th.text } else { th.text_dim },
                    );
                    dc.text(lr.right() - pad - cw, ty, row_rect, &cnt, th.text_dim);
                }
                Row::Item { id, enabled } => {
                    let Some(bm) = self.item(*id) else {
                        y += rh;
                        continue;
                    };
                    let live = bm.is_live() && *enabled;
                    let fg = if live { th.text } else { th.text_dim };
                    let mut tx = lr.x + pad + indent;
                    let badge = match bm.mnemonic {
                        Some(n) => format!("{n}"),
                        None => "▮".into(),
                    };
                    let bw = dc.text_width(&badge);
                    dc.text(
                        tx,
                        ty,
                        row_rect,
                        &badge,
                        if live { th.accent } else { th.text_dim },
                    );
                    tx += bw.max(px(12.0)) + px(6.0);
                    dc.select_font(FontSlot::Status, false);
                    let ln = (bm.anchor.line + 1).to_string();
                    let lw = dc.text_width(&ln);
                    dc.text(tx, ty, row_rect, &ln, th.text_dim);
                    dc.select_font(FontSlot::Base, false);
                    tx += lw + px(8.0);
                    let mut text = bm.display();
                    if let State::Invalid { reason, .. } = bm.state {
                        let why = match reason {
                            Reason::FileMissing => t(Msg::BmReasonFile),
                            Reason::ObjectMissing => t(Msg::BmReasonObject),
                            Reason::TextGone => t(Msg::BmReasonText),
                            Reason::ServerUnknown => t(Msg::BmReasonServer),
                            Reason::TooBig => t(Msg::BmReasonBig),
                        };
                        text = format!("{text}  · {why}");
                    } else if bm.label.is_some() {
                        let snippet = bm.anchor.text.trim();
                        if !snippet.is_empty() {
                            text = format!("{text}  {snippet}");
                        }
                    }
                    let clip = Rect::new(tx, y, (lr.right() - tx - pad).max(0), rh);
                    let shown = nexa_ctl::draw::ellipsize_middle(dc, &text, clip.w);
                    dc.text(tx, ty, clip, &shown, fg);
                }
            }
            y += rh;
        }
        self.bars.paint(
            dc,
            th,
            lr,
            lr.w,
            self.content_h().max(lr.h),
            0,
            self.scroll_y,
            self.scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel_with_one_item() -> (BookmarksPanel, usize) {
        let mut p = BookmarksPanel::new();
        p.set_visible(true);
        p.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        let mut store = Store::new();
        let anchor = nsql_bookmarks::make_anchor(
            &["a", "b", "c"],
            1,
            0,
            &nsql_bookmarks::RelocateOpts::default(),
        );
        store
            .add(
                DocKey::File {
                    path: "C:/x/a.sql".into(),
                },
                anchor,
                1,
                true,
                100,
                1000,
            )
            .expect("add");
        p.sync(&store);
        let r = p
            .rows
            .iter()
            .position(|r| matches!(r, Row::Item { .. }))
            .expect("item row");
        (p, r)
    }

    /// 한 번 클릭 = 미리보기(`permanent=false`) · 같은 행을 간격 안에 다시 = 정식 탭(`true`) · 세 번째는 새 시작(사용자 09-23).
    #[test]
    fn single_click_preview_double_click_permanent() {
        let (mut p, r) = panel_with_one_item();
        let y = p.list_rect.y + r as i32 * p.row_h + p.row_h / 2;
        let x = p.list_rect.x + 120;
        let down = InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        };
        p.on_event(&down);
        p.on_event(&down);
        p.on_event(&down);
        let acts: Vec<BmAction> = std::iter::from_fn(|| p.take_action()).collect();
        let flags: Vec<bool> = acts
            .iter()
            .map(|a| match a {
                BmAction::Goto(_, perm) => *perm,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(flags, vec![false, true, false]);
    }

    /// Enter = 정식 탭(프로젝트 탐색기 Enter와 같음).
    #[test]
    fn enter_opens_permanent() {
        let (mut p, r) = panel_with_one_item();
        p.sel = Some(r);
        p.on_event(&InputEvent::Key {
            key: CtlKey::Enter,
            shift: false,
            primary: false,
        });
        assert!(matches!(p.take_action(), Some(BmAction::Goto(_, true))));
    }
}
