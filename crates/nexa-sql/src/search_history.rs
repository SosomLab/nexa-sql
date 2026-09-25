//! 검색어 이력 부품(사용자 09-23 "각 검색 영역별(박스별)로 최대 20개(설정)의 검색어를 보관 · 프로그램 전역 속성" ·
//! "검색 창을 클릭하면 최대 5개(설정) 미리보기 + 스크롤 콤보 · ↑/↓/PgUp/PgDn/Home/End · 보기 방식 Flat/Dropdown(기본 Dropdown) ·
//! Flat에서도 ↑ 이전 ↓ 다음 · 마지막에서 ↓ = 다른 컨트롤로 포커스").
//!
//! - **전역 한 파일** `<설정 폴더>/search-history.json`(`NSQL_HOME` 격리 규약 · 프로젝트·폴더 모드와 무관) — 상자 이름 → 최근순 목록.
//! - **상자마다 이름**(`find.query` · `find.replace` · `search.query` · `search.where` · `filter.project` · `filter.bookmarks` ·
//!   `filter.extensions` · `filter.outline` · `prefs.search`) — 같은 이름을 쓰는 상자는 이력을 공유한다.
//! - **기록 시점** = Enter(찾기 실행 · 파일 검색 실행 · 필터 확정) · 상자가 포커스를 잃을 때(비어 있지 않으면) — 타이핑마다 쌓지 않는다.
//!   같은 글은 앞으로 옮기고(중복 없음) 상한(`search.history_max` · 0 = 끔)을 넘으면 오래된 것부터 버린다 · 바뀌면 바로 쓴다(작은 파일).
//! - **보기 방식**(`search.history_view` · 기본 **Dropdown**): **Dropdown** = 상자를 클릭하면(그리고 닫힌 채 ↑/↓를 누르면) 상자 아래에
//!   최근 N개(`search.history_rows` · 기본 5)를 nexa-ctl `ContextMenu`(행 수 상한 + 스크롤 · 팝업 배치 규칙)로 미리보기 · ↑/↓/PgUp/PgDn/
//!   Home/End · 휠 · Enter/클릭 = 넣기 · Esc = 닫기 · 타이핑 = 그 글이 든 항목만으로 다시 거름 · 바깥 클릭 = 닫고 통과.
//!   **Flat** = 상자 안에서 ↑(오래된 쪽)/↓(새로운 쪽 → 끝에서 치던 글로 복귀)로 되부른다(Sublime·VS Code 찾기 상자).
//! - **마지막에서 ↓ = 다음 컨트롤**: Flat은 치던 글(맨 아래)에서 ↓ · Dropdown은 마지막 항목에서 ↓ · 그리고 Tab — [`RecallEvent::LeaveDown`]
//!   으로 알리면 담는 쪽(패널)이 목록으로 포커스를 옮긴다(프로젝트 탐색기 = 폴더 목록 첫 행).
//! - 호스트는 [`SharedHistory`] 하나를 만들어 상자를 가진 패널마다 건네고, 패널은 [`Recall`]로 사건을 잇는다(30 §2 부품).

use nexa_ctl::controls::{ContextMenu, CtxItem};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nsql_settings::json::{dump, parse, Json};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// 전역 이력 파일 이름(설정 폴더 바로 아래) · 상한 기본값 = 설정 `search.history_max`(20).
pub(crate) const FILE_NAME: &str = "search-history.json";

/// 이력 보기 방식(설정 `search.history_view`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryView {
    /// 상자 아래 드롭다운 목록(기본).
    Dropdown,
    /// 상자 안에서 ↑/↓로만.
    Flat,
}

impl HistoryView {
    pub(crate) fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("flat") {
            HistoryView::Flat
        } else {
            HistoryView::Dropdown
        }
    }
}

/// 상자 이름 → 최근순 검색어.
pub(crate) struct SearchHistory {
    max: usize,
    boxes: Vec<(String, Vec<String>)>,
    /// 없으면 메모리 전용(시험 · 설정 폴더를 모를 때).
    path: Option<PathBuf>,
    view: HistoryView,
    /// 드롭다운에 한 번에 보이는 행 수(설정 `search.history_rows`).
    rows: usize,
}

pub(crate) type SharedHistory = Rc<RefCell<SearchHistory>>;

impl SearchHistory {
    pub(crate) fn new(max: usize) -> Self {
        SearchHistory {
            max,
            boxes: Vec::new(),
            path: None,
            view: HistoryView::Dropdown,
            rows: 5,
        }
    }

    /// 설정 폴더에서 읽는다(없거나 깨지면 빈 이력 · 쓰기는 같은 자리).
    pub(crate) fn load_in(dir: Option<&Path>, max: usize) -> Self {
        let mut h = Self::new(max);
        let Some(dir) = dir else {
            return h;
        };
        let p = dir.join(FILE_NAME);
        if let Ok(text) = std::fs::read_to_string(&p) {
            match Self::from_json(&text, max) {
                Ok(loaded) => h = loaded,
                Err(e) => eprintln!("search-history: {}: {e}", p.display()),
            }
        }
        h.path = Some(p);
        h
    }

    pub(crate) fn shared(self) -> SharedHistory {
        Rc::new(RefCell::new(self))
    }

    pub(crate) fn view(&self) -> HistoryView {
        self.view
    }

    pub(crate) fn set_view(&mut self, v: HistoryView) {
        self.view = v;
    }

    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    pub(crate) fn set_rows(&mut self, n: usize) {
        self.rows = n.clamp(1, 50);
    }

    /// 상한 변경(설정 창) — 줄어들면 잘라 내고 저장 · 0 = 끔(목록 비움 · 기록 안 함).
    pub(crate) fn set_max(&mut self, max: usize) {
        if self.max == max {
            return;
        }
        self.max = max;
        let mut changed = false;
        for (_, v) in &mut self.boxes {
            if v.len() > max {
                v.truncate(max);
                changed = true;
            }
        }
        self.boxes.retain(|(_, v)| !v.is_empty());
        if changed {
            self.save();
        }
    }

    /// 그 상자의 최근순 목록(없으면 빈 슬라이스).
    pub(crate) fn list(&self, key: &str) -> &[String] {
        self.boxes
            .iter()
            .find(|(k, _)| k == key)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    /// 검색어 기록 — 앞에 넣고 같은 글은 하나만 · 빈 글(공백뿐)은 무시 · 바뀌었으면 저장하고 true.
    pub(crate) fn push(&mut self, key: &str, term: &str) -> bool {
        if self.max == 0 || term.trim().is_empty() {
            return false;
        }
        let idx = match self.boxes.iter().position(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                self.boxes.push((key.to_string(), Vec::new()));
                self.boxes.len() - 1
            }
        };
        let v = &mut self.boxes[idx].1;
        if v.first().is_some_and(|f| f == term) {
            return false;
        }
        v.retain(|t| t != term);
        v.insert(0, term.to_string());
        v.truncate(self.max);
        self.save();
        true
    }

    /// 한 상자 비우기(메뉴용 · 저장).
    #[allow(dead_code)]
    pub(crate) fn clear(&mut self, key: &str) {
        let before = self.boxes.len();
        self.boxes.retain(|(k, _)| k != key);
        if self.boxes.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        let Some(p) = &self.path else {
            return;
        };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json()).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    }

    pub(crate) fn to_json(&self) -> String {
        let boxes = self
            .boxes
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| {
                (
                    k.clone(),
                    Json::Arr(v.iter().map(|t| Json::Str(t.clone())).collect()),
                )
            })
            .collect();
        dump(&Json::Obj(vec![
            ("version".into(), Json::Num(1.0)),
            ("boxes".into(), Json::Obj(boxes)),
        ]))
    }

    /// 모르는 키는 무시 · 문자열이 아닌 항목은 그 항목만 버림 · 읽을 때도 상한을 적용.
    pub(crate) fn from_json(text: &str, max: usize) -> Result<Self, String> {
        let root = parse(text)?;
        let Json::Obj(fields) = root else {
            return Err("root must be an object".into());
        };
        let mut h = Self::new(max);
        if let Some((_, Json::Obj(boxes))) = fields.iter().find(|(k, _)| k == "boxes") {
            for (name, v) in boxes {
                let Json::Arr(items) = v else { continue };
                let mut list: Vec<String> = Vec::new();
                for it in items {
                    if let Json::Str(s) = it {
                        if !s.trim().is_empty() && !list.contains(s) {
                            list.push(s.clone());
                        }
                    }
                }
                list.truncate(max);
                if !list.is_empty() {
                    h.boxes.push((name.clone(), list));
                }
            }
        }
        Ok(h)
    }
}

/// [`Recall::on_event`]의 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecallEvent {
    /// 이력과 무관 — 담는 쪽이 평소대로(상자에 넣는다).
    Pass,
    /// 이력이 먹었다(드롭다운 이동/열기/닫기 · Flat 되부르기 끝).
    Consumed,
    /// 상자 글이 바뀌었다(되부름/고름) — 담는 쪽은 상자 글로 다시 거른다/찾는다.
    Changed,
    /// 이력의 끝에서 ↓(또는 Tab) — 담는 쪽이 **다음 컨트롤(목록)로 포커스**를 옮긴다.
    LeaveDown,
}

/// 상자 하나의 되부르기 상태(패널이 상자마다 하나 든다) — Flat 위치 + 드롭다운 메뉴.
pub(crate) struct Recall {
    key: String,
    /// Flat: 지금 보이는 이력 위치(0 = 가장 최근) · None = 치던 글.
    pos: Option<usize>,
    /// 되부르기 시작 전에 치던 글(↓로 끝까지 내려오면 복귀).
    draft: String,
    /// 마지막으로 우리가 넣은 글의 본문 세대 — 다르면 사용자가 그 뒤 타이핑한 것.
    rev: u64,
    /// 드롭다운(행 수 상한 + 스크롤 · 팝업 배치 규칙).
    menu: ContextMenu,
    /// 드롭다운에 든 항목(표시 순 · 거른 결과).
    shown: Vec<String>,
    /// 마지막으로 연 자리(글을 치며 다시 거를 때 같은 자리에).
    host: Rect,
    scale: f32,
}

impl Recall {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Recall {
            key: key.into(),
            pos: None,
            draft: String::new(),
            rev: u64::MAX,
            menu: ContextMenu::new(),
            shown: Vec::new(),
            host: Rect::new(0, 0, i32::MAX / 2, i32::MAX / 4),
            scale: 1.0,
        }
    }

    /// 드롭다운이 열려 있는가.
    pub(crate) fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    pub(crate) fn close(&mut self) {
        self.menu.close();
    }

    /// 드롭다운 영역(닫혀 있으면 빈 rect).
    #[allow(dead_code)]
    pub(crate) fn bounds(&self) -> Rect {
        self.menu.bounds()
    }

    #[cfg(test)]
    pub(crate) fn shown_for_test(&self) -> &[String] {
        &self.shown
    }

    /// 드롭다운을 그린다(팝업 층 · 담는 쪽의 `paint_popup` 끝에).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.menu.is_open() {
            self.menu.paint(dc, th);
        }
    }

    /// 상자를 클릭했을 때(Dropdown 모드) — 이력이 있으면 상자 아래에 연다. `host` = 팝업이 넘어가면 안 되는 영역(아는 한 창 전체).
    pub(crate) fn on_click(&mut self, tb: &TextBox, h: &SharedHistory, host: Rect, scale: f32) {
        self.host = host;
        self.scale = scale;
        if h.borrow().view() == HistoryView::Dropdown {
            self.open_dropdown(tb, h, false);
        }
    }

    /// 드롭다운 열기/다시 거르기 — 상자 글이 비어 있지 않으면 그 글이 든 항목만(대소문자 무시) · 없으면 닫는다.
    fn open_dropdown(&mut self, tb: &TextBox, h: &SharedHistory, hover_first: bool) {
        let q = tb.text().to_lowercase();
        let hist = h.borrow();
        let all = hist.list(&self.key);
        self.shown = all
            .iter()
            .filter(|t| q.is_empty() || t.to_lowercase().contains(&q))
            .cloned()
            .collect();
        let rows = hist.rows();
        drop(hist);
        if self.shown.is_empty() {
            self.menu.close();
            return;
        }
        let items: Vec<CtxItem> = self
            .shown
            .iter()
            .enumerate()
            .map(|(i, t)| CtxItem::item(format!("h:{i}"), t.clone()))
            .collect();
        let b = tb.bounds();
        self.menu.set_scale(self.scale);
        self.menu.set_max_rows(Some(rows));
        // 폭 근사 = 상자 폭(첫 그리기가 실측으로 올려친다 · 팝업 규칙 안전망).
        self.menu.open_at(
            b.x,
            b.bottom() + (2.0 * self.scale) as i32,
            items,
            self.host,
            b.w.max(1),
        );
        if hover_first {
            let _ = self.menu.on_event(&InputEvent::Key {
                key: CtlKey::Down,
                shift: false,
                primary: false,
            });
        }
    }

    /// 고른 항목을 상자에 넣는다(캐럿 끝).
    fn put(&mut self, tb: &mut TextBox, text: &str, inv: &mut Invalidations) {
        tb.set_text(text);
        let n = text.chars().count();
        tb.select_range(n, n, inv);
        self.rev = tb.text_rev();
        self.pos = None;
    }

    /// 사건 하나 — 담는 쪽은 상자에 포커스가 있을 때(그리고 드롭다운이 열려 있으면 마우스도) 먼저 부른다.
    pub(crate) fn on_event(
        &mut self,
        ev: &InputEvent,
        tb: &mut TextBox,
        h: &SharedHistory,
        inv: &mut Invalidations,
    ) -> RecallEvent {
        // Tab = 다음 컨트롤로(열려 있으면 닫고).
        if let InputEvent::Char { c: '\t', .. } = ev {
            self.menu.close();
            return RecallEvent::LeaveDown;
        }
        if self.menu.is_open() {
            return self.on_event_open(ev, tb, h, inv);
        }
        let InputEvent::Key {
            key: key @ (CtlKey::Up | CtlKey::Down),
            shift: false,
            primary: false,
        } = ev
        else {
            return RecallEvent::Pass;
        };
        let view = h.borrow().view();
        if view == HistoryView::Dropdown {
            // 닫힌 채 ↑/↓ = 드롭다운 열기(↓ = 첫 항목 hover · ↑ = 마지막) · 이력이 없으면 ↓는 다음 컨트롤로.
            self.open_dropdown(tb, h, false);
            if !self.menu.is_open() {
                return if *key == CtlKey::Down {
                    RecallEvent::LeaveDown
                } else {
                    RecallEvent::Pass
                };
            }
            let _ = self.menu.on_event(&InputEvent::Key {
                key: *key,
                shift: false,
                primary: false,
            });
            return RecallEvent::Consumed;
        }
        // Flat.
        let list: Vec<String> = h.borrow().list(&self.key).to_vec();
        if tb.text_rev() != self.rev {
            // 그 사이 타이핑했다 — 지금 글이 새 초안.
            self.pos = None;
            self.draft = tb.text();
        }
        if list.is_empty() {
            return if *key == CtlKey::Down {
                RecallEvent::LeaveDown
            } else {
                RecallEvent::Pass
            };
        }
        let next = match (*key, self.pos) {
            (CtlKey::Up, None) => Some(0),
            (CtlKey::Up, Some(p)) => Some((p + 1).min(list.len() - 1)),
            // 치던 글(맨 아래)에서 ↓ = 다음 컨트롤로(사용자 09-23).
            (CtlKey::Down, None) => return RecallEvent::LeaveDown,
            (CtlKey::Down, Some(0)) => None,
            (CtlKey::Down, Some(p)) => Some(p - 1),
            _ => return RecallEvent::Pass,
        };
        let text = next.map_or_else(|| self.draft.clone(), |i| list[i].clone());
        let changed = text != tb.text();
        self.put(tb, &text, inv);
        self.pos = next;
        if changed {
            RecallEvent::Changed
        } else {
            RecallEvent::Consumed
        }
    }

    fn on_event_open(
        &mut self,
        ev: &InputEvent,
        tb: &mut TextBox,
        h: &SharedHistory,
        inv: &mut Invalidations,
    ) -> RecallEvent {
        match ev {
            InputEvent::Key {
                key: CtlKey::Down,
                shift: false,
                primary: false,
            } => {
                // 마지막 항목에서 ↓ = 닫고 다음 컨트롤로(사용자 09-23 "마지막 기록까지 간 다음 아래 한 번 더").
                if self.menu.hovered() == Some(self.shown.len().saturating_sub(1)) {
                    self.menu.close();
                    return RecallEvent::LeaveDown;
                }
                let _ = self.menu.on_event(ev);
                RecallEvent::Consumed
            }
            InputEvent::Key {
                key: CtlKey::Up | CtlKey::PageUp | CtlKey::PageDown | CtlKey::Home | CtlKey::End,
                ..
            } => {
                let _ = self.menu.on_event(ev);
                RecallEvent::Consumed
            }
            InputEvent::Key {
                key: CtlKey::Enter, ..
            } => {
                if self.menu.hovered().is_none() {
                    // hover 없는 Enter = 메뉴는 닫고 상자의 Enter(찾기/검색 실행)로.
                    self.menu.close();
                    return RecallEvent::Pass;
                }
                let _ = self.menu.on_event(ev);
                self.apply_pick(tb, h, inv)
            }
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } => {
                self.menu.close();
                RecallEvent::Consumed
            }
            InputEvent::MouseMove { .. } => {
                let _ = self.menu.on_event(ev);
                RecallEvent::Pass
            }
            InputEvent::MouseDown { x, y, .. } => {
                if self.menu.bounds().contains(Point { x: *x, y: *y }) {
                    let _ = self.menu.on_event(ev);
                    return self.apply_pick(tb, h, inv);
                }
                // 바깥 클릭 = 닫고 그 클릭은 그대로 진행(팝업 UX 규칙).
                self.menu.close();
                RecallEvent::Pass
            }
            // 우클릭 = 어디든 닫고 그 클릭은 그대로(사용자 09-25 "다른 영역의 좌/우 클릭으로 유효성이 상실되면 바로 감춰").
            InputEvent::RightDown { .. } => {
                self.menu.close();
                RecallEvent::Pass
            }
            // 휠 = 목록 스크롤(메뉴가 커서 아래 행을 다시 hover).
            InputEvent::Wheel { .. } => {
                let _ = self.menu.on_event(ev);
                RecallEvent::Consumed
            }
            // 글자·Backspace 등 = 상자로(담는 쪽이 넣은 뒤 `after_edit`로 다시 거른다).
            _ => RecallEvent::Pass,
        }
    }

    fn apply_pick(
        &mut self,
        tb: &mut TextBox,
        _h: &SharedHistory,
        inv: &mut Invalidations,
    ) -> RecallEvent {
        let Some(id) = self.menu.take_picked() else {
            return RecallEvent::Consumed;
        };
        let Some(i) = id.strip_prefix("h:").and_then(|n| n.parse::<usize>().ok()) else {
            return RecallEvent::Consumed;
        };
        let Some(text) = self.shown.get(i).cloned() else {
            return RecallEvent::Consumed;
        };
        let changed = text != tb.text();
        self.put(tb, &text, inv);
        self.menu.close();
        if changed {
            RecallEvent::Changed
        } else {
            RecallEvent::Consumed
        }
    }

    /// 상자 글이 바뀐 뒤(타이핑 · 지우기) — 드롭다운이 열려 있으면 그 글로 다시 거른다(없으면 닫힘).
    pub(crate) fn after_edit(&mut self, tb: &TextBox, h: &SharedHistory) {
        if self.menu.is_open() {
            self.open_dropdown(tb, h, false);
        }
    }

    /// 상자의 글을 이력에 올린다(Enter · 포커스 잃음) — 되부르기 위치는 처음으로 · 드롭다운은 닫는다.
    pub(crate) fn commit(&mut self, tb: &TextBox, h: &SharedHistory) {
        let text = tb.text();
        self.pos = None;
        self.rev = u64::MAX;
        self.menu.close();
        h.borrow_mut().push(&self.key, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_dedups_fronts_and_caps() {
        let mut h = SearchHistory::new(3);
        assert!(h.push("find.query", "a"));
        assert!(h.push("find.query", "b"));
        assert!(!h.push("find.query", "  "), "빈 글은 무시");
        assert!(!h.push("find.query", "b"), "맨 앞과 같으면 변화 없음");
        assert!(h.push("find.query", "a"), "같은 글은 앞으로");
        assert_eq!(h.list("find.query"), ["a", "b"]);
        h.push("find.query", "c");
        h.push("find.query", "d");
        assert_eq!(
            h.list("find.query"),
            ["d", "c", "a"],
            "상한 3 = 오래된 b 탈락"
        );
        assert!(h.list("other").is_empty());
        h.set_max(1);
        assert_eq!(h.list("find.query"), ["d"]);
        h.set_max(0);
        assert!(!h.push("find.query", "x"), "0 = 끔");
    }

    #[test]
    fn json_roundtrip_and_file() {
        let mut h = SearchHistory::new(20);
        h.push("find.query", "select \"a\"\n");
        h.push("find.query", "한글 ㄱ");
        h.push("filter.project", "*.sql");
        let text = h.to_json();
        let back = SearchHistory::from_json(&text, 20).expect("parse");
        assert_eq!(back.list("find.query"), ["한글 ㄱ", "select \"a\"\n"]);
        assert_eq!(back.list("filter.project"), ["*.sql"]);
        // 읽을 때 상한 적용 · 모르는 키 무시 · 문자열 아닌 항목만 버림.
        let odd = r#"{"version":1,"boxes":{"find.query":["x",1,"y","z"]},"extra":true}"#;
        let b = SearchHistory::from_json(odd, 2).expect("parse");
        assert_eq!(b.list("find.query"), ["x", "y"]);
        // 파일: 격리 임시 폴더에만 쓴다(실제 설정 폴더 금지).
        let dir = std::env::temp_dir().join(format!("nsql-sh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut on_disk = SearchHistory::load_in(Some(&dir), 20);
        assert!(on_disk.list("find.query").is_empty());
        on_disk.push("find.query", "q1");
        assert!(dir.join(FILE_NAME).exists(), "바뀌면 바로 쓴다");
        let again = SearchHistory::load_in(Some(&dir), 20);
        assert_eq!(again.list("find.query"), ["q1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn key(k: CtlKey) -> InputEvent {
        InputEvent::Key {
            key: k,
            shift: false,
            primary: false,
        }
    }

    fn flat_history(entries: &[&str]) -> SharedHistory {
        let mut h = SearchHistory::new(20);
        h.set_view(HistoryView::Flat);
        for e in entries {
            h.push("find.query", e);
        }
        h.shared()
    }

    /// Flat: ↑ = 최근부터 오래된 쪽 · 끝에서 더 올라가지 않음 · ↓ = 새로운 쪽 → 치던 글 복귀 → **한 번 더 ↓ = LeaveDown** ·
    /// 타이핑하면 위치 초기화 · 이력 없으면 ↑ 통과 · ↓ LeaveDown · Tab = LeaveDown.
    #[test]
    fn flat_recall_steps_and_leaves_downward() {
        let h = flat_history(&[]);
        let mut tb = TextBox::new("");
        let mut inv = Invalidations::default();
        let mut r = Recall::new("find.query");
        assert_eq!(
            r.on_event(&key(CtlKey::Up), &mut tb, &h, &mut inv),
            RecallEvent::Pass
        );
        assert_eq!(
            r.on_event(&key(CtlKey::Down), &mut tb, &h, &mut inv),
            RecallEvent::LeaveDown,
            "이력 없음 + ↓ = 다음 컨트롤"
        );
        let h = flat_history(&["old", "new"]);
        tb.set_text("typing");
        assert_eq!(
            r.on_event(&key(CtlKey::Up), &mut tb, &h, &mut inv),
            RecallEvent::Changed
        );
        assert_eq!(tb.text(), "new");
        assert_eq!(
            r.on_event(&key(CtlKey::Up), &mut tb, &h, &mut inv),
            RecallEvent::Changed
        );
        assert_eq!(tb.text(), "old");
        assert_eq!(
            r.on_event(&key(CtlKey::Up), &mut tb, &h, &mut inv),
            RecallEvent::Consumed,
            "가장 오래된 것에서 멈춤"
        );
        assert_eq!(
            r.on_event(&key(CtlKey::Down), &mut tb, &h, &mut inv),
            RecallEvent::Changed
        );
        assert_eq!(tb.text(), "new");
        assert_eq!(
            r.on_event(&key(CtlKey::Down), &mut tb, &h, &mut inv),
            RecallEvent::Changed
        );
        assert_eq!(tb.text(), "typing", "치던 글로 복귀");
        assert_eq!(
            r.on_event(&key(CtlKey::Down), &mut tb, &h, &mut inv),
            RecallEvent::LeaveDown,
            "맨 아래에서 ↓ = 다음 컨트롤"
        );
        // 되부른 뒤 타이핑 → 다음 ↑는 다시 최근부터.
        r.on_event(&key(CtlKey::Up), &mut tb, &h, &mut inv);
        tb.set_text("new2");
        r.on_event(&key(CtlKey::Up), &mut tb, &h, &mut inv);
        assert_eq!(tb.text(), "new");
        // Shift+↑는 손대지 않는다 · Tab = LeaveDown.
        let shifted = InputEvent::Key {
            key: CtlKey::Up,
            shift: true,
            primary: false,
        };
        assert_eq!(
            r.on_event(&shifted, &mut tb, &h, &mut inv),
            RecallEvent::Pass
        );
        let tab = InputEvent::Char { c: '\t', now_ms: 0 };
        assert_eq!(
            r.on_event(&tab, &mut tb, &h, &mut inv),
            RecallEvent::LeaveDown
        );
    }

    /// Dropdown(기본): 클릭 = 상자 아래 목록(행 수 상한 = `rows`) · ↓/↑ 이동 · Enter = 넣기(Changed) · 타이핑 = 다시 거름 ·
    /// 마지막 항목에서 ↓ = 닫고 LeaveDown · hover 없는 Enter = 닫고 Pass(상자의 Enter로) · Esc = 닫기.
    #[test]
    fn dropdown_lists_filters_picks_and_leaves() {
        let mut hist = SearchHistory::new(20);
        hist.set_rows(3);
        for e in ["alpha", "beta", "gamma", "delta", "alps"] {
            hist.push("filter.project", e);
        }
        let h = hist.shared();
        let mut tb = TextBox::new("");
        let mut inv = Invalidations::default();
        tb.set_bounds(Rect::new(20, 20, 200, 25), &mut inv);
        let mut r = Recall::new("filter.project");
        let host = Rect::new(0, 0, 800, 600);
        r.on_click(&tb, &h, host, 1.0);
        assert!(r.is_open(), "클릭 = 드롭다운");
        assert_eq!(r.shown.len(), 5);
        assert!(r.bounds().y >= 45, "상자 아래");
        // hover 없는 Enter = 닫고 Pass.
        assert_eq!(
            r.on_event(&key(CtlKey::Enter), &mut tb, &h, &mut inv),
            RecallEvent::Pass
        );
        assert!(!r.is_open());
        // 닫힌 채 ↓ = 열고 첫 항목 hover · Enter = 넣기.
        assert_eq!(
            r.on_event(&key(CtlKey::Down), &mut tb, &h, &mut inv),
            RecallEvent::Consumed
        );
        assert!(r.is_open());
        assert_eq!(
            r.on_event(&key(CtlKey::Enter), &mut tb, &h, &mut inv),
            RecallEvent::Changed
        );
        assert_eq!(tb.text(), "alps", "가장 최근 항목");
        assert!(!r.is_open());
        // 타이핑으로 거르기: "al" → alpha · alps 둘.
        tb.set_text("al");
        r.on_click(&tb, &h, host, 1.0);
        assert_eq!(r.shown, vec!["alps", "alpha"]);
        // End → 마지막 · 한 번 더 ↓ = LeaveDown(닫힘).
        assert_eq!(
            r.on_event(&key(CtlKey::End), &mut tb, &h, &mut inv),
            RecallEvent::Consumed
        );
        assert_eq!(
            r.on_event(&key(CtlKey::Down), &mut tb, &h, &mut inv),
            RecallEvent::LeaveDown
        );
        assert!(!r.is_open());
        // Esc = 닫기 · 바깥 클릭 = 닫고 Pass.
        r.on_click(&tb, &h, host, 1.0);
        assert_eq!(
            r.on_event(&key(CtlKey::Escape), &mut tb, &h, &mut inv),
            RecallEvent::Consumed
        );
        r.on_click(&tb, &h, host, 1.0);
        let outside = InputEvent::MouseDown {
            x: 700,
            y: 500,
            shift: false,
            primary: false,
        };
        assert_eq!(
            r.on_event(&outside, &mut tb, &h, &mut inv),
            RecallEvent::Pass
        );
        assert!(!r.is_open());
        // ★ 행 클릭 = 고르기(09-25 결함: 상자 밖이라 사건이 닿지 않았다) · 우클릭 = 어디든 닫고 Pass.
        tb.set_text("");
        r.on_click(&tb, &h, host, 1.0);
        assert!(r.is_open());
        let row1 = r.menu.row_rect_of(1).expect("둘째 행 영역");
        let click = InputEvent::MouseDown {
            x: row1.x + 5,
            y: row1.y + row1.h / 2,
            shift: false,
            primary: false,
        };
        assert_eq!(
            r.on_event(&click, &mut tb, &h, &mut inv),
            RecallEvent::Changed
        );
        assert_eq!(tb.text(), r.shown_for_test()[1], "클릭한 행의 글");
        assert!(!r.is_open());
        r.on_click(&tb, &h, host, 1.0);
        assert!(r.is_open());
        assert_eq!(
            r.on_event(
                &InputEvent::RightDown { x: 700, y: 500 },
                &mut tb,
                &h,
                &mut inv
            ),
            RecallEvent::Pass
        );
        assert!(!r.is_open(), "우클릭 = 즉시 감춤");
        // Flat 모드로 바꾸면 클릭해도 안 열린다.
        h.borrow_mut().set_view(HistoryView::Flat);
        r.on_click(&tb, &h, host, 1.0);
        assert!(!r.is_open());
    }
}
