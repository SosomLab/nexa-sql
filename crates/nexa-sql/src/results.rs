//! 결과 탭 패널(T-93 · docs/43 §4 · D-71·D-73~75 · 사용자 09-16) — **편집기 탭 하나 ↔ 결과 탭 여러 개**.
//!
//! 구조 규칙(성능): 활성 결과 탭의 그리드는 호스트의 `App.grid`가 들고 있고, 이 패널의 그 자리는 **자리표시자**다
//! (`tabs[active].grid` = 빈 그리드). 탭을 바꾸면 호스트가 `mem::swap`으로 그리드를 맞바꾼다 → 그리기·이벤트는 언제나
//! 활성 그리드 하나만 만진다. 잠든 탭은 rows·폭 캐시만 쥐고 있다가 닫히면 즉시 drop.
//!
//! 탭 바 = 편집기 탭과 같은 nexa-ctl `TabBar`(단일 행 고정 · 넘치면 ◀ ▶ · 드래그 재정렬 · 핀). 표시는 설정
//! `grid.result_tabbar_single`(끔 = 2개 이상일 때만 · 켬 = 1개여도). 우클릭 메뉴 = 닫기 · 다른 탭 닫기 · 오른쪽 탭 닫기 · 고정/해제 ·
//! 맨 앞/뒤로 · 첫/마지막 탭 활성.

use crate::grid::Grid;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TabAction, TabBar, Widget};
use nsql_i18n::{t, Msg};

/// 결과 탭 하나.
pub(crate) struct ResultTab {
    /// 전역 고유 id(워커 요청의 키 · 편집기 탭 id와 다른 공간).
    pub id: u64,
    pub title: String,
    pub pinned: bool,
    /// 사용자가 이름을 붙였으면 실행 결과로 제목을 덮지 않는다.
    pub named: bool,
    /// ★ **이 결과를 만든 실행 쿼리**(사용자 09-21) — 결과가 도착할 때 보관하고 우클릭 ▸ "실행 쿼리 복사"가 클립보드에 담는다.
    /// 제목을 번호(`결과N`)로 바꾼 뒤에는 탭만 봐서는 어느 문장의 결과인지 알 수 없다. 바인드·치환이 적용되기 전의 **편집기 원문**.
    pub sql: String,
    /// 잠든 그리드(활성 탭이면 자리표시자).
    pub grid: Grid,
    /// 생성 순서(자동 정리 = 가장 오래된 비고정 탭).
    pub seq: u64,
    /// ★ 한 문장이 결과를 둘 이상 냈을 때(REF CURSOR 여러 개 · 암묵 결과 · 다중 결과 집합)의 **딸린 탭**:
    /// `(실행을 시작한 탭 id, 몇 번째 추가 결과인가)` — 다시 실행하면 같은 자리를 재사용하고, 이번 실행에서 안 쓰인 것은 걷는다.
    pub child_of: Option<(u64, u32)>,
}

/// 호스트가 처리할 패널 동작.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PanelAction {
    Activate(usize),
    Close(usize),
    CloseOthers(usize),
    CloseRight(usize),
    TogglePin(usize),
    MoveFirst(usize),
    MoveLast(usize),
    /// 이름 바꾸기(호스트가 입력 프롬프트를 연다).
    Rename(usize),
    /// 이 결과를 만든 실행 쿼리를 클립보드에(호스트가 담는다 — 패널은 클립보드를 모른다).
    CopySql(usize),
    /// 드래그 재정렬.
    Move {
        from: usize,
        to: usize,
    },
}

pub(crate) struct ResultPanel {
    pub tabs: Vec<ResultTab>,
    pub active: usize,
    bar: TabBar,
    menu: CtxMenu,
    /// 탭 바 영역(보이지 않으면 높이 0).
    bar_rect: Rect,
    scale: f32,
    /// 마지막으로 탭 바에 보낸 제목 목록(바뀔 때만 다시 보낸다).
    shown: Vec<String>,
    /// 우클릭한 탭(메뉴 항목 선택 때 대상).
    menu_tab: Option<usize>,
    /// 탭 수가 1↔2를 넘어 탭 바 표시가 바뀌었다(호스트 재배치 1회성).
    bar_changed: bool,
    /// 설정 `grid.result_tabbar_single = on`.
    always_bar: bool,
    /// 설정 `grid.result_tab_title = number`(기본).
    numbered: bool,
    /// 설정 `grid.result_tabs`(끄면 탭 1개만 · 바 없음).
    enabled: bool,
}

impl ResultPanel {
    pub(crate) fn new(first: ResultTab, enabled: bool, always_bar: bool) -> Self {
        let mut bar = TabBar::new();
        bar.set_multiline(false);
        bar.set_show_new(false);
        bar.set_close_last(true);
        ResultPanel {
            tabs: vec![first],
            active: 0,
            bar,
            menu: CtxMenu::new(),
            bar_rect: Rect::default(),
            scale: 1.0,
            shown: Vec::new(),
            menu_tab: None,
            bar_changed: false,
            always_bar,
            enabled,
            numbered: true,
        }
        .with_first_numbered()
    }

    fn with_first_numbered(mut self) -> Self {
        self.number_if_default(0);
        self
    }

    /// 제목 규칙(설정 `grid.result_tab_title`): 참 = `결과1` `결과2` …(기본 · 사용자 09-21) · 거짓 = 테이블 이름/첫 낱말(종전).
    pub(crate) fn set_numbered(&mut self, on: bool) {
        self.numbered = on;
    }

    pub(crate) fn numbered(&self) -> bool {
        self.numbered
    }

    /// 다음 번호 제목 — **지금 있는 번호 제목의 가장 큰 수 + 1**(`결과3`·`결과6`이 있으면 `결과7` · 빈 번호를 메우지 않는다 —
    /// 닫은 탭의 번호가 다른 결과에 다시 붙으면 헷갈린다). 사용자가 같은 꼴로 직접 붙인 이름도 센다(겹치지 않게).
    pub(crate) fn next_numbered_title(&self) -> String {
        let max = self
            .tabs
            .iter()
            .filter_map(|t| numbered_index(&t.title))
            .max()
            .unwrap_or(0);
        nsql_i18n::tf(Msg::ResultTabN, &[&(max + 1).to_string()])
    }

    /// 번호 규칙일 때: 아직 기본 문구("결과")인 탭에 번호 제목을 준다(이름 붙인 탭 · 이미 번호가 있는 탭은 그대로).
    pub(crate) fn number_if_default(&mut self, i: usize) {
        if !self.numbered {
            return;
        }
        let Some(tab) = self.tabs.get(i) else { return };
        if tab.named || numbered_index(&tab.title).is_some() {
            return;
        }
        if tab.title == t(Msg::ResultTabDefault) || tab.title.is_empty() {
            let title = self.next_numbered_title();
            self.tabs[i].title = title;
        }
    }

    /// 결과가 도착한 탭의 제목을 번호 규칙으로 — 번호가 없으면(종전 규칙의 이름 · 기본 문구) 새 번호 · 있으면 그대로.
    pub(crate) fn ensure_numbered(&mut self, i: usize) {
        let Some(tab) = self.tabs.get(i) else { return };
        if tab.named || tab.pinned || numbered_index(&tab.title).is_some() {
            return;
        }
        let title = self.next_numbered_title();
        self.tabs[i].title = title;
    }

    /// 이름 바꾸기 — 빈 이름은 무시 · 붙인 이름은 실행 결과가 덮지 않는다(`named`).
    pub(crate) fn rename(&mut self, id: u64, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(i) = self.index_of(id) else {
            return false;
        };
        self.tabs[i].title = name.to_string();
        self.tabs[i].named = true;
        self.sync_bar();
        true
    }

    pub(crate) fn set_options(&mut self, enabled: bool, always_bar: bool) {
        if self.enabled != enabled || self.always_bar != always_bar {
            self.bar_changed = true;
        }
        self.enabled = enabled;
        self.always_bar = always_bar;
    }

    /// 탭 바를 그릴 것인가(D-74).
    pub(crate) fn bar_visible(&self) -> bool {
        self.enabled && (self.always_bar || self.tabs.len() >= 2)
    }

    /// 탭 바 높이(물리 px · 보이지 않으면 0).
    pub(crate) fn bar_height(&self, scale: f32) -> i32 {
        if self.bar_visible() {
            (self.bar.preferred_height() as f32 * scale)
                .round()
                .max(1.0) as i32
        } else {
            0
        }
    }

    /// 배치 — 호스트가 결과 영역 전체를 주면 위에 탭 바(보일 때만).
    pub(crate) fn set_bounds(&mut self, area: Rect, scale: f32) -> Rect {
        self.scale = scale;
        self.bar.set_scale(scale);
        self.menu.set_scale(scale);
        let bh = self.bar_height(scale);
        let mut inv = Invalidations::default();
        self.bar_rect = Rect::new(area.x, area.y, area.w, bh);
        self.bar.set_bounds(self.bar_rect, &mut inv);
        self.bar_changed = false;
        Rect::new(area.x, area.y + bh, area.w, (area.h - bh).max(0))
    }

    /// 탭 수 변화로 바 표시가 바뀌었는가(1회성 · 호스트가 `layout`).
    pub(crate) fn take_bar_changed(&mut self) -> bool {
        std::mem::take(&mut self.bar_changed)
    }

    /// 탭 제목·활성·핀을 탭 바에 반영(바뀔 때만).
    pub(crate) fn sync_bar(&mut self) {
        let titles: Vec<String> = self
            .tabs
            .iter()
            .map(|t| {
                if t.pinned {
                    format!("📌 {}", t.title)
                } else {
                    t.title.clone()
                }
            })
            .collect();
        let mut inv = Invalidations::default();
        if titles != self.shown || self.bar.active() != self.active {
            self.shown = titles.clone();
            self.bar.set_tabs(titles, self.active, &mut inv);
            self.bar
                .set_pinned(self.tabs.iter().map(|t| t.pinned).collect(), &mut inv);
        }
    }

    pub(crate) fn active_id(&self) -> u64 {
        self.tabs.get(self.active).map_or(0, |t| t.id)
    }

    pub(crate) fn index_of(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// 탭 바·메뉴 이벤트. 소비했으면 `Some(동작 또는 None)` · 바 밖이면 `None`.
    pub(crate) fn route(&mut self, ev: &InputEvent, cursor: Point) -> Option<Option<PanelAction>> {
        if self.menu.is_open() {
            let consumed = self.menu.on_event(ev);
            if let Some(id) = self.menu.take_picked() {
                let i = self.menu_tab.take().unwrap_or(self.active);
                let act = match id.as_str() {
                    "rename" => Some(PanelAction::Rename(i)),
                    "copy_sql" => Some(PanelAction::CopySql(i)),
                    "close" => Some(PanelAction::Close(i)),
                    "close_others" => Some(PanelAction::CloseOthers(i)),
                    "close_right" => Some(PanelAction::CloseRight(i)),
                    "pin" => Some(PanelAction::TogglePin(i)),
                    "first" => Some(PanelAction::MoveFirst(i)),
                    "last" => Some(PanelAction::MoveLast(i)),
                    "go_first" => Some(PanelAction::Activate(0)),
                    "go_last" => Some(PanelAction::Activate(self.tabs.len().saturating_sub(1))),
                    _ => None,
                };
                return Some(act);
            }
            if consumed
                || !matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                )
            {
                return Some(None);
            }
            // 바깥 클릭 = 닫고 그 클릭을 그대로 진행(팝업 UX 규칙).
        }
        if !self.bar_visible() {
            return None;
        }
        let p = match *ev {
            InputEvent::MouseMove { x, y }
            | InputEvent::MouseDown { x, y, .. }
            | InputEvent::MouseUp { x, y }
            | InputEvent::RightDown { x, y } => Point { x, y },
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => cursor,
            _ => return None,
        };
        if !self.bar_rect.contains(p) && self.bar.dragging().is_none() {
            return None;
        }
        let mut inv = Invalidations::default();
        self.bar.on_event(ev, &mut inv);
        let act = match self.bar.take_action() {
            Some(TabAction::Switch(i)) => Some(PanelAction::Activate(i)),
            Some(TabAction::Close(i)) => Some(PanelAction::Close(i)),
            Some(TabAction::Move { from, to }) => Some(PanelAction::Move { from, to }),
            Some(TabAction::Context(i)) => {
                self.open_menu(i, p);
                None
            }
            Some(TabAction::New | TabAction::Badge(_) | TabAction::BadgeContext(_)) | None => None,
        };
        Some(act)
    }

    /// 결과 탭의 rect(탭 줄이 보일 때만) — 이름 바꾸기 상자가 그 탭에 붙는다.
    pub(crate) fn tab_rect(&self, i: usize) -> Option<Rect> {
        self.bar_visible().then(|| self.bar.tab_rect(i)).flatten()
    }

    /// 자체 캡처용(기동 명령 `result.menu`): 활성 탭의 메뉴를 탭 줄 자리에 연다 — 결과 영역이 창 아래쪽이라 메뉴가 아래로
    /// 넘치는 자리다(잘림 확인 · 입력 주입 없음).
    pub(crate) fn open_menu_for_capture(&mut self) {
        let p = Point {
            x: self.bar_rect.x + (24.0 * self.scale) as i32,
            y: self.bar_rect.y + self.bar_rect.h / 2,
        };
        self.open_menu(self.active, p);
    }

    fn open_menu(&mut self, i: usize, p: Point) {
        self.menu_tab = Some(i);
        let pinned = self.tabs.get(i).is_some_and(|t| t.pinned);
        // 실행한 적이 없는 탭(쿼리 없음)은 복사 항목을 흐리게.
        let has_sql = self.tabs.get(i).is_some_and(|t| !t.sql.trim().is_empty());
        let items = vec![
            CtxItem::item("rename", t(Msg::MnResultRename)),
            CtxItem::maybe("copy_sql", t(Msg::MnResultCopySql), has_sql),
            CtxItem::Separator,
            CtxItem::item("close", t(Msg::MnResultCloseTab)),
            CtxItem::item("close_others", t(Msg::MnResultCloseOthers)),
            CtxItem::item("close_right", t(Msg::MnResultCloseRight)),
            CtxItem::Separator,
            CtxItem::item(
                "pin",
                t(if pinned {
                    Msg::MnResultUnpin
                } else {
                    Msg::MnResultPin
                }),
            ),
            CtxItem::Separator,
            CtxItem::item("first", t(Msg::MnResultMoveFirst)),
            CtxItem::item("last", t(Msg::MnResultMoveLast)),
            CtxItem::Separator,
            CtxItem::item("go_first", t(Msg::MnResultFirstTab)),
            CtxItem::item("go_last", t(Msg::MnResultLastTab)),
        ];
        // 창 크기를 모르는 자리 — 메뉴 부품이 그리는 시점에 표면 안으로 스스로 들어온다(nexa-ctl `place_popup` · 61 §2-2 팝업 규칙).
        let host = Rect::new(0, 0, i32::MAX / 2, i32::MAX / 2);
        let text_w = (200.0 * self.scale) as i32;
        self.menu.open_at(p.x, p.y, items, host, text_w);
    }

    pub(crate) fn paint_bar(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.bar_visible() {
            self.bar.paint(dc, th);
        }
    }

    /// 우클릭 메뉴(팝업 층 · 맨 마지막에).
    pub(crate) fn paint_popups(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.menu.paint(dc, th);
    }

    /// 탭 하나 추가(맨 뒤 · 활성으로 만들지는 않는다 — 호스트가 그리드 교환과 함께 활성화).
    pub(crate) fn push(&mut self, tab: ResultTab) -> usize {
        let was = self.bar_visible();
        self.tabs.push(tab);
        self.number_if_default(self.tabs.len() - 1);
        if self.bar_visible() != was {
            self.bar_changed = true;
        }
        self.tabs.len() - 1
    }

    /// 탭 제거(활성 탭이면 호스트가 먼저 그리드를 돌려놓고 부른다) — 남은 탭이 없으면 `None`.
    pub(crate) fn remove(&mut self, i: usize) -> Option<ResultTab> {
        if i >= self.tabs.len() {
            return None;
        }
        let was = self.bar_visible();
        let tab = self.tabs.remove(i);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if i < self.active {
            self.active -= 1;
        }
        if self.bar_visible() != was {
            self.bar_changed = true;
        }
        Some(tab)
    }

    /// 자동 정리 후보 = 가장 오래된 **비고정·비활성** 탭(D-75 `grid.result_tab_evict`).
    pub(crate) fn evict_candidate(&self) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(i, t)| !t.pinned && *i != self.active)
            .min_by_key(|(_, t)| t.seq)
            .map(|(i, _)| i)
    }

    /// 제목 자동 부여(같은 이름이면 ` 2` `3`… 접미).
    pub(crate) fn unique_title(&self, base: &str, except: usize) -> String {
        let n = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                *i != except && (t.title == base || t.title.starts_with(&format!("{base} ")))
            })
            .count();
        if n == 0 {
            base.to_string()
        } else {
            format!("{base} {}", n + 1)
        }
    }
}

/// 번호 제목(`결과7` · `Result 7`)의 번호 — 지금 언어의 틀과 **다른 언어의 틀** 둘 다 읽는다(언어를 바꾼 뒤에도 번호가 이어지게).
pub(crate) fn numbered_index(title: &str) -> Option<u32> {
    let title = title.trim();
    for lang in nsql_i18n::Lang::ALL {
        let Some((pre, post)) = nsql_i18n::tr(lang, Msg::ResultTabN).split_once("{0}") else {
            continue;
        };
        if let Some(mid) = title
            .strip_prefix(pre)
            .and_then(|rest| rest.strip_suffix(post))
        {
            let mid = mid.trim();
            if !mid.is_empty() && mid.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(n) = mid.parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// 실행문에서 탭 제목 후보 — 테이블 이름 → 첫 단어(대문자) → 기본 문구.
pub(crate) fn title_from_sql(table: Option<&str>, sql: &str) -> String {
    if let Some(t) = table.filter(|s| !s.trim().is_empty()) {
        return t.trim().trim_matches('"').to_string();
    }
    let word = sql
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|w| !w.is_empty())
        .map(|w| w.to_ascii_uppercase())
        .unwrap_or_default();
    if word.is_empty() {
        t(Msg::ResultTabDefault).to_string()
    } else {
        word
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: u64, seq: u64, pinned: bool) -> ResultTab {
        ResultTab {
            id,
            title: format!("T{id}"),
            pinned,
            named: false,
            sql: String::new(),
            grid: Grid::default(),
            seq,
            child_of: None,
        }
    }

    #[test]
    fn bar_visibility_follows_tab_count_and_setting() {
        let mut p = ResultPanel::new(tab(1, 1, false), true, false);
        assert!(!p.bar_visible(), "탭 1개 = auto에서 숨김");
        p.push(tab(2, 2, false));
        assert!(p.bar_visible());
        assert!(p.take_bar_changed(), "1→2에서 재배치 신호");
        assert!(!p.take_bar_changed(), "1회성");
        p.set_options(true, true);
        p.remove(1);
        assert!(p.bar_visible(), "always면 1개여도 표시");
        p.set_options(false, true);
        assert!(!p.bar_visible(), "끄면 표시 없음");
    }

    #[test]
    fn remove_keeps_active_sane_and_evicts_oldest_unpinned() {
        let mut p = ResultPanel::new(tab(1, 1, false), true, false);
        p.push(tab(2, 2, true));
        p.push(tab(3, 3, false));
        p.active = 2;
        assert_eq!(p.evict_candidate(), Some(0), "가장 오래된 비고정·비활성");
        p.remove(0);
        assert_eq!(p.active, 1, "앞 탭이 빠지면 활성 인덱스가 당겨진다");
        p.remove(1);
        assert_eq!(p.active, 0, "활성 탭 제거 = 마지막 남은 탭");
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.evict_candidate(), None, "고정·활성뿐이면 없음");
    }

    fn tab_default(id: u64) -> ResultTab {
        ResultTab {
            title: t(Msg::ResultTabDefault).to_string(),
            ..tab(id, id, false)
        }
    }

    fn panel_of(id: u64) -> ResultPanel {
        ResultPanel::new(tab_default(id), true, false)
    }

    /// 실행 쿼리 복사(사용자 09-21): 쿼리가 있는 탭만 메뉴 항목이 켜지고 · 고르면 `CopySql(그 탭)`이 나온다(클립보드는 호스트 몫 —
    /// 테스트는 클립보드를 건드리지 않는다).
    #[test]
    fn copy_sql_menu_item_follows_the_tab() {
        let mut p = panel_of(1);
        p.push(tab_default(2));
        p.tabs[1].sql = "select * from emp".into();
        let enabled = |p: &mut ResultPanel, i: usize| {
            p.open_menu(i, Point { x: 5, y: 5 });
            let on = p.menu.items_for_test().iter().any(
                |it| matches!(it, CtxItem::Item { id, enabled: true, .. } if id == "copy_sql"),
            );
            p.menu.close();
            on
        };
        assert!(!enabled(&mut p, 0), "실행한 적 없는 탭 = 흐림");
        assert!(enabled(&mut p, 1));
    }

    /// 번호 제목(사용자 09-21): 새 탭 = 가장 큰 번호 + 1(`결과3`·`결과6` → `결과7` · 빈 번호를 메우지 않는다) · 두 언어의 틀 ·
    /// 이름 붙인 탭은 그대로 · 같은 꼴로 직접 붙인 이름도 센다 · 이름 바꾸기 = `named`(빈 이름 무시).
    #[test]
    fn numbered_titles_and_rename() {
        assert_eq!(numbered_index("결과7"), Some(7));
        assert_eq!(numbered_index("Result 12"), Some(12));
        assert_eq!(numbered_index("결과"), None);
        assert_eq!(numbered_index("결과 x"), None);
        assert_eq!(numbered_index("EMP 2"), None);
        let mut p = panel_of(1);
        assert_eq!(numbered_index(&p.tabs[0].title), Some(1), "첫 탭 = 1번");
        for _ in 0..5 {
            p.push(tab_default(99));
        }
        let nums: Vec<u32> = p
            .tabs
            .iter()
            .filter_map(|t| numbered_index(&t.title))
            .collect();
        assert_eq!(nums, vec![1, 2, 3, 4, 5, 6]);
        // 3번·6번만 남긴다 → 다음은 7번.
        p.tabs
            .retain(|t| matches!(numbered_index(&t.title), Some(3 | 6)));
        let i = p.push(tab_default(100));
        assert_eq!(numbered_index(&p.tabs[i].title), Some(7));
        // 이름 바꾸기.
        let id = p.tabs[i].id;
        assert!(!p.rename(id, "   "));
        assert!(p.rename(id, " 월말 집계 "));
        assert_eq!(
            (p.tabs[i].title.as_str(), p.tabs[i].named),
            ("월말 집계", true)
        );
        p.ensure_numbered(i);
        assert_eq!(p.tabs[i].title, "월말 집계", "붙인 이름은 덮지 않는다");
        // 종전 규칙(테이블 이름)이면 번호를 붙이지 않는다.
        p.set_numbered(false);
        let j = p.push(tab_default(101));
        assert_eq!(numbered_index(&p.tabs[j].title), None);
    }

    #[test]
    fn titles_from_sql_and_unique_suffix() {
        assert_eq!(
            title_from_sql(Some("M4S_I002040"), "select * from x"),
            "M4S_I002040"
        );
        assert_eq!(title_from_sql(None, "  select 1"), "SELECT");
        let mut p = ResultPanel::new(tab(1, 1, false), true, false);
        p.tabs[0].title = "EMP".into();
        p.push(tab(2, 2, false));
        assert_eq!(p.unique_title("EMP", 1), "EMP 2");
        assert_eq!(p.unique_title("DEPT", 1), "DEPT");
    }
}
