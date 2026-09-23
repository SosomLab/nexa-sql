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

use crate::filterbar::{FilterBar, FilterEvent, SideBtn, GAP_Y, INPUT_H};
use crate::findbar::BtnKind;
use crate::toolicons;
use nexa_ctl::controls::{ContextMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{IconImage, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox};
use nexa_fs::shell::{IconKey, IconService, Lookup};
use nsql_i18n::{t, tf, Msg};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// OPEN FILES 한 줄(편집기 탭 · 사용자 09-23 "파일 필터 위쪽에 열린 파일 목록").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenFile {
    pub id: u64,
    pub title: String,
    pub dirty: bool,
    pub active: bool,
    /// 동시 편집 칸에 든 탭(다중 선택 표시).
    pub grouped: bool,
}

/// OPEN FILES에 보이는 최대 줄 수(넘치면 "+n").
const OPEN_FILES_MAX: usize = 8;
/// 아이콘 조회가 다 끝난 뒤 이만큼 유휴면 셸 아이콘 워커를 거둔다(ms).
const ICON_WORKER_IDLE_MS: u64 = 5_000;

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

/// ★ 필터 열거 워커(사용자 09-23 "폴더/파일이 많아도 멈추지 않게 · 별도 스레드 · 분할 병렬 · 로그 병합") — [`crate::parwalk`]가
///   아직 열거하지 않은 폴더들 아래를 병렬로 읽어 **폴더째** 보내고, 패널은 틱마다 시간 예산 안에서 트리에 합친다(메인 루프는 막히지 않는다).
///   필터 글이 바뀌면 이전 열거는 취소되고(수신자 버림) 이미 트리에 들어온 폴더는 `loaded`라 다시 읽지 않는다.
struct Scan {
    rx: std::sync::mpsc::Receiver<crate::parwalk::DirMsg>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// 받은 폴더 수(진행 표시).
    dirs: usize,
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 틱 하나가 트리 합치기에 쓰는 시간 예산(ms) — 그 뒤는 다음 틱(입력이 밀리지 않게).
const SCAN_APPLY_BUDGET_MS: u128 = 6;

pub(crate) struct ProjectPanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    filter: FilterBar,
    /// 프로젝트 이름(없으면 None = 빈 상태).
    name: Option<String>,
    nodes: Vec<Node>,
    roots: Vec<usize>,
    /// 보이는 행 = 노드 인덱스.
    rows: Vec<usize>,
    scroll_y: i32,
    /// 가로 스크롤(긴 이름 · 깊은 트리 · 사용자 09-22) — 내용 폭은 행이 바뀔 때 한 번 잰다(`rows_gen`/`measured_gen`).
    scroll_x: i32,
    content_w: i32,
    rows_gen: u64,
    measured_gen: u64,
    bars: ScrollBars,
    sel: Option<usize>,
    /// 선택이 없을 때의 **캐럿 행**(빈 곳 클릭으로 해제한 자리 · 테두리만 · 키보드 이동의 시작점 · 사용자 09-23).
    caret: Option<usize>,
    hover: Option<(usize, Instant)>,
    list_rect: Rect,
    header_rect: Rect,
    /// 빈 상태의 링크 행(명령 id · 영역).
    links: Vec<(&'static str, Rect)>,
    row_h: i32,
    open: Option<OpenReq>,
    /// 링크·헤더·우클릭 메뉴가 낸 명령 id(`project.*` · 접미 인자가 붙을 수 있어 String).
    command: Option<String>,
    /// 우클릭 메뉴(루트 = 프로젝트에서 폴더 제거 · 어디서나 폴더 추가 · 팝업 배치 규칙 = `ContextMenu` · 사용자 09-22).
    menu: ContextMenu,
    /// 메뉴를 연 행(루트 노드면 그 폴더 순번 = `roots` 순번).
    menu_row: Option<usize>,
    /// 최근 프로젝트가 있는가(빈 상태에 "프로젝트 전환…" 링크를 보일지 · 09-22).
    has_recent: bool,
    last_click: Option<(usize, Instant)>,
    dblclick_ms: u128,
    tooltip_ms: u128,
    clamp_w: i32,
    show_hidden: bool,
    show_dot: bool,
    /// 필터 열거의 메모리 상한(트리 항목 수 · 설정 `project.scan_max` · **0 = 무제한**).
    scan_max: usize,
    /// 필터 열거가 상한에 걸렸다(안내 한 줄).
    scan_capped: bool,
    /// 진행 중인 필터 열거(워커 · 없으면 None).
    scan: Option<Scan>,
    /// 워커 스레드 수(설정 `project.scan_threads` · 0 = 코어 수/2).
    scan_threads: usize,
    /// 읽지 못한 폴더(경로 · 사유) — 도착 순 병합 · 호스트가 `take_scan_log`로 로그 창에 옮긴다.
    scan_log: Vec<(PathBuf, String)>,
    /// 이번 필터 열거에서 읽지 못한 폴더 수(안내 한 줄).
    scan_errors: usize,
    /// 경로 → 노드(워커가 보낸 폴더를 트리에서 찾는다 · 노드는 지워지지 않고 붙기만 한다).
    path_index: std::collections::HashMap<PathBuf, usize>,
    filter_text: String,
    /// 파일/폴더 아이콘(설정 `project.icons` · 파일 대화상자와 같은 OS 셸 아이콘 서비스 · 향상 모드 = 끔 · 사용자 09-22).
    icons_on: bool,
    /// 서비스 RGBA → 이 창의 `IconImage` 변환 캐시(키 = 종류) · 폴백 그림 둘.
    icons: HashMap<IconKey, Rc<IconImage>>,
    fallback_dir: Rc<IconImage>,
    fallback_file: Rc<IconImage>,
    /// 마지막으로 본 서비스 버전(바뀌면 = 조회 결과 도착 → 다시 그린다).
    icon_version: u64,
    /// 조회가 전부 끝난 시각(ms) · `None` = 조회 중이거나 이미 거뒀다. 이 시각에서 [`ICON_WORKER_IDLE_MS`] 지나면
    /// 아이콘 워커 스레드(COM 아파트먼트 · 셸 아이콘 캐시 핸들 +GDI)를 거둔다(파일 대화상자는 닫힐 때 거둠 · 패널은 늘 열려 있으니 유휴로 · 96차 성능 점검).
    icon_idle_since: Option<u64>,
    /// OPEN FILES(편집기 탭 목록 · 호스트가 틱마다 `set_open_files`) · 그 영역.
    open_files: Vec<OpenFile>,
    open_rect: Rect,
    open_rows: Vec<Rect>,
    /// OPEN FILES에서 머문 행(× 닫기 표시).
    open_hover: Option<usize>,
    /// 활성 탭과 맞춘 선택 경로(사용자 09-22 "탭을 고르면 탐색기에도 선택 표시") — 접혀 있으면 행이 없어 보이지 않다가
    /// 사용자가 직접 펼치면(`rebuild_rows`) 그 행이 선택된 채 나타난다. 자동 확장(`project.auto_reveal`)은 `reveal`.
    sel_path: Option<PathBuf>,
}

const ROW_H: f32 = 22.0;
const PAD: f32 = 8.0;
const INDENT: f32 = 14.0;

impl ProjectPanel {
    pub(crate) fn new() -> Self {
        // 필터 틀(부품) + 오른쪽 부가 토글: 숨김 파일 · 점 파일(Windows만 · 사용자 09-22).
        let mut side: Vec<SideBtn> = vec![(BtnKind::Hidden, toolicons::mi_visibility)];
        if cfg!(windows) {
            side.push((BtnKind::DotFiles, toolicons::mi_dot_file));
        }
        let filter = FilterBar::new(t(Msg::PhProjectFilter), &side).with_path_toggle();
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
            scroll_x: 0,
            content_w: 0,
            rows_gen: 0,
            measured_gen: u64::MAX,
            bars: ScrollBars::new(),
            sel: None,
            caret: None,
            hover: None,
            list_rect: Rect::default(),
            header_rect: Rect::default(),
            links: Vec::new(),
            row_h: 22,
            open: None,
            command: None,
            menu: ContextMenu::new(),
            menu_row: None,
            has_recent: false,
            last_click: None,
            dblclick_ms: 400,
            tooltip_ms: 600,
            clamp_w: i32::MAX / 2,
            show_hidden: false,
            show_dot: true,
            scan_max: 0,
            scan_capped: false,
            scan: None,
            scan_threads: 4,
            scan_log: Vec::new(),
            scan_errors: 0,
            path_index: std::collections::HashMap::new(),
            filter_text: String::new(),
            icons_on: true,
            icons: HashMap::new(),
            fallback_dir: Rc::new(nexa_ctl::controls::fallback_file_icon(true)),
            fallback_file: Rc::new(nexa_ctl::controls::fallback_file_icon(false)),
            icon_version: 0,
            icon_idle_since: None,
            sel_path: None,
            open_files: Vec::new(),
            open_rect: Rect::default(),
            open_rows: Vec::new(),
            open_hover: None,
        }
    }

    /// OPEN FILES 갱신(바뀔 때만 · 줄 수가 바뀌면 다시 배치).
    pub(crate) fn set_open_files(&mut self, v: Vec<OpenFile>) -> bool {
        if v == self.open_files {
            return false;
        }
        let relayout = v.len() != self.open_files.len();
        self.open_files = v;
        if relayout {
            let (b, s) = (self.bounds, self.scale);
            self.set_bounds(b, s);
        }
        true
    }

    /// OPEN FILES 행의 × 닫기 영역(오른쪽 끝).
    fn open_close_rect(&self, r: Rect) -> Rect {
        let w = (r.h as f32 * 0.9).round() as i32;
        Rect::new(
            r.right() - w - (4.0 * self.scale).round() as i32,
            r.y,
            w,
            r.h,
        )
    }

    /// OPEN FILES 섹션 높이(헤더 1줄 + 항목 ≤ 8줄) — **프로젝트가 없어도** 보인다(사용자 09-23 "프로젝트 여부와 상관없이 열린 파일은 보여지도록").
    fn open_files_h(&self, rh: i32) -> i32 {
        if self.open_files.is_empty() {
            return 0;
        }
        rh * (1 + self.open_files.len().min(OPEN_FILES_MAX) as i32)
    }

    /// 파일/폴더 아이콘 켬/끔(설정 `project.icons` · 끄면 캐시도 비운다 = 상주 0).
    pub(crate) fn set_icons(&mut self, on: bool) {
        self.icons_on = on;
        if !on {
            self.icons.clear();
        }
    }

    /// 종류 아이콘 — 캐시 적중/도착이면 OS 아이콘, 아니면(조회 중·없음) 자체 그림(파일 대화상자와 같은 규칙 · 절대 막지 않는다).
    fn icon_for(&mut self, is_dir: bool, ext: &str) -> Rc<IconImage> {
        let key = IconKey::Kind {
            ext: ext.to_string(),
            is_dir,
        };
        if let Some(img) = self.icons.get(&key) {
            return img.clone();
        }
        let look = IconService::global().icon(&key, false);
        match look {
            Lookup::Ready(Some(ic)) => {
                let rc = Rc::new(IconImage::from_rgba(ic.w, ic.h, ic.rgba.clone()));
                self.icons.insert(key, rc.clone());
                rc
            }
            Lookup::Pending | Lookup::Ready(None) => {
                if matches!(look, Lookup::Pending) {
                    // 새 조회가 나갔다 → 유휴 시계는 결과가 다 도착한 뒤 `tick`에서 다시 시작.
                    self.icon_idle_since = None;
                }
                if is_dir {
                    self.fallback_dir.clone()
                } else {
                    self.fallback_file.clone()
                }
            }
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
        self.filter.set_tooltip_delay(ms);
    }

    /// 필터 검색어 이력 잇기(전역 · `filter.project` · 사용자 09-23).
    pub(crate) fn set_history(&mut self, h: crate::search_history::SharedHistory) {
        self.filter.set_history(h, "filter.project");
    }

    pub(crate) fn set_dblclick_ms(&mut self, ms: u128) {
        self.dblclick_ms = ms;
    }

    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
        self.filter.set_clamp_width(w);
    }

    pub(crate) fn set_list_opts(
        &mut self,
        show_hidden: bool,
        show_dot: bool,
        scan_max: usize,
        scan_threads: usize,
    ) {
        let changed = self.show_hidden != show_hidden || self.show_dot != show_dot;
        self.show_hidden = show_hidden;
        self.show_dot = show_dot;
        // 0 = 무제한 · 그 밖은 최소 100(사용자 09-23 설계 변경 = 워커 열거라 상한은 메모리 보호용).
        self.scan_max = if scan_max == 0 { 0 } else { scan_max.max(100) };
        self.scan_threads = scan_threads.min(crate::parwalk::MAX_THREADS);
        self.filter.set_checked(BtnKind::Hidden, show_hidden);
        self.filter.set_checked(BtnKind::DotFiles, show_dot);
        // 표시 규칙이 바뀌면 펼친 폴더를 다시 열거(토글 · 설정 창 · 파일 대화상자 어디서 바꿔도).
        if changed && self.name.is_some() {
            self.refresh();
        }
    }

    #[cfg(test)]
    pub(crate) fn has_project(&self) -> bool {
        self.name.is_some()
    }

    /// 프로젝트를 바꾼다(없으면 `None`) — 트리를 다시 만든다(루트만 · 펼침은 사용자가).
    pub(crate) fn set_project(
        &mut self,
        name: Option<String>,
        folders: &[PathBuf],
        selected: Option<&Path>,
    ) {
        self.name = name;
        self.scan = None;
        self.scan_capped = false;
        self.scan_errors = 0;
        self.nodes.clear();
        self.roots.clear();
        self.path_index.clear();
        self.sel = None;
        self.scroll_y = 0;
        self.scroll_x = 0;
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
            self.path_index.insert(f.clone(), i);
            self.roots.push(i);
        }
        // 루트가 하나면 바로 펼친다(한 번 더 누르지 않게).
        if self.roots.len() == 1 {
            self.expand(self.roots[0]);
        }
        self.rebuild_rows();
        // 마지막 클릭 위치 복원(프로젝트 파일 `selected` · 사용자 09-22) — 조상 펼치고 선택 · 없는 경로면 무시.
        if let Some(p) = selected {
            self.reveal(p);
        }
    }

    /// 필터 글을 넣고 바로 거른다(자체 시험 `project.filter:` · 키 주입 없이).
    pub(crate) fn set_filter_text(&mut self, text: &str) {
        self.filter.set_text(text);
        self.filter_text = text.to_string();
        if !self.filter_text.trim().is_empty() {
            self.scan_for_filter();
        } else {
            self.scan = None; // 필터를 비우면 진행 중 열거도 취소
            self.scan_capped = false;
        }
        self.sel = None;
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.rebuild_rows();
    }

    /// 활성 탭의 파일을 선택 표시(펼치지 않는다 · 스크롤 안 함) — 보이는 행이면 지금 선택 · 접혀 있으면 펼칠 때 선택돼 나타난다.
    pub(crate) fn mark_path(&mut self, p: &Path) {
        self.sel_path = Some(p.to_path_buf());
        self.sel = self.rows.iter().position(|&n| self.nodes[n].path == p);
    }

    /// 지금 선택된 항목의 경로(프로젝트 파일 `selected`에 저장).
    pub(crate) fn selected_path(&self) -> Option<PathBuf> {
        self.sel
            .and_then(|r| self.rows.get(r))
            .map(|&n| self.nodes[n].path.clone())
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
        self.set_project(name, &folders, None);
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

    /// 지금 펼쳐진 폴더들(루트 포함 · 트리 순서) — 프로젝트 파일에 저장(사용자 09-23 "좌측 기능별 복원").
    pub(crate) fn expanded_dirs(&self) -> Vec<PathBuf> {
        self.nodes
            .iter()
            .filter(|n| n.is_dir && n.expanded && n.loaded && !n.error)
            .map(|n| n.path.clone())
            .collect()
    }

    /// 저장된 펼침 상태를 되살린다 — 각 폴더까지 조상을 따라 펼친다(선택·스크롤은 건드리지 않음 · 없는 폴더는 건너뜀).
    pub(crate) fn expand_dirs(&mut self, dirs: &[PathBuf]) {
        for path in dirs {
            let root = self
                .roots
                .iter()
                .copied()
                .find(|&r| path.starts_with(&self.nodes[r].path));
            let Some(mut cur) = root else { continue };
            loop {
                self.expand(cur);
                if self.nodes[cur].path == *path {
                    break;
                }
                let next = self.nodes[cur]
                    .children
                    .iter()
                    .copied()
                    .find(|&c| self.nodes[c].is_dir && path.starts_with(&self.nodes[c].path));
                match next {
                    Some(c) => cur = c,
                    None => break,
                }
            }
        }
        self.rebuild_rows();
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
            Some(self.filter.tb_mut())
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
        // OPEN FILES(헤더 + 항목 줄) — 파일 필터 위(사용자 09-23).
        let oh = self.open_files_h(self.row_h);
        self.open_rect = Rect::new(b.x, self.header_rect.bottom() + px(2.0), b.w, oh);
        self.open_rows = (0..self.open_files.len().min(OPEN_FILES_MAX))
            .map(|i| {
                Rect::new(
                    b.x,
                    self.open_rect.y + self.row_h * (1 + i as i32),
                    b.w,
                    self.row_h,
                )
            })
            .collect();
        // 필터 위·아래 여백 = 부품 상수(네 패널 공통 · 사용자 09-22).
        let y1 = self.open_rect.bottom() + px(GAP_Y);
        self.filter.set_bounds(
            Rect::new(b.x + pad, y1, (b.w - pad * 2).max(px(80.0)), ih),
            scale,
        );
        let list_top = y1 + ih + px(GAP_Y);
        self.list_rect = Rect::new(b.x, list_top, b.w, (b.bottom() - list_top).max(0));
        self.clamp_scroll();
    }

    pub(crate) fn take_open(&mut self) -> Option<OpenReq> {
        self.open.take()
    }

    pub(crate) fn set_has_recent(&mut self, on: bool) {
        self.has_recent = on;
    }

    pub(crate) fn take_command(&mut self) -> Option<String> {
        self.command.take()
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open() || self.filter.popup_open()
    }

    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
    }

    /// 메뉴(팝업 층 · 창의 맨 마지막에).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.visible && self.menu.is_open() {
            self.menu.paint(dc, th);
        }
    }

    /// 우클릭 메뉴 열기 — **루트 노드에서만** "프로젝트에서 폴더 제거"(폴더 추가는 Project 풀다운에 · 사용자 09-22 "추가 메뉴는 필요 없다").
    fn open_menu(&mut self, p: Point) {
        let row = self.row_at(p);
        if let Some(r) = row {
            self.sel = Some(r);
        }
        self.menu_row = row;
        let is_root = row
            .and_then(|r| self.rows.get(r))
            .is_some_and(|&n| self.nodes[n].parent.is_none());
        if !is_root {
            return;
        }
        let items = vec![CtxItem::item(
            "remove_folder",
            t(Msg::MnProjectRemoveFolder),
        )];
        let text_w = (self.row_h * 10).max(180);
        // host = 아는 한 창 전체(팝업 배치 규칙 ③) — 패널은 창 높이를 모르므로 폭만 clamp · 세로는 그리는 시점의 표면 크기 안전망.
        let host = Rect::new(0, 0, self.clamp_w, i32::MAX / 4);
        self.menu.open_at(p.x, p.y, items, host, text_w);
    }

    fn menu_pick(&mut self, id: &str) {
        if id != "remove_folder" {
            return;
        }
        // 루트 노드의 순번 = 프로젝트 폴더 순번(`set_project`가 `folders` 순서대로 루트를 만든다).
        let idx = self
            .menu_row
            .and_then(|r| self.rows.get(r).copied())
            .and_then(|n| self.roots.iter().position(|&x| x == n));
        if let Some(i) = idx {
            self.command = Some(format!("project.remove_folder:{i}"));
        }
    }

    // ───────────────────────── 트리 ──────────────────

    fn list_children(&mut self, i: usize) {
        if self.nodes[i].loaded || !self.nodes[i].is_dir {
            return;
        }
        let path = self.nodes[i].path.clone();
        let listed = nexa_fs::list_opts(&path, self.show_hidden, self.show_dot);
        self.fill_children(i, listed.map_err(|e| e.to_string()));
    }

    /// 열거 결과를 노드 `i`의 자식으로 붙인다(동기 펼침과 워커 열거가 같은 길 · 폴더 먼저 · 이름순 대소문자 무시).
    fn fill_children(&mut self, i: usize, listed: Result<Vec<nexa_fs::Entry>, String>) {
        self.nodes[i].loaded = true;
        let depth = self.nodes[i].depth + 1;
        match listed {
            Ok(mut entries) => {
                entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                let mut kids = Vec::with_capacity(entries.len());
                for e in entries {
                    let k = self.nodes.len();
                    self.path_index.insert(e.path.clone(), k);
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
                self.nodes[i].error = false;
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

    /// 필터용 열거 — 아직 열거하지 않은 폴더를 **워커가 뒤에서** 병렬로 읽는다(사용자 09-23 설계 변경 · 종전 = 동기 BFS + 5,000 상한).
    /// 여기서는 열거할 폴더(트리에 있지만 `loaded`가 아닌 것)를 모아 열거를 시작만 하고, 결과는 [`Self::tick`]이 합친다.
    /// 상한(`scan_max` > 0)은 메모리 보호용 — 이미 넘겼으면 시작하지 않고 "멈춤"만 표시(새로 열거할 폴더가 없으면 표시하지 않는다 ·
    /// 사용자 09-22 "5000개 열거가 안 된 것 같은데 왜 표시되지").
    fn scan_for_filter(&mut self) {
        self.scan_capped = false;
        let mut pending: Vec<PathBuf> = Vec::new();
        let mut queue: Vec<usize> = self.roots.clone();
        let mut qi = 0;
        while qi < queue.len() {
            let i = queue[qi];
            qi += 1;
            if !self.nodes[i].loaded {
                if !self.nodes[i].error {
                    pending.push(self.nodes[i].path.clone());
                }
                continue;
            }
            for &c in &self.nodes[i].children {
                if self.nodes[c].is_dir {
                    queue.push(c);
                }
            }
        }
        if pending.is_empty() {
            self.scan = None;
            return;
        }
        if self.scan_max > 0 && self.nodes.len() >= self.scan_max {
            self.scan = None;
            self.scan_capped = true;
            return;
        }
        // 이전 열거는 버린다(Drop = 취소) · 새 열거 시작.
        self.scan = None;
        self.scan_errors = 0;
        let (rx, cancel) = crate::parwalk::spawn(
            pending,
            crate::parwalk::ListOpts {
                show_hidden: self.show_hidden,
                show_dot: self.show_dot,
            },
            self.scan_threads,
        );
        self.scan = Some(Scan {
            rx,
            cancel,
            dirs: 0,
        });
    }

    /// 워커가 보낸 폴더를 트리에 합친다(틱 · 시간 예산 안에서) — 합친 것이 있으면 true(행 다시 만들기).
    fn scan_pump(&mut self) -> bool {
        // 진행 중 열거를 잠시 꺼내 든다(트리를 고치는 동안 self를 둘로 빌리지 않게) · 끝나지 않았으면 되돌려 놓는다.
        let Some(mut scan) = self.scan.take() else {
            return false;
        };
        let t0 = std::time::Instant::now();
        let mut applied = false;
        let mut finished = false;
        let mut capped = false;
        loop {
            if t0.elapsed().as_millis() >= SCAN_APPLY_BUDGET_MS {
                break;
            }
            let msg = match scan.rx.try_recv() {
                Ok(m) => m,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            };
            match msg {
                crate::parwalk::DirMsg::Dir { path, entries } => {
                    scan.dirs += 1;
                    let Some(&i) = self.path_index.get(&path) else {
                        continue; // 트리에 없는 폴더(취소된 옛 열거의 잔여)
                    };
                    if self.nodes[i].loaded {
                        continue;
                    }
                    if let Err(e) = &entries {
                        self.scan_errors += 1;
                        self.scan_log.push((path.clone(), e.clone()));
                    }
                    self.fill_children(i, entries);
                    applied = true;
                    if self.scan_max > 0 && self.nodes.len() >= self.scan_max {
                        capped = true;
                        break;
                    }
                }
                crate::parwalk::DirMsg::Done { dirs } => {
                    scan.dirs = scan.dirs.max(dirs);
                    finished = true;
                    break;
                }
            }
        }
        if capped {
            self.scan_capped = true; // scan은 여기서 버려진다(Drop = 취소)
        } else if !finished {
            self.scan = Some(scan);
        }
        applied || finished || capped
    }

    /// 필터 열거가 진행 중인가(호스트는 `animating()`으로 틱을 계속 돌린다 · 시험용).
    #[cfg(test)]
    fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    /// 읽지 못한 폴더(경로 · 사유)를 거둔다 — 호스트가 로그 창에 쓴다(도착 순 = 병합 순).
    pub(crate) fn take_scan_log(&mut self) -> Vec<(PathBuf, String)> {
        std::mem::take(&mut self.scan_log)
    }

    /// 시험용: 열거가 끝날 때까지 틱을 돌린다(최대 10초).
    #[cfg(test)]
    fn scan_wait(&mut self) {
        let t0 = std::time::Instant::now();
        while self.scan.is_some() && t0.elapsed().as_secs() < 10 {
            if self.scan_pump() {
                self.rebuild_rows();
            } else {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }

    /// 이름이 필터에 걸리는가 — 옵션(Aa·ab·(.*))은 부품이 본다(`needle`은 종전 호출자 호환용 · 안 쓴다).
    fn matches(&self, i: usize, _needle: &str) -> bool {
        if self.filter.is_on(BtnKind::PathMatch) {
            // 경로까지 검색(기본 끔): 루트 폴더 이름부터의 상대 경로(`/` 구분)에 일반/정규식 매칭.
            self.filter.matches(&self.rel_path(i))
        } else {
            self.filter.matches(&self.nodes[i].name)
        }
    }

    /// 루트 폴더 이름부터의 상대 경로(`root/a/b.sql`).
    fn rel_path(&self, i: usize) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = Some(i);
        while let Some(n) = cur {
            parts.push(&self.nodes[n].name);
            cur = self.nodes[n].parent;
        }
        parts.reverse();
        parts.join("/")
    }

    /// 자체 시험: 필터 옵션 켜기/끄기(`project.filter:<flags>:<글>` · c/w/r/p).
    pub(crate) fn set_filter_opts(&mut self, case: bool, word: bool, regex: bool, path: bool) {
        self.filter.set_checked(BtnKind::Case, case);
        self.filter.set_checked(BtnKind::Word, word);
        self.filter.set_checked(BtnKind::Regex, regex);
        self.filter.set_checked(BtnKind::PathMatch, path);
        self.filter.refresh();
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
        self.rows_gen = self.rows_gen.wrapping_add(1);
        self.caret = None;
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
        // 활성 탭과 맞춘 선택 경로(`mark_path`)가 이제 보이면 그 행을 선택한다(사용자 선택이 없을 때만).
        if self.sel.is_none() {
            if let Some(p) = self.sel_path.clone() {
                self.sel = self.rows.iter().position(|&n| self.nodes[n].path == p);
            }
        }
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
        let mut changed = self.filter.tick(now_ms) | self.bars.tick(now_ms);
        // 필터 열거 워커의 결과 합치기(시간 예산 안 · 남은 것은 다음 틱).
        if self.scan_pump() {
            self.rebuild_rows();
            changed = true;
        }
        // 셸 아이콘 조회 결과가 도착했다(서비스 버전 변화) → 다시 그린다(파일 대화상자 `apply_icon_updates`와 같은 규칙).
        if self.icons_on {
            let svc = IconService::global();
            let v = svc.version();
            if v != self.icon_version {
                self.icon_version = v;
                changed = true;
            }
            // 조회가 다 끝난 뒤 유휴 5초 = 워커(COM 아파트먼트·셸 캐시 핸들) 회수 · 캐시(`icons`)는 남아 다시 그리는 데 조회 0.
            // 워커 스스로의 30초 회수보다 앞당긴다(패널이 열린 채 오래 머무는 동안 핸들 +80 · GDI +37을 들고 있지 않게).
            if svc.pending() == 0 {
                match self.icon_idle_since {
                    None => self.icon_idle_since = Some(now_ms),
                    Some(t) if t != u64::MAX && now_ms.saturating_sub(t) >= ICON_WORKER_IDLE_MS => {
                        svc.release_worker();
                        self.icon_idle_since = Some(u64::MAX); // 거둠 표식 — 다음 조회(`icon_for` Pending)가 `None`으로 되돌린다.
                    }
                    Some(_) => {}
                }
            }
        }
        changed
    }

    pub(crate) fn animating(&self) -> bool {
        self.visible && (self.filter.is_animating() || self.scan.is_some())
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
        if self.name.is_some() {
            let (nx, ny, consumed) = self.bars.on_event(
                ev,
                self.list_rect,
                self.content_w.max(self.list_rect.w),
                self.content_h().max(self.list_rect.h),
                self.scroll_x,
                self.scroll_y,
                self.scale,
            );
            if ny != self.scroll_y || nx != self.scroll_x {
                self.scroll_y = ny;
                self.scroll_x = nx;
                return true;
            }
            if consumed {
                return true;
            }
        }
        match *ev {
            InputEvent::Key { key, .. } => {
                if self.filter.is_focused() {
                    // 필터에 포커스: 이력(드롭다운/Flat)이 먼저 · 끝에서 ↓/Tab = 폴더 목록 첫 행으로(사용자 09-23).
                    let fe = self.filter.on_event(ev, &mut inv);
                    self.on_filter_event(fe);
                    return true;
                } else if let Some(cur) = self.sel.or(self.caret) {
                    let n = self.rows.len();
                    match key {
                        // 선택이 없으면 캐럿 행(테두리)에서부터 움직인다.
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
                // OPEN FILES 항목 클릭 = 그 탭으로(사용자 09-23) — 프로젝트가 없어도(빈 상태 분기보다 먼저).
                if let Some(k) = self.open_rows.iter().position(|r| r.contains(p)) {
                    if let Some(f) = self.open_files.get(k) {
                        // × = 그 탭 닫기(미저장이면 호스트가 묻는다) · 그 밖 = 그 탭으로.
                        let close = self.open_close_rect(self.open_rows[k]).contains(p);
                        self.command = Some(if close {
                            format!("editor.close:{}", f.id)
                        } else {
                            format!("editor.switch:{}", f.id)
                        });
                        return true;
                    }
                }
                if self.name.is_none() {
                    // 빈 상태: 링크 행만 반응.
                    if let Some((id, _)) = self.links.iter().find(|(_, r)| r.contains(p)) {
                        self.command = Some((*id).into());
                        return true;
                    }
                    return false;
                }
                // 헤더(프로젝트 이름) 클릭 = 프로젝트 전환(최근 목록 팔레트 · 사용자 09-22).
                if self.header_rect.contains(p) {
                    self.command = Some("project.switch".into());
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
                        let xs = x + self.scroll_x;
                        if dbl || (xs >= cx0 && xs < cx1) {
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
                // 빈 영역(행 아래) 클릭 = 선택 해제(사용자 09-22) — 활성 탭 동기 표시도 함께 지운다.
                if self.list_rect.contains(p) && !in_f && !self.filter.bounds().contains(p) {
                    // 배경은 지우고 테두리(캐럿)만 남긴다 — 키보드 이동의 기준점(사용자 09-23).
                    self.caret = self.sel.or(self.caret);
                    self.sel = None;
                    self.sel_path = None;
                    return true;
                }
            }
            InputEvent::RightDown { x, y } if self.name.is_some() => {
                let p = Point { x, y };
                if self.list_rect.contains(p) {
                    self.open_menu(p);
                    return true;
                }
            }
            InputEvent::MouseMove { x, y } => {
                let oh = self
                    .open_rows
                    .iter()
                    .position(|r| r.contains(Point { x, y }));
                if oh != self.open_hover {
                    self.open_hover = oh;
                }
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
            let evt = self.filter.on_event(ev, &mut inv);
            self.on_filter_event(evt);
        }
        true
    }

    /// 필터 틀의 결과 처리 — 부가 토글 = 설정을 뒤집는 명령(호스트가 `set_list_opts`로 되돌려 준다) · 글 변경 = 다시 거름 ·
    /// 이력 끝 ↓/Tab = 폴더 목록 첫 행으로 포커스(사용자 09-23).
    fn on_filter_event(&mut self, evt: FilterEvent) {
        match evt {
            FilterEvent::Side(BtnKind::Hidden) => {
                self.command = Some("project.toggle_hidden".into());
            }
            FilterEvent::Side(BtnKind::DotFiles) => {
                self.command = Some("project.toggle_dot".into());
            }
            FilterEvent::Changed => self.apply_filter(self.filter.display_text()),
            FilterEvent::LeaveDown => {
                if !self.rows.is_empty() {
                    self.filter.set_focused(false);
                    self.sel = Some(0);
                    self.ensure_visible(0);
                }
            }
            FilterEvent::Consumed | FilterEvent::None | FilterEvent::Side(_) => {}
        }
    }

    /// 필터 글이 바뀌었다(조합 중 글자 포함) — 열거·행 재구성.
    fn apply_filter(&mut self, text: String) {
        self.filter_text = text;
        if !self.filter_text.trim().is_empty() {
            self.scan_for_filter();
        } else {
            self.scan = None; // 필터를 비우면 진행 중 열거도 취소
            self.scan_capped = false;
        }
        self.sel = None;
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.rebuild_rows();
    }

    /// IME 조합·확정 뒤 호스트가 부른다 — 조합 중 글자("기")까지 바로 거른다(확장 패널과 같은 규칙 · 사용자 09-23).
    pub(crate) fn query_changed(&mut self) {
        if self.name.is_none() || !self.filter.is_focused() {
            return;
        }
        self.filter.refresh();
        let now = self.filter.display_text();
        if now != self.filter_text {
            self.apply_filter(now);
        }
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
        // OPEN FILES(사용자 09-23): 제목 줄 + 탭들(활성 = 선택 배경 · 동시 편집 칸 = 외곽선 · 미저장 = ● 앞).
        if self.open_rect.h > 0 {
            let hr = Rect::new(
                self.open_rect.x,
                self.open_rect.y,
                self.open_rect.w,
                self.row_h,
            );
            dc.select_font(FontSlot::Base, true);
            let hy = dc.text_center_y(hr.y, hr.h);
            dc.text(hr.x + pad, hy, hr, t(Msg::ProjOpenFiles), th.text_dim);
            dc.select_font(FontSlot::Base, false);
            for (k, f) in self.open_files.iter().take(OPEN_FILES_MAX).enumerate() {
                let Some(rr) = self.open_rows.get(k).copied() else {
                    break;
                };
                if f.active {
                    dc.fill_rect(rr, th.sel_bg);
                } else if f.grouped {
                    dc.stroke_round_rect(rr, 0, th.accent, 1.0);
                }
                let ty = dc.text_center_y(rr.y, rr.h);
                // 미저장 표시(사용자 09-23 정정): 점 자리를 **예약**(폭 = 작은 점 5px + 여백 · 시작 x = 머리글 "열린 파일"의 시작 x)해
                // 파일명이 늘 같은 x에 서고, 미저장이면 그 자리 가운데에 **작은 점**(글자 ●보다 작게 · 5px 원)을 그린다.
                let dot_x = hr.x + pad; // 머리글 글자와 같은 시작
                let dot_d = px(5.0);
                let dot_w = dot_d + px(6.0);
                if f.dirty {
                    dc.fill_ellipse(
                        Rect::new(dot_x, rr.y + (rr.h - dot_d) / 2, dot_d, dot_d),
                        th.text,
                    );
                }
                let label = f.title.clone();
                let more = self.open_files.len().saturating_sub(OPEN_FILES_MAX);
                let label = if k + 1 == OPEN_FILES_MAX && more > 0 {
                    format!("{label}  (+{more})")
                } else {
                    label
                };
                let cr = self.open_close_rect(rr);
                let tx = dot_x + dot_w;
                let clip = Rect::new(tx, rr.y, (cr.x - tx).max(0), rr.h);
                dc.text(tx, ty, clip, &label, th.text);
                // 머문 행 = × 닫기(오른쪽).
                if self.open_hover == Some(k) {
                    let xw = dc.text_width("\u{00d7}");
                    dc.text(cr.x + (cr.w - xw) / 2, ty, cr, "\u{00d7}", th.text_dim);
                }
            }
        }
        // 필터 틀(부품 · 프로젝트 없으면 흐린 자리 표시).
        self.filter.paint(dc, th, self.name.is_some());
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
        // 셰브론·아이콘 = 글꼴 높이(객체 탐색기와 같은 규칙 · 사용자 09-22 "객체 탐색기와 동일하게").
        let cw = dc.text_height().max(10);
        let icon_sz = dc.text_height().max(12);
        // 내용 폭 = 가장 긴 행(셰브론 + 아이콘 + 이름 + 여백) — 행 집합이 바뀐 뒤 첫 그리기에서 한 번 잰다.
        if self.measured_gen != self.rows_gen {
            let lead = if self.icons_on {
                icon_sz + px(5.0)
            } else {
                px(2.0)
            };
            let mut w = 0;
            for &node in &self.rows {
                let (_, cx1) = self.chevron_x(node);
                let tw = dc.text_width(&self.nodes[node].name);
                w = w.max(cx1 - lr.x + lead + tw + pad);
            }
            self.content_w = w;
            self.measured_gen = self.rows_gen;
            let max_x = (self.content_w - lr.w).max(0);
            self.scroll_x = self.scroll_x.clamp(0, max_x);
        }
        let sx = self.scroll_x;
        for r in first..self.rows.len() {
            if y >= lr.bottom() {
                break;
            }
            let node = self.rows[r];
            let (is_dir, expanded, empty_loaded, error, name) = {
                let n = &self.nodes[node];
                (
                    n.is_dir,
                    n.expanded,
                    n.loaded && n.children.is_empty(),
                    n.error,
                    n.name.clone(),
                )
            };
            // ★ 행은 목록 영역으로 클립(스크롤로 반쯤 올라간 첫 행이 필터 상자를 덮던 결함 · 사용자 09-22).
            let row_rect = Rect::new(lr.x, y, lr.w, rh).intersection(&lr);
            if row_rect.is_empty() {
                y += rh;
                continue;
            }
            if self.sel == Some(r) {
                dc.fill_rect(row_rect, th.sel_bg);
            } else if hover_row == Some(r) {
                dc.fill_rect_alpha(row_rect, th.text, 0.06);
            }
            if self.sel.is_none() && self.caret == Some(r) {
                dc.stroke_round_rect(row_rect, 0, th.text_dim, 1.0);
            }
            let ty = dc.text_center_y(y, rh);
            let (cx0, cx1) = self.chevron_x(node);
            let (cx0, cx1) = (cx0 - sx, cx1 - sx);
            let mut tx = cx1;
            // 셰브론 — 객체 탐색기와 같은 부품(`draw_chevron_90_in` · 읽어서 빈 폴더는 안 그림 · 접힘 = 흐림 · 펼침/호버 = 본문색).
            if is_dir && !empty_loaded {
                let chev = Rect::new(cx0, y + (rh - cw) / 2, cw, cw);
                let color = if expanded || hover_row == Some(r) {
                    th.text
                } else {
                    th.text_dim
                };
                nexa_ctl::controls::draw_chevron_90_in(dc, chev, color, expanded, Some(row_rect));
            }
            // 파일/폴더 아이콘(설정 `project.icons` · 파일 대화상자와 같은 OS 셸 아이콘 · 없으면 자체 그림).
            if self.icons_on {
                let ext = if is_dir {
                    String::new()
                } else {
                    Path::new(&name)
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default()
                };
                let img = self.icon_for(is_dir, &ext);
                let dst = Rect::new(tx, y + (rh - icon_sz) / 2, icon_sz, icon_sz);
                dc.image_scaled(dst, &img, row_rect);
                tx += icon_sz + px(5.0);
            } else {
                tx += px(2.0);
            }
            let color = if error { th.danger } else { th.text };
            let clip = Rect::new(tx, y, (lr.right() - tx).max(0), rh).intersection(&lr);
            if !clip.is_empty() {
                dc.text(tx, ty, clip, &name, color);
            }
            y += rh;
        }
        // 안내 글은 위에서부터 한 줄씩(겹치지 않게 · 사용자 09-22): 행이 없으면 "일치 없음"/"폴더 없음" 먼저, 그 아래 상한 안내.
        let mut my = if self.rows.is_empty() {
            lr.y + px(4.0)
        } else {
            y
        };
        if self.rows.is_empty() {
            let msg = if self.filter_text.trim().is_empty() {
                t(Msg::ProjNoFolders)
            } else {
                t(Msg::ProjNoMatch)
            };
            let ty = dc.text_center_y(my, rh);
            dc.text(lr.x + pad, ty, lr, msg, th.text_dim);
            my += rh;
        }
        // 상태 줄들(작은 글꼴 · 위에서부터): 열거 중 → 상한 → 읽지 못한 폴더.
        let mut notes: Vec<(String, nexa_ctl::theme::Color)> = Vec::new();
        if let Some(sc) = &self.scan {
            notes.push((tf(Msg::ProjScanning, &[&sc.dirs.to_string()]), th.text_dim));
        }
        if self.scan_capped {
            notes.push((
                tf(Msg::ProjScanCapped, &[&self.scan_max.to_string()]),
                th.warn,
            ));
        }
        if self.scan_errors > 0 {
            notes.push((
                tf(Msg::ProjScanErrors, &[&self.scan_errors.to_string()]),
                th.warn,
            ));
        }
        if !notes.is_empty() {
            dc.select_font(FontSlot::Status, false);
            for (text, color) in &notes {
                if my >= lr.bottom() {
                    break;
                }
                let ty = dc.text_center_y(my, rh);
                dc.text(lr.x + pad, ty, Rect::new(lr.x, my, lr.w, rh), text, *color);
                my += rh;
            }
            dc.select_font(FontSlot::Base, false);
        }
        self.bars.paint(
            dc,
            th,
            lr,
            self.content_w.max(lr.w),
            self.content_h().max(lr.h),
            self.scroll_x,
            self.scroll_y,
            s,
        );
    }

    /// 머문 행의 전체 경로 툴팁(팝업 층).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        if self.name.is_some() {
            self.filter.paint_popup(dc, th);
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

    fn fixture(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsql-ppanel-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/one.sql"), "x").unwrap();
        std::fs::write(dir.join("a/b/two.sql"), "y").unwrap();
        std::fs::write(dir.join("top.txt"), "z").unwrap();
        dir
    }

    /// 이력 드롭다운(사용자 09-23): 필터 상자를 클릭하면 최근 검색어 목록이 열리고 · 마지막 항목에서 ↓ 한 번 더 = 폴더 목록 첫 행으로 포커스 ·
    /// Flat 모드면 클릭해도 안 열리고 치던 글에서 ↓ = 목록으로.
    #[test]
    fn filter_history_dropdown_opens_on_click_and_leaves_to_list() {
        let dir = fixture("hist");
        let mut p = ProjectPanel::new();
        p.set_visible(true);
        p.set_project(Some("t".into()), std::slice::from_ref(&dir), None);
        p.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        let mut hist = crate::search_history::SearchHistory::new(20);
        for e in ["one", "two"] {
            hist.push("filter.project", e);
        }
        let h = hist.shared();
        p.set_history(h.clone());
        let tb = p.filter.text_bounds();
        assert!(tb.w > 0 && tb.h > 0, "상자 배치 {tb:?}");
        let (x, y) = (tb.x + 10, tb.y + tb.h / 2);
        p.set_focused(true);
        let down = InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        };
        p.on_event(&down);
        p.on_event(&InputEvent::MouseUp { x, y });
        assert!(p.filter.is_focused(), "클릭 = 필터 포커스");
        assert!(p.filter.popup_open(), "클릭 = 드롭다운 열림");
        assert!(p.menu_open());
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: false,
            primary: false,
        };
        // ↓ ×2 = 두 항목 · 한 번 더 = 닫고 목록 첫 행으로.
        p.on_event(&key(CtlKey::Down));
        p.on_event(&key(CtlKey::Down));
        assert!(p.filter.popup_open());
        p.on_event(&key(CtlKey::Down));
        assert!(!p.filter.popup_open(), "마지막에서 ↓ = 닫힘");
        assert!(!p.filter.is_focused(), "필터 포커스 해제");
        assert_eq!(p.sel, Some(0), "폴더 목록 첫 행");
        // Flat: 클릭해도 안 열림 · 치던 글(비어 있음)에서 ↓ = 목록으로.
        h.borrow_mut()
            .set_view(crate::search_history::HistoryView::Flat);
        p.on_event(&down);
        p.on_event(&InputEvent::MouseUp { x, y });
        assert!(p.filter.is_focused() && !p.filter.popup_open());
        p.on_event(&key(CtlKey::Up));
        assert_eq!(p.filter.text(), "two", "Flat ↑ = 최근");
        p.on_event(&key(CtlKey::Down));
        assert_eq!(p.filter.text(), "", "↓ = 치던 글 복귀");
        p.on_event(&key(CtlKey::Down));
        assert!(
            !p.filter.is_focused() && p.sel == Some(0),
            "한 번 더 ↓ = 목록"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_root_expands_and_filter_finds_deep_files() {
        let dir = fixture("t1");
        let mut p = ProjectPanel::new();
        p.set_project(Some("t".into()), std::slice::from_ref(&dir), None);
        // 루트 하나 = 자동 펼침 · 폴더 먼저.
        let names: Vec<&str> = p.rows.iter().map(|&n| p.nodes[n].name.as_str()).collect();
        assert_eq!(
            names,
            vec![dir.file_name().unwrap().to_str().unwrap(), "a", "top.txt"]
        );
        // 필터 = 깊이와 무관하게 찾고 조상만 보인다.
        p.filter.set_text("two");
        p.filter_text = "two".into();
        p.scan_for_filter();
        p.scan_wait(); // 워커 열거가 끝날 때까지(사용자 09-23 설계 변경)
        p.rebuild_rows();
        let names: Vec<&str> = p.rows.iter().map(|&n| p.nodes[n].name.as_str()).collect();
        assert_eq!(names[1..], ["a", "b", "two.sql"]);
        // 필터를 지우면 펼침 상태 그대로(a는 열거만 됐지 펼치지 않았다).
        p.filter.set_text("");
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
    fn set_project_restores_last_selected_path() {
        let dir = fixture("t2");
        let mut p = ProjectPanel::new();
        let target = dir.join("a/b/two.sql");
        p.set_project(Some("t".into()), std::slice::from_ref(&dir), Some(&target));
        assert_eq!(p.selected_path().as_deref(), Some(target.as_path()));
        // 없는 경로 = 선택 없음(오류 없이).
        p.set_project(
            Some("t".into()),
            std::slice::from_ref(&dir),
            Some(Path::new("/nowhere/x.sql")),
        );
        assert!(p.selected_path().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_toggle_matches_relative_path_and_cap_only_when_pending() {
        let dir = fixture("path");
        let mut p = ProjectPanel::new();
        p.set_project(Some("t".into()), std::slice::from_ref(&dir), None);
        // 이름만: "a/b"는 어떤 이름에도 없다.
        p.set_filter_text("a/b");
        p.scan_wait();
        assert!(p.rows.is_empty());
        // 경로까지: root/a/b/two.sql 에 걸린다(조상 a·b가 함께 보인다).
        p.set_filter_opts(false, false, false, true);
        p.set_filter_text("a/b");
        p.scan_wait();
        let names: Vec<&str> = p.rows.iter().map(|&n| p.nodes[n].name.as_str()).collect();
        assert_eq!(names[1..], ["a", "b", "two.sql"]);
        // 정규식 + 경로.
        p.set_filter_opts(false, false, true, true);
        p.set_filter_text(r"^t\d*/a/b/.*\.sql$");
        p.scan_wait();
        assert!(
            p.rows.is_empty(),
            "루트 이름은 임시 폴더 이름이라 ^t로 시작하지 않는다"
        );
        p.set_filter_text(r"/a/b/.*\.sql$");
        p.scan_wait();
        assert_eq!(p.rows.len(), 4);
        // 상한: 트리가 상한을 넘겨도 열거할 폴더가 남지 않았으면 "멈춤"이 아니다.
        p.scan_max = 100;
        p.set_filter_opts(false, false, false, false);
        p.set_filter_text("two");
        p.scan_wait();
        assert!(!p.scan_capped);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_cap_flags_only_when_folders_remain() {
        let dir = std::env::temp_dir().join(format!("nsql-ppanel-{}-cap", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in 0..30 {
            let sub = dir.join(format!("d{d:02}"));
            std::fs::create_dir_all(&sub).unwrap();
            for f in 0..5 {
                std::fs::write(sub.join(format!("f{f}.sql")), "x").unwrap();
            }
        }
        let mut p = ProjectPanel::new();
        p.set_project(Some("t".into()), std::slice::from_ref(&dir), None);
        p.scan_max = 100;
        p.set_filter_text("zzqq");
        assert!(p.scanning(), "워커 열거가 시작된다");
        p.scan_wait();
        assert!(p.scan_capped, "30개 폴더 중 일부는 못 열었다");
        assert!(p.rows.is_empty());
        // 상한을 넉넉히 → 전부 열거 → 멈춤 아님.
        p.scan_max = 100_000;
        p.set_filter_text("f3");
        p.scan_wait();
        assert!(!p.scan_capped);
        assert!(!p.rows.is_empty());
        assert_eq!(p.rows.len(), 1 + 30 * 2, "루트 + 폴더 30 + f3 30");
        // 다시 낮춰도 이미 다 열거했으면 멈춤이 아니다(사용자 09-22) — 열거할 폴더가 없어 워커도 뜨지 않는다.
        p.scan_max = 100;
        p.set_filter_text("f4");
        assert!(!p.scanning());
        assert!(!p.scan_capped);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★ 무제한(0) + 워커 열거: 큰 트리(200 폴더 · 1,000 파일)를 상한 없이 전부 열거 · 읽지 못한 폴더는 로그(경로 · 사유) ·
    /// 필터를 바꾸면 이전 열거는 취소되고 새 열거로(사용자 09-23 설계 변경).
    #[test]
    fn unlimited_worker_scan_lists_everything_and_logs_failures() {
        let dir = std::env::temp_dir().join(format!("nsql-ppanel-{}-big", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in 0..200 {
            let sub = dir.join(format!("g{}/d{d:03}", d % 10));
            std::fs::create_dir_all(&sub).unwrap();
            for f in 0..5 {
                std::fs::write(sub.join(format!("f{f}.sql")), "x").unwrap();
            }
        }
        let mut p = ProjectPanel::new();
        p.set_project(
            Some("t".into()),
            &[dir.clone(), dir.join("missing-root")],
            None,
        );
        p.set_list_opts(false, true, 0, 3);
        p.set_filter_text("f4");
        p.scan_wait();
        assert!(!p.scan_capped);
        // 루트 2 + g 10 + d 200 + f4 200 = 412 행(missing-root는 오류 노드로 행에 남지 않는다 — 일치 없음).
        assert_eq!(p.rows.len(), 1 + 10 + 200 + 200);
        assert_eq!(p.scan_errors, 1, "없는 루트 = 읽기 실패 1");
        let log = p.take_scan_log();
        assert_eq!(log.len(), 1);
        assert!(log[0].0.ends_with("missing-root"));
        assert!(p.take_scan_log().is_empty(), "거두면 비운다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mark_path_selects_only_when_visible() {
        let dir = fixture("mark");
        let mut p = ProjectPanel::new();
        p.set_project(Some("t".into()), std::slice::from_ref(&dir), None);
        let target = dir.join("a/b/two.sql");
        p.mark_path(&target);
        // 접혀 있다 → 선택 없음 · 펼치지 않았다.
        assert!(p.sel.is_none());
        assert_eq!(p.rows.len(), 3);
        // 사용자가 a → b를 펼치면 그 행이 선택된 채 나타난다.
        let a = p.rows[1];
        p.toggle(a);
        let b = p.nodes[a]
            .children
            .iter()
            .copied()
            .find(|&c| p.nodes[c].is_dir)
            .unwrap();
        p.toggle(b);
        assert_eq!(p.selected_path().as_deref(), Some(target.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_state_has_links_and_no_rows() {
        let mut p = ProjectPanel::new();
        p.set_project(None, &[], None);
        assert!(!p.has_project());
        assert!(p.rows.is_empty());
        // 프로젝트 없는 상태에서 행 활성화는 아무것도 내지 않는다.
        p.activate_row(0, true);
        assert!(p.take_open().is_none());
    }
}
