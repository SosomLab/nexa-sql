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
//! 키보드는 칸 경계에서 이웃 서버로 넘어간다([`cross_pane`] · mac 09-21: ↑ = 이전 서버의 마지막 행 · ↓ = 다음 서버의 첫 행 ·
//! Home/End = 전체의 첫/마지막 · PageUp/Down도 경계에서는 ↑/↓와 같다 · 타입어헤드 중에는 그 트리 안에서만).

use crate::explorer::{Explorer, ExplorerAction, LiveReq, LiveResult};
use crate::filterbar::{FilterBar, FilterEvent, GAP_Y, INPUT_H};
use crate::search_history::SharedHistory;
use crate::worker::{same_catalog, same_host, same_server};
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Key};
use nexa_ctl::{Invalidations, TextBox};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use std::sync::Arc;

/// 서버 헤더 "연결 해제"의 방식(설정 `explorer.disconnect_pick` · docs/54 §9).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DiscPick {
    /// 연결 1개면 바로 · 2개 이상이면 고르기(모두/개별).
    Auto,
    /// 1개여도 고르기.
    Always,
    /// 늘 전부(개별 해제는 연결 행 메뉴에서).
    All,
}

impl DiscPick {
    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "auto" => DiscPick::Auto,
            "always" => DiscPick::Always,
            "all" => DiscPick::All,
            _ => DiscPick::Always,
        }
    }
}

/// 헤더 메뉴에 낼 해제 항목의 모양(순수 판정 · MC/DC).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DiscMenu {
    /// 항목 하나 = 그 연결을 바로.
    Direct,
    /// 하위 메뉴 = 모두 해제 + 연결마다.
    Pick,
    /// 항목 하나 = 모두 해제(N).
    AllOnly,
}

/// 검색창 범위(설정 `explorer.filter_scope` · docs/28 §7): 전 서버 / 키보드 대상 서버만.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FilterScope {
    All,
    Shown,
}

pub(crate) fn disc_menu(mode: DiscPick, n: usize) -> DiscMenu {
    match (mode, n) {
        (DiscPick::All, _) => DiscMenu::AllOnly,
        (DiscPick::Always, _) => DiscMenu::Pick,
        (DiscPick::Auto, n) if n >= 2 => DiscMenu::Pick,
        (DiscPick::Auto, _) => DiscMenu::Direct,
    }
}

struct Pane {
    /// 카탈로그 키(방언·호스트·포트·DB — 첫 연결의 스펙 · 비밀번호 없음 · `None` = 빈 자리). 같은 카탈로그의 연결은 이 칸을 공유한다.
    key: Option<ConnectSpec>,
    /// 이 칸에 붙어 있는 연결들(계정별 · 비밀번호 없음) — 헤더 메뉴 "연결별 해제" · 루트 라벨 계정 목록 · 메타 세션 자격 후보.
    conns: Vec<ConnectSpec>,
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

/// 키보드 이동의 목적지(순수 판정 · MC/DC 표 D12). `pos` = 선택 행의 위치와 보이는 행 수(선택 없음 = `None` → 그 트리가
/// 스스로 첫 행을 고른다) · `at` = 지금 칸의 순서 · `n` = 놓인 칸 수 · `page` = 공용 뷰포트에 보이는 행 수.
/// ↑/↓는 칸 경계에서만 넘어가고, PageUp/PageDown은 **한 페이지가 칸 끝을 넘으면 남은 행 수만큼 이웃 칸 안으로**(한 트리처럼).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CrossPane {
    /// 그 트리가 스스로 처리한다.
    Stay,
    /// 지금 칸의 이 행으로(페이지 이동이 칸 안에서 끝남).
    Within(usize),
    /// 이전 서버의 마지막 행에서 `n`행 위로.
    Prev(usize),
    /// 다음 서버의 첫 행에서 `n`행 아래로.
    Next(usize),
    /// 첫 서버의 첫 행으로.
    First,
    /// 마지막 서버의 마지막 행으로.
    Last,
}

pub(crate) fn cross_pane(
    key: Key,
    pos: Option<(usize, usize)>,
    at: usize,
    n: usize,
    page: usize,
) -> CrossPane {
    use CrossPane::*;
    let has_prev = at > 0;
    let has_next = at + 1 < n;
    let page = page.max(1);
    match (key, pos) {
        (Key::Up, Some((0, _))) if has_prev => Prev(0),
        (Key::Down, Some((p, len))) if p + 1 >= len && has_next => Next(0),
        (Key::PageUp, Some((p, _))) => {
            if p >= page {
                Within(p - page)
            } else if has_prev {
                Prev(page - p - 1)
            } else {
                Within(0)
            }
        }
        (Key::PageDown, Some((p, len))) => {
            if p + page < len {
                Within(p + page)
            } else if has_next {
                Next(p + page - len)
            } else {
                Within(len.saturating_sub(1))
            }
        }
        (Key::Home, _) if has_prev => First,
        (Key::End, _) if has_next => Last,
        _ => Stay,
    }
}

pub(crate) struct ExplorerSet {
    /// 우클릭 메뉴 배치 영역(창 전체 · 0 = 탐색기 영역).
    menu_area: Rect,
    panes: Vec<Pane>,
    shown: usize,
    wake: Arc<dyn Fn() + Send + Sync>,
    visible: bool,
    icons: bool,
    sizes: bool,
    /// 노드 툴팁(`explorer.tooltip` · T-249): 커서 아래 노드가 **목표**(칸, 노드) — 머문 뒤에만 확정(nexa-ctl `LatestIntent` · 09-30
    ///   "마지막 사건만 살아남는 큐잉") · 그릴 때 목표가 아직 커서 아래인지 한 번 더 확인.
    tooltip: bool,
    tip: nexa_ctl::LatestIntent<(usize, usize)>,
    font_px: f32,
    ta_cfg: crate::explorer::TypeAheadCfg,
    /// `intel.preload`(새 서버 칸에도 적용).
    preload: bool,
    /// `intel.from_routines`(새 서버 칸에도 적용).
    routines: bool,
    /// 새 객체 강조 시간(설정 `meta.refresh_highlight_ms` · 새로 만드는 칸에도 준다).
    highlight_ms: u64,
    focused: bool,
    bounds: Rect,
    scale: f32,
    /// 마지막 마우스 위치.
    cursor: Point,
    /// 이어 붙인 트리 전체의 세로 스크롤(px) · 공용 오버레이 스크롤바.
    scroll: i32,
    /// 공용 가로 스크롤(긴 이름 · 09-19 사용자).
    scroll_x: i32,
    bars: nexa_ctl::controls::ScrollBars,
    /// ★ 서버 헤더(docs/54 §9 · 09-25): (그룹 첫 칸, 행 영역) — 배치 때 계산.
    headers: Vec<(usize, Rect)>,
    /// ★ 고정 헤더(사용자 09-28 "스크롤해도 서버 정보는 필터 아래 유지"): 스크롤로 헤더가 위로 나간 그룹의 내용이 아직 보이면
    ///   그 헤더를 영역 맨 위에 붙인다 · 다음 그룹 헤더가 올라오면 그만큼 밀려 나간다(VS Code 고정 스크롤과 같다). (그룹 첫 칸, 행 영역)
    pinned: Option<(usize, Rect)>,
    /// 서버 헤더 우클릭 메뉴(연결 해제 = 모두/개별).
    menu: CtxMenu,
    menu_group: Option<usize>,
    /// 헤더 메뉴가 만든 동작(다음 `take_actions`에 합쳐 낸다).
    pending: Vec<ExplorerAction>,
    disconnect_pick: DiscPick,
    /// 세션이 0이 된 연결을 오프라인 행으로 남길 것인가(끔 = 트리에서 뺀다 · 사용자 09-25).
    keep_offline: bool,
    gen_opts: nsql_catalog::GenOpts,
    /// 소스 열기 스키마 한정(09-30).
    source_qualify: bool,
    schema_opts: nsql_catalog::SchemaOpts,
    /// ★ 검색 인덱스 설정(84 §5 · 새 칸에도 준다).
    index_cfg: crate::explorer::IndexCfg,
    /// ★ 칸 단위(사용자 09-25 원복): 끔(기본) = **연결(계정)마다 칸·인텔리센스 분리**(권한이 달라 보이는 객체가 다르다) ·
    /// 켬 = 같은 카탈로그(방언·호스트·포트·DB)의 연결들이 칸을 공유(docs/54 §10 · 설정 `explorer.share_catalog`).
    share_catalog: bool,
    /// ★ 검색창(docs/28 §7 · 09-25): 패널 맨 위 필터 틀(Aa·ab·(.*) · 이력) — 전 서버 트리를 거른다(범위 설정).
    filter: FilterBar,
    filter_scope: FilterScope,
    /// 패널 전체(필터 + 트리) · `bounds` = 트리 영역.
    area: Rect,
}

impl ExplorerSet {
    pub(crate) fn new(wake: Arc<dyn Fn() + Send + Sync>, visible: bool) -> Self {
        let mut s = ExplorerSet {
            menu_area: Rect::default(),
            panes: Vec::new(),
            shown: 0,
            wake,
            visible,
            icons: true,
            sizes: true,
            tooltip: true,
            tip: nexa_ctl::LatestIntent::new(700),
            font_px: 17.0,
            ta_cfg: crate::explorer::TypeAheadCfg::default(),
            preload: true,
            routines: true,
            highlight_ms: 2000,
            focused: false,
            bounds: Rect::default(),
            scale: 1.0,
            cursor: Point { x: -1, y: -1 },
            scroll: 0,
            scroll_x: 0,
            bars: nexa_ctl::controls::ScrollBars::new(),
            headers: Vec::new(),
            pinned: None,
            menu: CtxMenu::new(),
            menu_group: None,
            pending: Vec::new(),
            disconnect_pick: DiscPick::Always,
            keep_offline: false,
            gen_opts: nsql_catalog::GenOpts::default(),
            source_qualify: true,
            schema_opts: nsql_catalog::SchemaOpts::default(),
            index_cfg: crate::explorer::IndexCfg::default(),
            share_catalog: false,
            filter: FilterBar::new(t(Msg::PhExplorerFilter), &[]),
            filter_scope: FilterScope::All,
            area: Rect::default(),
        };
        let p = s.new_pane(None);
        s.panes.push(p);
        s
    }

    fn new_pane(&self, key: Option<ConnectSpec>) -> Pane {
        let w = Arc::clone(&self.wake);
        let mut ex = Explorer::new(Box::new(move || w()), self.visible);
        ex.set_icons(self.icons);
        ex.set_sizes(self.sizes);
        ex.set_font_px(self.font_px);
        ex.set_typeahead(self.ta_cfg);
        ex.set_preload(self.preload);
        ex.set_routines(self.routines);
        ex.set_highlight_ms(self.highlight_ms);
        ex.set_gen_opts(self.gen_opts);
        ex.set_source_qualify(self.source_qualify);
        ex.set_schema_opts(self.schema_opts);
        ex.set_index_cfg(self.index_cfg);
        // ★ 검색어가 있는 채로 서버가 추가되면 새 칸도 바로 검색 모드(사용자 09-25) — 스키마 목록이 오는 즉시 인덱스·부분 노드로 이어진다.
        if self.filter_on() && matches!(self.filter_scope, FilterScope::All) {
            ex.apply_filter(Some(self.filter.matcher()));
        }
        Pane {
            key,
            conns: Vec::new(),
            ex,
        }
    }

    fn cur_mut(&mut self) -> &mut Explorer {
        &mut self.panes[self.shown].ex
    }

    /// 칸 열쇠 판정 — 기본 = 연결(계정) 단위 · `share_catalog`면 카탈로그 단위.
    fn same_pane(&self, k: &ConnectSpec, spec: &ConnectSpec) -> bool {
        if self.share_catalog {
            same_catalog(k, spec)
        } else {
            same_server(k, spec)
        }
    }

    /// ★ **메타를 읽을 칸**(사용자 09-30 "메타는 서버별 1벌 · 칸은 연결별"): `spec`의 칸에 메타(스키마 목록)가 있으면 그 칸 ·
    ///   비어 있으면(그 연결의 트리를 아직 안 펼침 — `explorer.share_catalog` 끔 = 계정마다 칸) **같은 카탈로그에서 메타를 가진 칸**
    ///   (수집 계정 = 그 칸의 계정 · 96 §6) · 그것도 없으면 원래 칸. 링크 판정·완성·상세·컬럼 요청이 모두 이 칸을 본다.
    fn meta_pane(&self, spec: Option<&ConnectSpec>) -> usize {
        let own = spec.and_then(|s| self.find(s)).unwrap_or(self.shown);
        let has_meta = |i: usize| self.panes.get(i).is_some_and(|p| p.ex.meta_has_schemas());
        if has_meta(own) {
            return own;
        }
        let Some(s) = spec else { return own };
        self.panes
            .iter()
            .position(|p| {
                p.key.as_ref().is_some_and(|k| same_catalog(k, s)) && p.ex.meta_has_schemas()
            })
            .unwrap_or(own)
    }

    /// ★ 그 연결의 칸 메타가 비어 있으면 루트 읽기를 시작한다(㉗-g · 링크 분석·완성이 다른 칸 스냅숏에 기대지 않게). 시작했으면 true.
    pub(crate) fn ensure_meta(&mut self, spec: Option<&ConnectSpec>, cur: Option<&str>) -> bool {
        let Some(i) = spec.and_then(|s| self.find(s)) else {
            return false;
        };
        if !self.panes[i].ex.meta_has_schemas() {
            return self.panes[i].ex.kick_meta_load();
        }
        // 스키마 목록은 있어도 **현재 스키마의 이름 버킷**이 비었을 수 있다(복원된 칸 · 유휴 선적재 전 · ㉗-h) — 비었거나 낡은 버킷만 청한다(멱등).
        if let Some(c) = cur.filter(|c| !c.is_empty()) {
            self.panes[i].ex.request_objects(c);
        }
        false
    }

    /// 진단(㉗-g): (자기 칸, 자기 칸 메타 유무, 실제로 쓴 칸).
    pub(crate) fn meta_pane_info(
        &self,
        spec: Option<&ConnectSpec>,
    ) -> (Option<usize>, bool, usize) {
        let own = spec.and_then(|s| self.find(s));
        let has = own.is_some_and(|i| self.panes[i].ex.meta_has_schemas());
        (own, has, self.meta_pane(spec))
    }

    fn find(&self, spec: &ConnectSpec) -> Option<usize> {
        self.panes
            .iter()
            .position(|p| p.key.as_ref().is_some_and(|k| self.same_pane(k, spec)))
    }

    pub(crate) fn set_share_catalog(&mut self, on: bool) {
        self.share_catalog = on;
    }

    // ── 서버 관리

    /// 세션이 이 서버에 붙었다 — 탐색기가 없으면 만들고(메타 접속), 있으면 그대로 둔다(오프라인이었으면 다시 붙인다).
    /// `show` = 이 서버의 트리를 앞으로(활성 탭의 세션일 때).
    /// `once` = `spec`의 비밀번호가 일회성(입력 창으로 받은 것)이다 — 메타 세션이 접속에만 쓰고 지운다.
    pub(crate) fn connect(&mut self, spec: &ConnectSpec, name: &str, show: bool, once: bool) {
        let mut conn = spec.clone();
        conn.password = None;
        let i = match self.find(spec) {
            Some(i) => {
                if self.panes[i].ex.is_offline() {
                    self.panes[i].ex.connect(spec, name, once);
                }
                // 같은 카탈로그의 새 계정 = 이 칸에 연결만 하나 더(트리·메타 공유 · docs/54 §10).
                if !self.panes[i].conns.iter().any(|c| same_server(c, &conn)) {
                    self.panes[i].conns.push(conn);
                }
                self.sync_users(i);
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
                self.panes[i].key = Some(conn.clone());
                self.panes[i].conns = vec![conn];
                self.panes[i].ex.connect(spec, name, once);
                self.sync_users(i);
                i
            }
        };
        if show {
            self.show(i);
        }
        // 필터 중에 붙은 새 칸도 같은 판정기로(⑩ 09-25).
        if self.filter_on() {
            self.apply_filter();
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
        let mut gone: Vec<usize> = Vec::new();
        let share = self.share_catalog;
        for (i, p) in self.panes.iter_mut().enumerate() {
            let Some(k) = p.key.clone() else { continue };
            let alive: Vec<&ConnectSpec> = live
                .iter()
                .filter(|s| {
                    if share {
                        same_catalog(&k, s)
                    } else {
                        same_server(&k, s)
                    }
                })
                .collect();
            // 연결 목록 = 살아 있는 것만(사라진 계정은 뺀다).
            let before = p.conns.len();
            p.conns.retain(|c| alive.iter().any(|l| same_server(c, l)));
            if p.conns.len() != before {
                changed = true;
                let users: Vec<String> = p.conns.iter().filter_map(|c| c.user.clone()).collect();
                p.ex.set_users(users);
            }
            let n = alive.len();
            // ★ 메타 세션의 자격이 사라졌는데 다른 연결이 남았으면 그 자격으로 다시 연다(트리 유지 · docs/54 §10).
            if n > 0 && !p.ex.is_offline() {
                let bound = p.ex.bound_user();
                let still = alive
                    .iter()
                    .any(|l| l.user.clone().unwrap_or_default() == bound);
                if !still {
                    let next = (*alive[0]).clone();
                    p.ex.rebind(&next);
                    changed = true;
                }
            }
            if pane_move(p.key.is_some(), p.ex.is_offline(), n) == PaneMove::GoOffline {
                p.ex.go_offline();
                changed = true;
                if !self.keep_offline {
                    gone.push(i);
                }
            }
        }
        // ★ 해제된 연결은 트리에서 뺀다(사용자 09-25 "C2 해제 시 그 행이 사라지게") — `explorer.keep_offline`이면 오프라인 행으로 남긴다.
        for i in gone.into_iter().rev() {
            self.remove(i);
        }
        changed
    }

    pub(crate) fn set_disconnect_pick(&mut self, mode: &str) {
        self.disconnect_pick = DiscPick::parse(mode);
    }

    pub(crate) fn set_keep_offline(&mut self, on: bool) {
        self.keep_offline = on;
    }

    pub(crate) fn set_source_qualify(&mut self, on: bool) {
        self.source_qualify = on;
        for p in &mut self.panes {
            p.ex.set_source_qualify(on);
        }
    }

    /// 그 서버 칸으로 객체 하나의 Generate SQL 요청(삭제 백업 DDL · 10-01) — 칸이 없으면 false.
    pub(crate) fn gen_object_on(
        &mut self,
        server: &ConnectSpec,
        owner: nsql_catalog::ObjectInfo,
        what: nsql_catalog::GenWhat,
        opts: nsql_catalog::GenOpts,
    ) -> bool {
        let Some(i) = self.find(server) else {
            return false;
        };
        self.panes[i].ex.gen_object(owner, what, opts);
        true
    }

    pub(crate) fn set_gen_opts(&mut self, opts: nsql_catalog::GenOpts) {
        self.gen_opts = opts;
        for p in &mut self.panes {
            p.ex.set_gen_opts(opts);
        }
    }

    /// 검색 인덱스 설정(84 §5) — 전 칸에.
    pub(crate) fn set_index_cfg(&mut self, c: crate::explorer::IndexCfg) {
        self.index_cfg = c;
        for p in &mut self.panes {
            p.ex.set_index_cfg(c);
        }
    }

    /// ★ 유휴 선적재 틱(84 §7 · 호스트 주기 호출) — 칸마다 한 걸음.
    pub(crate) fn prefetch_tick(&mut self, now: std::time::Instant) {
        for p in &mut self.panes {
            p.ex.prefetch_step(now);
        }
    }

    /// 스키마 목록 옵션(설정) — 읽어 둔 루트는 조용히 다시 읽는다.
    /// ★ 세션의 현재 DB가 바뀌었다(`USE` · 101 §3) — 그 연결의 칸에.
    pub(crate) fn set_current_db(&mut self, spec: &ConnectSpec, db: &str) {
        if let Some(i) = self.find(spec) {
            self.panes[i].ex.set_current_db(db);
        }
    }

    /// 설정 `explorer.mssql_tree`(ssms|schema) · `explorer.mssql_system_dbs`.
    pub(crate) fn set_mssql_tree(&mut self, ssms: bool, system: bool) {
        for p in &mut self.panes {
            p.ex.set_mssql_tree(ssms, system);
        }
    }

    /// 자체 시험용(101 E2E · `explorer.expand:<DB>/<라벨>`) — 보이는 칸에.
    pub(crate) fn capture_expand_label(&mut self, path: &str) -> bool {
        let i = self.shown;
        let ok = self.panes[i].ex.capture_expand_label(path);
        if ok {
            self.relayout();
        }
        ok
    }

    pub(crate) fn set_schema_opts(&mut self, opts: nsql_catalog::SchemaOpts) {
        let changed = self.schema_opts != opts;
        self.schema_opts = opts;
        for p in &mut self.panes {
            p.ex.set_schema_opts(opts);
            if changed {
                p.ex.reload_schemas();
            }
        }
    }

    fn sync_users(&mut self, i: usize) {
        let users: Vec<String> = self.panes[i]
            .conns
            .iter()
            .filter_map(|c| c.user.clone())
            .collect();
        self.panes[i].ex.set_users(users);
    }

    /// 그룹의 살아 있는 연결 전부 = (칸, 연결 인덱스).
    fn group_conns(&self, first: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for i in self.group_online(first) {
            for j in 0..self.panes[i].conns.len() {
                out.push((i, j));
            }
        }
        out
    }

    /// ★ 서버 묶음(docs/54 §9): 서버 칸을 같은 호스트(방언·호스트·포트)끼리 접속 순으로 묶는다.
    /// **파일 방언(SQLite · 호스트 없음) = 파일 하나가 서버 하나이고 거기에 연결 하나가 붙는다**(사용자 09-28 "묶음이 아니라
    /// 파일별로 서버 1개에 연결된다는 기준 · 표시 구조를 다른 서버와 동일하게") → 헤더(파일 이름) + 연결 행. 같은 파일을 다시
    /// 열면 같은 서버(`same_catalog`). 돌려주는 값 = (칸 목록, 헤더 있음) — 키 있는 칸은 늘 헤더 있음.
    fn groups(&self) -> Vec<(Vec<usize>, bool)> {
        let mut out: Vec<(Vec<usize>, bool)> = Vec::new();
        for i in 0..self.panes.len() {
            let Some(k) = &self.panes[i].key else {
                continue;
            };
            let found = out.iter().position(|(g, h)| {
                *h && self.panes[g[0]].key.as_ref().is_some_and(|k0| {
                    if k.host.is_some() {
                        same_host(k0, k)
                    } else {
                        same_catalog(k0, k)
                    }
                })
            });
            match found {
                Some(gi) => out[gi].0.push(i),
                None => out.push((vec![i], true)),
            }
        }
        out
    }

    /// 메타 세션 유휴 회수(세션 유휴 닫기와 같은 한도 · 0 = 끔).
    pub(crate) fn idle_tick(&mut self, limit_secs: u64) {
        for p in &mut self.panes {
            p.ex.suspend_if_idle(limit_secs);
            // ★ L3 회수(85 §4) — 30초 주기.
            let _ = p.ex.reclaim_meta();
        }
    }

    /// 오프라인 서버 제거(루트 우클릭 "탐색기에서 제거" · 세션 0 = 자동 제거) — 붙은 세션이 있는 서버는 지울 수 없다(메뉴에 나오지 않는다).
    /// ★ 세션 자격 금고는 건드리지 않는다(사용자 09-27 실기: DISCONNECT·전용 세션 전환으로 세션이 0이 되면 이 길을 타 비밀번호가
    ///   지워져 다음 `CONNECT user@host`가 다시 물었다 — 금고 초기화는 **빈 비밀번호 명시 · 서버 거부**뿐 · 21 §7).
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
        let keyed: Vec<usize> = self.groups().into_iter().flat_map(|(g, _)| g).collect();
        if keyed.is_empty() {
            vec![0]
        } else {
            keyed
        }
    }

    /// 서버 헤더 행 높이(그 그룹 첫 칸의 행 높이).
    fn header_h(&self, first: usize) -> i32 {
        self.panes[first].ex.row_px().max(1)
    }

    fn header_at(&self, p: Point) -> Option<usize> {
        if !self.bounds.contains(p) {
            return None;
        }
        // 고정 헤더가 먼저(그 아래 가려진 행보다 위 층).
        if let Some((first, r)) = self.pinned {
            if r.contains(p) {
                return Some(first);
            }
        }
        self.headers
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|(first, _)| *first)
    }

    /// 그룹의 온라인 연결 칸.
    fn group_online(&self, first: usize) -> Vec<usize> {
        self.groups()
            .into_iter()
            .find(|(g, _)| g.first() == Some(&first))
            .map(|(g, _)| {
                g.into_iter()
                    .filter(|&i| !self.panes[i].ex.is_offline())
                    .collect()
            })
            .unwrap_or_default()
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

    /// 용량 표시(설정 `explorer.sizes`) — 모든 칸 + 새 칸.
    pub(crate) fn set_sizes(&mut self, on: bool) {
        self.sizes = on;
        for p in &mut self.panes {
            p.ex.set_sizes(on);
        }
    }

    /// 노드 툴팁 켜고 끔(`explorer.tooltip` · T-249 · 전 칸).
    pub(crate) fn set_tooltip(&mut self, on: bool) {
        self.tooltip = on;
        self.tip.clear();
        for p in &mut self.panes {
            p.ex.set_tooltip(on);
        }
    }

    /// 노드 로드 타임아웃(`explorer.timeout` · 초 · 전 칸).
    pub(crate) fn set_load_timeout(&mut self, secs: u64) {
        for p in &mut self.panes {
            p.ex.set_load_timeout(secs);
        }
    }

    /// 타입어헤드 설정(전 칸 · 새 칸에도).
    pub(crate) fn set_typeahead(&mut self, cfg: crate::explorer::TypeAheadCfg) {
        self.ta_cfg = cfg;
        for p in &mut self.panes {
            p.ex.set_typeahead(cfg);
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
        if self.area.w > 0 {
            return self.area;
        }
        self.bounds
    }

    /// ★ 포인터 캡처·hover 이탈 판정용 **전체 영역** = 필터 막대 + 트리(사용자 09-30 "검색 상자 드래그가 밖에서 안 놓인다" —
    ///   `bounds()`는 트리만이라 필터 상자 누름이 캡처되지 않았다).
    pub(crate) fn hit_bounds(&self) -> Rect {
        let b = self.bounds();
        let f = self.filter.bounds();
        if f.w <= 0 || f.h <= 0 {
            return b;
        }
        let x = b.x.min(f.x);
        let y = b.y.min(f.y);
        let r = b.right().max(f.right());
        let bt = b.bottom().max(f.bottom());
        Rect::new(x, y, r - x, bt - y)
    }

    /// 서버 헤더 메뉴(연결 해제) — 항목 = `disc_menu(mode, n)` · n = 그룹의 살아 있는 **연결(계정)** 수(docs/54 §10).
    fn open_header_menu(&mut self, first: usize, p: Point) {
        let conns = self.group_conns(first);
        let n = conns.len();
        if n == 0 {
            return;
        }
        let conn_label = |(i, j): (usize, usize)| -> String {
            let k = &self.panes[i].conns[j];
            let db = k.database.clone().unwrap_or_default();
            let user = k.user.clone().unwrap_or_default();
            match (db.is_empty(), user.is_empty()) {
                (false, false) => format!("{db} · {user}"),
                (false, true) => db,
                (true, false) => user,
                (true, true) => t(Msg::ExpNotConnected).to_string(),
            }
        };
        let items = match disc_menu(self.disconnect_pick, n) {
            DiscMenu::Direct => vec![CtxItem::item(
                format!("disc:{}:{}", conns[0].0, conns[0].1),
                tf(Msg::ExpDisconnectOne, &[&conn_label(conns[0])]),
            )],
            DiscMenu::AllOnly => vec![CtxItem::item(
                "disc:all",
                tf(Msg::ExpDisconnectAllN, &[&n.to_string()]),
            )],
            DiscMenu::Pick => {
                let mut kids = vec![
                    CtxItem::item("disc:all", tf(Msg::ExpDisconnectAllN, &[&n.to_string()])),
                    CtxItem::Separator,
                ];
                for c in &conns {
                    kids.push(CtxItem::item(
                        format!("disc:{}:{}", c.0, c.1),
                        conn_label(*c),
                    ));
                }
                vec![CtxItem::submenu("disc", t(Msg::ExpDisconnectPick), kids)]
            }
        };
        let host = if self.menu_area.h > 0 {
            self.menu_area
        } else {
            self.bounds
        };
        // 기준 행 = 클릭한 그 헤더(★ 고정 헤더를 눌렀으면 고정 자리 — 원래 헤더 rect는 스크롤로 영역 위에 있어 메뉴가 툴바
        //   위로 올라갔다 · 사용자 09-28) · 둘 다 아니면 클릭 점.
        let row = self
            .pinned
            .filter(|(f, r)| *f == first && r.contains(p))
            .or_else(|| {
                self.headers
                    .iter()
                    .find(|(f, r)| *f == first && r.contains(p))
                    .copied()
            })
            .map_or(Rect::new(p.x, p.y, 1, 1), |(_, r)| r);
        self.menu.set_scale(self.scale);
        self.menu_group = Some(first);
        let text_w = (160.0 * self.scale).round() as i32;
        self.menu.open_beside(p.x, p.y, row, items, host, text_w);
    }

    fn header_pick(&mut self, id: &str) {
        let Some(first) = self.menu_group.take() else {
            return;
        };
        let Some(rest) = id.strip_prefix("disc:") else {
            return;
        };
        let targets: Vec<(usize, usize)> = if rest == "all" {
            self.group_conns(first)
        } else {
            match rest.split_once(':') {
                Some((a, b)) => a
                    .parse::<usize>()
                    .ok()
                    .zip(b.parse::<usize>().ok())
                    .into_iter()
                    .collect(),
                None => Vec::new(),
            }
        };
        for (i, j) in targets {
            if let Some(k) = self.panes.get(i).and_then(|p| p.conns.get(j).cloned()) {
                self.pending.push(ExplorerAction::DisconnectServer(Some(k)));
            }
        }
    }

    pub(crate) fn menu_open(&self) -> bool {
        if self.menu.is_open() || self.filter.popup_open() {
            return true;
        }
        self.panes.iter().any(|p| p.ex.menu_open())
    }

    /// 우클릭 메뉴 전부 닫기(풀다운과 배타 · 09-22).
    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
        self.menu_group = None;
        for p in &mut self.panes {
            p.ex.close_menu();
        }
    }

    /// 열린 우클릭 메뉴의 영역(없으면 빈 영역).
    pub(crate) fn menu_bounds(&self) -> Rect {
        if self.menu.is_open() {
            return self.menu.bounds();
        }
        if self.filter.popup_open() {
            return self.filter.popup_bounds();
        }
        self.panes
            .iter()
            .find(|p| p.ex.menu_open())
            .map_or(Rect::default(), |p| p.ex.menu_bounds())
    }

    /// 자체 캡처용 — 첫 서버 칸의 `row`번째 줄에서 우클릭한 것과 같은 사건을 준다(전체 영역을 거쳐 = 실제 경로).
    /// ★ 탐색기에서 객체 찾기(10-01 ㉗): 그 서버 칸으로 선택을 옮기고 단계 기계를 시작한다 — 칸이 없으면 false.
    pub(crate) fn reveal(
        &mut self,
        spec: Option<&ConnectSpec>,
        t: crate::explorer::RevealTarget,
    ) -> bool {
        let Some(i) = spec.and_then(|s| self.find(s)) else {
            return false;
        };
        self.switch_pane(i);
        self.panes[i].ex.reveal(t);
        self.after_reveal();
        true
    }

    /// 찾기가 선택을 옮겼으면 공용 스크롤을 그 행에 맞춘다(`drain` 뒤 · 시작 직후).
    fn after_reveal(&mut self) {
        let Some(i) = (0..self.panes.len()).find(|&i| self.panes[i].ex.take_reveal_done()) else {
            return;
        };
        self.relayout();
        // 선택 행을 **보이는 영역의 1/3 지점**에(사용자 10-01 "자동 이동하면 그 항목이 화면에 보여야") — 끝에 딱 맞추면 뒤이은 응답으로
        // 행 높이·상태 행이 몇 px만 바뀌어도 다시 밖으로 나갔다(두 번째 연결 칸 · 16px 모자람). 위치 = 칸 시작 + 칸 안 행 y(스크롤 0 기준).
        let Some(node) = self.panes[i].ex.selected_node() else {
            return;
        };
        let Some(y) = self.panes[i].ex.row_top_of(node) else {
            return;
        };
        let b = self.bounds;
        let content_y = self.pane_top(i) + y;
        self.scroll = (content_y - b.h / 3).max(0);
        self.bars.show();
        self.relayout();
    }

    /// 선택 노드의 라벨 경로(자체 시험 `explorer.selpath:<파일>`).
    pub(crate) fn selected_path(&self) -> String {
        // 둘째 줄 = 선택 행이 화면 안에 있는가(㉗-b · 두 번째 연결 칸의 선택이 화면 밖에 남던 결함의 판정).
        let b = self.bounds;
        let top = b.y + self.pinned.map_or(0, |(_, r)| r.h);
        let vis = self.panes[self.shown]
            .ex
            .selected_span()
            .is_some_and(|(y, h)| y >= top && y + h <= b.bottom());
        // 셋째 줄 = 진단(칸 · 공용 스크롤 · 선택 행 y/h · 영역 · 고정 헤더).
        format!(
            "{}
visible={vis}
shown={} scroll={} span={:?} bounds={:?} pinned={:?}",
            self.panes[self.shown].ex.selected_path(),
            self.shown,
            self.scroll,
            self.panes[self.shown].ex.selected_span(),
            (b.y, b.h),
            self.pinned.map(|(_, r)| r.h)
        )
    }

    /// ★ DB 용량 요청(기동 명령 `explorer.dbsizes` · 10-01 ㉕) — 보이는 칸.
    pub(crate) fn request_db_sizes(&mut self) {
        let i = self.shown;
        if let Some(p) = self.panes.get_mut(i) {
            p.ex.request_db_sizes();
        }
    }

    pub(crate) fn capture_menu(&mut self, row: usize) -> bool {
        self.panes
            .first_mut()
            .is_some_and(|p| p.ex.capture_menu(row))
    }

    /// 자체 시험용 — 첫 칸의 `row`번째 줄 펼치기.
    pub(crate) fn capture_expand(&mut self, row: usize) -> bool {
        self.panes
            .first_mut()
            .is_some_and(|p| p.ex.capture_expand(row))
    }

    /// 자체 시험용 — 첫 칸의 보이는 줄 덤프.
    pub(crate) fn dump_rows(&self) -> String {
        self.panes
            .first()
            .map(|p| p.ex.dump_rows())
            .unwrap_or_default()
    }

    /// 자체 캡처용 — 메뉴가 열린 칸에서 그 항목을 고른다.
    pub(crate) fn capture_pick(&mut self, id: &str) -> bool {
        self.panes
            .iter_mut()
            .find(|p| p.ex.menu_open())
            .is_some_and(|p| p.ex.capture_pick(id))
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.area = b;
        self.scale = scale;
        let px = |v: f32| (v * scale).round() as i32;
        let (gap, ih, pad) = (px(GAP_Y), px(INPUT_H), px(8.0));
        // 필터 틀 = 패널 맨 위(다른 패널과 같은 여백 · 폭 0이면 숨김).
        let bar_h = if b.w > 0 && b.h > 0 {
            gap + ih + gap
        } else {
            0
        };
        self.filter.set_bounds(
            Rect::new(b.x + pad, b.y + gap, (b.w - pad * 2 - 1).max(0), ih),
            scale,
        );
        self.filter.set_clamp_width((b.w - pad * 2).max(0));
        self.bounds = Rect::new(b.x, b.y + bar_h, b.w, (b.h - bar_h).max(0));
        self.relayout();
    }

    pub(crate) fn set_history(&mut self, h: SharedHistory) {
        self.filter.set_history(h, "filter.explorer");
    }

    pub(crate) fn set_tooltip_delay(&mut self, ms: u128) {
        // 노드 툴팁(T-249)도 같은 지연.
        self.tip.set_delay_ms(ms.min(u128::from(u64::MAX)) as u64);
        self.filter.set_tooltip_delay(ms);
    }

    pub(crate) fn set_filter_scope(&mut self, scope: &str) {
        self.filter_scope = if scope == "shown" {
            FilterScope::Shown
        } else {
            FilterScope::All
        };
        self.apply_filter();
    }

    /// 검색창에 포커스(⌘F/Ctrl+F가 탐색기에 있을 때 · 사용자 09-25).
    pub(crate) fn focus_filter(&mut self) {
        self.filter.set_focused(true);
        let mut inv = Invalidations::default();
        self.filter.on_event(&InputEvent::SelectAll, &mut inv);
    }

    /// 자체 시험용(기동 명령 `explorer.filter:<글>`): 필터 글을 넣고 거른다.
    pub(crate) fn set_filter_text(&mut self, q: &str) {
        self.filter.set_text(q);
        self.filter.refresh();
        self.apply_filter();
    }

    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.filter.is_focused() {
            Some(self.filter.tb_mut())
        } else {
            None
        }
    }

    /// 필터 글·옵션을 서버 트리에 반영(범위 = 전 서버 / 키보드 대상 서버).
    fn apply_filter(&mut self) {
        let m = self.filter_on().then(|| self.filter.matcher());
        for (i, p) in self.panes.iter_mut().enumerate() {
            let on = match self.filter_scope {
                FilterScope::All => true,
                FilterScope::Shown => i == self.shown,
            };
            p.ex.apply_filter(if on { m.clone() } else { None });
        }
        self.sync_search_state();
        self.relayout();
    }

    fn filter_on(&self) -> bool {
        !self.filter.display_text().trim().is_empty()
    }

    /// 이어 붙인 전체 높이.
    fn total_h(&self) -> i32 {
        let panes: i32 = self
            .laid()
            .iter()
            .map(|&i| self.panes[i].ex.content_height())
            .sum();
        let heads: i32 = self
            .groups()
            .iter()
            .filter(|(_, h)| *h)
            .map(|(g, _)| self.header_h(g[0]))
            .sum();
        panes + heads
    }

    /// 이어 붙인 전체 폭(가장 긴 행 · 그린 뒤에 안다).
    fn total_w(&self) -> i32 {
        self.laid()
            .iter()
            .map(|&i| self.panes[i].ex.content_width())
            .max()
            .unwrap_or(0)
    }

    fn relayout(&mut self) {
        let b = self.bounds;
        let s = self.scale;
        let laid = self.laid();
        self.scroll = self.scroll.clamp(0, (self.total_h() - b.h).max(0));
        self.scroll_x = self.scroll_x.clamp(0, (self.total_w() - b.w).max(0));
        let mut y = b.y - self.scroll;
        let mut headers = Vec::new();
        let menu_host = if self.menu_area.h > 0 {
            self.menu_area
        } else {
            b
        };
        // (그룹 첫 칸, 헤더 rect, 그룹 끝 y) — 고정 헤더 판정용.
        let mut spans: Vec<(usize, Rect, i32)> = Vec::new();
        for (g, hosted) in self.groups() {
            let mut head = None;
            if hosted {
                let hh = self.header_h(g[0]);
                let r = Rect::new(b.x, y, b.w, hh);
                headers.push((g[0], r));
                head = Some(r);
                y += hh;
            }
            for &i in &g {
                let ex = &mut self.panes[i].ex;
                ex.set_grouped(hosted);
                let h = ex.content_height();
                ex.set_bounds(Rect::new(b.x, y, b.w, h), s);
                ex.set_clip(b);
                ex.set_menu_host(menu_host);
                ex.set_scroll_x(self.scroll_x);
                y += h;
            }
            if let Some(r) = head {
                spans.push((g[0], r, y));
            }
        }
        self.headers = headers;
        self.pinned = Self::pinned_header(&spans, b);
        let _ = &laid;
        // 놓이지 않은 칸(빈 자리)은 영역 0.
        for i in 0..self.panes.len() {
            if !laid.contains(&i) {
                self.panes[i].ex.set_bounds(Rect::new(b.x, b.y, 0, 0), s);
            }
        }
    }

    /// ★ 고정 헤더 판정(순수 · 사용자 09-28): `spans` = 그룹마다 (첫 칸, 헤더 rect(스크롤 반영), 그룹 끝 y) 위→아래 순.
    /// 헤더가 영역 위로 나갔고(`r.y < b.y`) 그 그룹의 내용이 아직 영역에 걸려 있으면(`end > b.y`) 그 헤더를 `b.y`에 붙인다 ·
    /// 다음 그룹의 헤더가 고정 자리와 겹치면 그만큼 위로 밀린다(`min(b.y, next.y - h)`). 헤더가 아직 제자리에 보이면 None.
    fn pinned_header(spans: &[(usize, Rect, i32)], b: Rect) -> Option<(usize, Rect)> {
        let k = spans
            .iter()
            .position(|(_, r, end)| r.y < b.y && *end > b.y)?;
        let (first, r, _) = spans[k];
        let mut y = b.y;
        if let Some((_, next, _)) = spans.get(k + 1) {
            y = y.min(next.y - r.h);
        }
        let r = Rect::new(b.x, y, b.w, r.h);
        (r.bottom() > b.y).then_some((first, r))
    }

    /// 서버 헤더의 흐린 부가 글: 연결 수 · (필터 중) 일치 수 · 인덱싱 진행 · 상한 안내.
    fn header_sub(&self, first: usize) -> String {
        let n = self.group_conns(first).len();
        let mut sub = tf(Msg::ExpServerConns, &[&n.to_string()]);
        if self.filter_on() {
            let members: Vec<usize> = self
                .groups()
                .into_iter()
                .find(|(g, _)| g.first() == Some(&first))
                .map(|(g, _)| g)
                .unwrap_or_default();
            let hits: usize = members
                .iter()
                .map(|&i| self.panes[i].ex.filter_hits())
                .sum();
            sub = format!("{sub} · {}", tf(Msg::ExpFilterHits, &[&hits.to_string()]));
            // ★ 검색 중 표시(84 §1 · 사용자 09-25): 인덱스가 아직 다 안 읽혔으면 "인덱싱 n/N" · 상한에 잘렸으면 안내.
            let prog = members
                .iter()
                .filter_map(|&i| self.panes[i].ex.index_progress())
                .fold(None::<(usize, usize)>, |acc, (d, t)| {
                    Some(acc.map_or((d, t), |(ad, at)| (ad + d, at + t)))
                });
            if let Some((d, t)) = prog {
                sub = format!(
                    "{sub} · {}",
                    tf(Msg::ExpIndexing, &[&d.to_string(), &t.to_string()])
                );
            }
            if members.iter().any(|&i| self.panes[i].ex.index_truncated()) {
                sub = format!("{sub} · {}", t(Msg::ExpIndexCapped));
            }
        }
        sub
    }

    /// 키보드로 옮긴 선택이 보이도록 공용 스크롤을 맞춘다 — 고정 헤더 아래로(가려지지 않게).
    fn reveal_selection(&mut self) {
        let b = self.bounds;
        if let Some((y, h)) = self.panes[self.shown].ex.selected_span() {
            let top = b.y + self.pinned.map_or(0, |(_, r)| r.h);
            if y < top {
                self.scroll -= top - y;
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

    /// 막힘 감지(docs/56 L3) — 그 서버의 메타 세션에 요청(칸이 없거나 오프라인이면 false).
    pub(crate) fn blockers_poll(&mut self, spec: Option<&ConnectSpec>, sid: &str) -> bool {
        match spec.and_then(|s| self.find(s)) {
            Some(i) => self.panes[i].ex.blockers_poll(sid),
            None => false,
        }
    }

    pub(crate) fn take_blockers(&mut self) -> Vec<crate::explorer::BlockersResult> {
        self.panes
            .iter_mut()
            .flat_map(|p| p.ex.take_blockers())
            .collect()
    }

    pub(crate) fn take_live(&mut self) -> Vec<LiveResult> {
        self.panes
            .iter_mut()
            .flat_map(|p| p.ex.take_live())
            .collect()
    }

    /// 툴팁 목표 = 커서 아래 (칸, 노드).
    fn tip_target(&self) -> Option<(usize, usize)> {
        let p = hover_point(self.pinned, self.cursor);
        self.laid()
            .into_iter()
            .find_map(|i| self.panes[i].ex.node_at(p).map(|n| (i, n)))
    }

    /// ★ 그 칸의 **수집 계정**(메타 세션의 사용자 · 96 §6): 이름 풀이의 접근성 판정 — 세션 계정과 같을 때만 다른 스키마의 객체가 보인다.
    pub(crate) fn meta_account(&self, spec: Option<&ConnectSpec>) -> Option<String> {
        let i = self.meta_pane(spec);
        self.panes
            .get(i)
            .and_then(|p| p.key.as_ref())
            .and_then(|k| k.user.clone())
            .filter(|u| !u.is_empty())
    }

    /// 자동 완성이 읽는 메타 스냅샷(docs/76): `spec`의 칸이 있으면 그 칸 · 없으면 보이는 칸.
    pub(crate) fn meta_view(
        &self,
        spec: Option<&ConnectSpec>,
    ) -> (
        &nsql_run::meta::Interner,
        std::sync::Arc<nsql_run::meta::Snapshot>,
    ) {
        let i = self.meta_pane(spec);
        self.panes[i].ex.meta_view()
    }

    /// 자동 완성 즉시 채움 — 그 서버 칸의 메타 세션으로 컬럼 1건.
    pub(crate) fn request_columns(
        &mut self,
        spec: Option<&ConnectSpec>,
        schema: Option<&str>,
        table: &str,
        urgent: bool,
    ) {
        let i = self.meta_pane(spec);
        self.panes[i].ex.request_columns(schema, table, urgent);
    }

    /// 완성 상세 카드(09-24): 테이블 상세·컬럼을 객체 id로 요청.
    pub(crate) fn request_detail(&mut self, spec: Option<&ConnectSpec>, id: nsql_run::meta::ObjId) {
        let i = self.meta_pane(spec);
        self.panes[i].ex.request_detail(id);
        self.panes[i].ex.request_columns_by_id(id);
    }

    /// 자동 완성 즉시 채움 — 스키마(또는 사전)의 관계 객체(09-23).
    pub(crate) fn request_objects(&mut self, spec: Option<&ConnectSpec>, schema: &str) {
        let i = self.meta_pane(spec);
        self.panes[i].ex.request_objects(schema);
    }

    /// ★ 명시 메타 갱신(T-188): `spec`의 서버(없으면 보이는 칸) · `all` = 전 서버. 반환 = (표시한 버킷, 비운 객체) 합.
    pub(crate) fn refresh_meta(
        &mut self,
        spec: Option<&ConnectSpec>,
        schema: Option<&str>,
        all: bool,
    ) -> (usize, usize) {
        if all {
            let mut total = (0, 0);
            for p in &mut self.panes {
                let r = p.ex.refresh_meta(None);
                total.0 += r.0;
                total.1 += r.1;
            }
            return total;
        }
        let i = self.meta_pane(spec);
        match self.panes.get_mut(i) {
            Some(p) => p.ex.refresh_meta(schema),
            None => (0, 0),
        }
    }

    /// ★ `spec` 연결의 현재 DB(101 §3 · 툴바 작업 단위).
    pub(crate) fn current_db(&self, spec: Option<&ConnectSpec>) -> Option<String> {
        let i = spec.and_then(|s| self.find(s)).unwrap_or(self.shown);
        self.panes
            .get(i)
            .and_then(|p| p.ex.current_db().map(str::to_string))
    }

    /// 데이터베이스 목록(SQL Server · 메타 버킷) · 스키마 목록(MySQL = DB) — 툴바 작업 단위 드롭다운(10-01 ⑫).
    pub(crate) fn databases(&self, spec: Option<&ConnectSpec>) -> Vec<String> {
        let i = self.meta_pane(spec);
        self.panes
            .get(i)
            .map_or_else(Vec::new, |p| p.ex.databases())
    }

    pub(crate) fn schemas(&self, spec: Option<&ConnectSpec>) -> Vec<String> {
        let i = self.meta_pane(spec);
        self.panes.get(i).map_or_else(Vec::new, |p| p.ex.schemas())
    }

    /// `spec` 서버의 현재 스키마(서버가 말한 값).
    pub(crate) fn current_schema(&self, spec: Option<&ConnectSpec>) -> Option<String> {
        let i = self.meta_pane(spec);
        self.panes
            .get(i)
            .and_then(|p| p.ex.server_schema().map(str::to_string))
    }

    pub(crate) fn set_preload(&mut self, on: bool) {
        self.preload = on;
        for p in &mut self.panes {
            p.ex.set_preload(on);
        }
    }

    pub(crate) fn set_routines(&mut self, on: bool) {
        self.routines = on;
        for p in &mut self.panes {
            p.ex.set_routines(on);
        }
    }

    /// 전 서버의 `스키마.` 버킷 가운데 열린 문서가 안 쓰는 것을 즉시 해제(09-23).
    pub(crate) fn reclaim_intel_buckets(&mut self, used: &dyn Fn(&str) -> bool) -> usize {
        self.panes
            .iter_mut()
            .map(|p| p.ex.reclaim_intel_buckets(used))
            .sum()
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExplorerAction> {
        let mut out = std::mem::take(&mut self.pending);
        let mut remove = None;
        for (i, p) in self.panes.iter_mut().enumerate() {
            for a in p.ex.take_actions() {
                match a {
                    ExplorerAction::RemoveServer => remove = Some(i),
                    // 루트 메뉴의 서버 동작 = 이 칸의 서버 스펙을 채워 호스트로.
                    // 루트(연결 행) 메뉴 = 이 카탈로그에 붙은 연결 전부(계정마다 하나씩 · docs/54 §10).
                    ExplorerAction::DisconnectServer(_) => {
                        if p.conns.is_empty() {
                            out.push(ExplorerAction::DisconnectServer(p.key.clone()));
                        }
                        for c in &p.conns {
                            out.push(ExplorerAction::DisconnectServer(Some(c.clone())));
                        }
                    }
                    ExplorerAction::ConnectServer(_) => {
                        out.push(ExplorerAction::ConnectServer(p.key.clone()));
                    }
                    ExplorerAction::NewTabHere(_) => {
                        out.push(ExplorerAction::NewTabHere(p.key.clone()));
                    }
                    ExplorerAction::Preview { spec, r, .. } => {
                        out.push(ExplorerAction::Preview {
                            spec,
                            r,
                            server: p.key.clone(),
                        });
                    }
                    // 소스 열기 탭의 출처에 이 카탈로그의 서버를 채운다(09-30).
                    ExplorerAction::OpenSql {
                        title,
                        text,
                        origin: Some(mut o),
                    } => {
                        o.server = p.key.clone();
                        out.push(ExplorerAction::OpenSql {
                            title,
                            text,
                            origin: Some(o),
                        });
                    }
                    ExplorerAction::Import { owner, .. } => {
                        out.push(ExplorerAction::Import {
                            owner,
                            server: p.key.clone(),
                        });
                    }
                    ExplorerAction::DropObject { owner, .. } => {
                        out.push(ExplorerAction::DropObject {
                            owner,
                            server: p.key.clone(),
                        });
                    }
                    a => out.push(a),
                }
            }
        }
        if let Some(i) = remove {
            self.remove(i);
        }
        out
    }

    /// ★ Generate SQL 다시(SQL Preview "새로고침" · 83 §4) — 그 서버 칸(없으면 보이는 칸)에 같은 사양을 보낸다.
    pub(crate) fn gen_sql(
        &mut self,
        server: Option<&ConnectSpec>,
        spec: nsql_catalog::GenSpec,
    ) -> bool {
        let i = match server {
            Some(k) => self.panes.iter().position(|p| p.key.as_ref() == Some(k)),
            None => Some(self.shown),
        };
        match i.and_then(|i| self.panes.get_mut(i)) {
            Some(p) => {
                p.ex.gen_sql(spec);
                true
            }
            None => false,
        }
    }

    /// 모든 서버의 메타 응답을 반영(보이지 않는 서버도 뒤에서 읽기가 끝난다).
    pub(crate) fn drain(&mut self) -> bool {
        // 조용한 갱신(docs/57 §2-2)이 보이는 곳 **위**에 행을 넣거나 빼도 화면이 밀리지 않게: 맨 위에 걸린 (칸, 노드)를 기억했다가
        // 반영 뒤 그 노드가 같은 자리에 오도록 공용 스크롤을 맞춘다.
        let anchor = (self.scroll > 0).then(|| self.top_anchor()).flatten();
        let mut changed = false;
        for p in &mut self.panes {
            changed |= p.ex.drain();
        }
        if changed {
            if let Some((pane, node, inner)) = anchor {
                // ★ 서버 헤더 행 높이까지 더한 칸 시작 위치(09-25: 헤더를 빼고 더해 응답마다 한 행씩 위로 밀리던 결함).
                let top = self.pane_top(pane);
                if let Some(y) = self.panes[pane].ex.row_top_of(node) {
                    let want = top + y + inner;
                    if want != self.scroll {
                        self.scroll = want;
                        self.relayout();
                    }
                }
            }
            // ★ "탐색기에서 보기"가 선택을 옮겼으면 **앵커 보정 뒤에** 그 행으로 스크롤(10-01 ㉗-b · 사용자 "자동 이동하면 그 항목이
            //   화면에 보여야" — 앞에 두면 앵커 보정이 선택 행 스크롤을 도로 덮어 다른 서버 칸(두 번째 연결)의 선택이 화면 밖에 남았다).
            self.after_reveal();
        }
        changed
    }

    /// 칸의 시작 y(스크롤 0 기준 · 앞선 그룹의 헤더·칸 높이 합 + 이 그룹의 헤더).
    fn pane_top(&self, pane: usize) -> i32 {
        let mut top = 0;
        for (g, hosted) in self.groups() {
            if hosted {
                top += self.header_h(g[0]);
            }
            for i in g {
                if i == pane {
                    return top;
                }
                top += self.panes[i].ex.content_height();
            }
        }
        top
    }

    /// 공용 스크롤의 맨 위에 걸린 (칸 index, 노드, 행 안쪽 px) — 헤더 위면 그 그룹 첫 칸의 루트에 음수 안쪽(헤더 높이만큼 위).
    fn top_anchor(&self) -> Option<(usize, usize, i32)> {
        let mut top = 0;
        for (g, hosted) in self.groups() {
            if hosted {
                let hh = self.header_h(g[0]);
                if self.scroll < top + hh {
                    let (node, _) = self.panes[g[0]].ex.anchor_at(0)?;
                    return Some((g[0], node, self.scroll - (top + hh)));
                }
                top += hh;
            }
            for i in g {
                let h = self.panes[i].ex.content_height();
                if self.scroll < top + h {
                    let (node, inner) = self.panes[i].ex.anchor_at(self.scroll - top)?;
                    return Some((i, node, inner));
                }
                top += h;
            }
        }
        None
    }

    /// 실행한 DDL 반영(docs/57 T1) — 그 서버의 칸에만(없거나 오프라인이면 0).
    pub(crate) fn apply_ddl(
        &mut self,
        spec: Option<&ConnectSpec>,
        t: &nsql_core::DdlTarget,
        default_schema: Option<&str>,
    ) -> usize {
        match spec.and_then(|s| self.find(s)) {
            Some(i) => self.panes[i].ex.apply_ddl(t, default_schema),
            None => 0,
        }
    }

    /// "못 찾음" 신호(docs/57 T4).
    pub(crate) fn note_missing(
        &mut self,
        spec: Option<&ConnectSpec>,
        name: Option<&str>,
        default_schema: Option<&str>,
    ) -> usize {
        match spec.and_then(|s| self.find(s)) {
            Some(i) => self.panes[i].ex.note_missing(name, default_schema),
            None => 0,
        }
    }

    /// 유휴 워터마크(docs/57 T2) — 온라인인 모든 서버 칸에(각 칸이 스스로 거른다).
    pub(crate) fn watermark_poll(&mut self, all: bool) -> bool {
        let mut any = false;
        for p in &mut self.panes {
            any |= p.ex.watermark_poll(all);
        }
        any
    }

    /// 수동 새로 고침(docs/57 T3 · F5 / Shift+F5) — 마지막으로 누른 칸의 선택 노드.
    pub(crate) fn refresh_selected(&mut self, hard: bool) {
        self.panes[self.shown].ex.refresh_selected(hard);
    }

    pub(crate) fn set_highlight_ms(&mut self, ms: u64) {
        self.highlight_ms = ms;
        for p in &mut self.panes {
            p.ex.set_highlight_ms(ms);
        }
    }

    /// ★ 검색 진행 상태를 필터 틀에(84 §8): 검색어 없음 = Idle · 어느 칸이든 인덱스/완성 진행 = Running · 그 밖 = Done.
    fn sync_search_state(&mut self) {
        let st = if !self.filter_on() {
            crate::filterbar::SearchState::Idle
        } else if self.panes.iter().any(|p| p.ex.search_busy()) {
            crate::filterbar::SearchState::Running
        } else {
            crate::filterbar::SearchState::Done
        };
        self.filter.set_search_state(st);
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.sync_search_state();
        let mut any = self.bars.tick(now_ms) | self.filter.tick(now_ms);
        for p in &mut self.panes {
            any |= p.ex.tick(now_ms);
        }
        // 툴팁: 커서 아래 목표가 머문 뒤 확정(확정이 바뀌는 순간만 다시 그림 · T-249 · LatestIntent).
        if self.tooltip {
            any |= self.tip.tick(now_ms);
        }
        any
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.visible
            && (self.bars.is_visible()
                || self.filter.is_animating()
                || self.panes.iter().any(|p| p.ex.bars_visible()))
    }

    /// 뒤에서 할 일(L1·L2·검색 완성)이 남은 칸이 있는가 — 호스트 빠른 타이머 유지 조건(85 §2).
    pub(crate) fn background_pending(&self) -> bool {
        self.panes.iter().any(|p| p.ex.background_pending())
    }

    /// ★ 객체 상세 패널(86): 마지막으로 누른 칸의 선택 대상 · 상세 요청.
    pub(crate) fn selected_target(&self) -> Option<crate::explorer::DetailTarget> {
        self.panes.get(self.shown)?.ex.selected_target()
    }

    /// 자체 시험: 첫 칸의 `row`번째 행 선택(`explorer.select<row>`).
    pub(crate) fn capture_select(&mut self, row: usize) -> bool {
        self.shown = 0;
        self.panes
            .first_mut()
            .is_some_and(|p| p.ex.capture_select(row))
    }

    pub(crate) fn schema_kind_counts(
        &self,
        schema: &str,
    ) -> Vec<(nsql_catalog::ObjectKind, usize)> {
        self.panes
            .get(self.shown)
            .map(|p| p.ex.schema_kind_counts(schema))
            .unwrap_or_default()
    }

    /// 메타에 있는 코멘트(스키마 단위로 읽어 둔 것만 · 왕복 0).
    pub(crate) fn comments_of(
        &self,
        owner: &nsql_catalog::ObjectInfo,
    ) -> Option<nsql_run::meta::CommentsOf> {
        self.panes.get(self.shown)?.ex.comments_of(owner)
    }

    pub(crate) fn request_comments(&mut self, owner: nsql_catalog::ObjectInfo) {
        if let Some(p) = self.panes.get_mut(self.shown) {
            p.ex.request_comments(owner);
        }
    }

    pub(crate) fn request_details(
        &mut self,
        owner: nsql_catalog::ObjectInfo,
        col: Option<nsql_catalog::ColumnInfo>,
    ) {
        if let Some(p) = self.panes.get_mut(self.shown) {
            p.ex.request_details(owner, col);
        }
    }

    /// 상세 캐시(보이는 칸) — 있으면 즉시 표시용 · `true` = 다시 읽어 교체.
    pub(crate) fn cached_details(
        &mut self,
        key: &str,
    ) -> Option<(Vec<nsql_catalog::DetailSection>, bool)> {
        self.panes.get_mut(self.shown)?.ex.cached_details(key)
    }

    /// 모든 칸의 상세 무효화 범위(1회성 · T-227 후속).
    pub(crate) fn take_detail_invalidations(&mut self) -> Vec<(Option<String>, Option<String>)> {
        self.panes
            .iter_mut()
            .flat_map(|p| p.ex.take_detail_invalidations())
            .collect()
    }

    /// 진단(기동 명령 `explorer.stat:<파일>`): 칸마다 한 줄.
    pub(crate) fn stat_text(&self) -> String {
        self.panes
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{i}: {}\n", p.ex.stat_line()))
            .collect()
    }

    /// 마우스 라우팅 규칙(CLAUDE.md): 누름·휠은 **커서 아래 칸에만** · 이동은 전 칸(hover 해제용) · 키는 마지막으로 누른 칸 ·
    /// 메뉴가 열린 칸이 있으면 그 칸이 먼저(모달).
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        // ★ 검색창(docs/28 §7): 누름 = 틀 안이면 포커스(밖이면 해제) · 포커스 중 키 = 틀로(Enter = 다음 일치 · Shift+Enter = 이전 ·
        //   Esc = 지우고 트리로) · 글/옵션 변경 = 다시 거름.
        let mut inv = Invalidations::default();
        match ev {
            InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } => {
                let p = Point { x: *x, y: *y };
                let in_f = self.filter.bounds().contains(p);
                if self.filter.is_focused() != in_f && !self.filter.popup_open() {
                    self.filter.set_focused(in_f);
                }
                if in_f || self.filter.popup_open() {
                    let fe = self.filter.on_event(ev, &mut inv);
                    self.on_filter_event(fe);
                    return true;
                }
            }
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. } => {
                let fe = self.filter.on_event(ev, &mut inv);
                self.on_filter_event(fe);
                if self.filter.popup_open() {
                    return true;
                }
            }
            InputEvent::Key { key, shift, .. } if self.filter.is_focused() => {
                match key {
                    Key::Escape => {
                        self.filter.set_text("");
                        self.filter.set_focused(false);
                        self.apply_filter();
                        return true;
                    }
                    Key::Enter if !self.filter.popup_open() => {
                        let forward = !*shift;
                        let i = self.shown;
                        let f = self.focused;
                        self.panes[i].ex.set_focused(f);
                        if self.panes[i].ex.select_next_hit(forward) {
                            self.relayout();
                            self.reveal_selection();
                        }
                        return true;
                    }
                    _ => {}
                }
                let fe = self.filter.on_event(ev, &mut inv);
                self.on_filter_event(fe);
                return true;
            }
            InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo
                if self.filter.is_focused() =>
            {
                let fe = self.filter.on_event(ev, &mut inv);
                self.on_filter_event(fe);
                return true;
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } if self.filter.popup_open() => {
                let fe = self.filter.on_event(ev, &mut inv);
                self.on_filter_event(fe);
                return true;
            }
            _ => {}
        }
        // 서버 헤더 메뉴(모달 · 바깥 클릭 = 닫고 그 클릭은 그대로 진행 — 팝업 UX 규칙).
        if self.menu.is_open() {
            let outside = self.menu.is_outside_click(ev);
            let consumed = self.menu.on_event(ev) && !outside;
            if let Some(id) = self.menu.take_picked() {
                self.header_pick(&id);
            }
            if !self.menu.is_open() {
                self.menu_group = None;
            }
            if consumed || !outside {
                return true;
            }
        }
        if let Some(i) = self.panes.iter().position(|p| p.ex.menu_open()) {
            return self.panes[i].ex.on_event(ev);
        }
        // 공용 스크롤(휠 · 스크롤바 끌기)이 먼저.
        let total = self.total_h();
        let (nx, ny, consumed) = self.bars.on_event(
            ev,
            self.bounds,
            self.total_w().max(self.bounds.w),
            total.max(self.bounds.h),
            self.scroll_x,
            self.scroll,
            self.scale,
        );
        if ny != self.scroll || nx != self.scroll_x {
            self.scroll = ny;
            self.scroll_x = nx;
            self.relayout();
            // ★ 스크롤로 행이 밀리면 hover를 **지금 커서 자리**에서 다시 판정하고 툴팁 목표를 다시 잡는다(사용자 09-30 "직전 대상의
            //   툴팁이 남는다") — 목표가 바뀌면 앞 의도는 버려진다(LatestIntent).
            let p = hover_point(self.pinned, self.cursor);
            let fwd = InputEvent::MouseMove { x: p.x, y: p.y };
            for i in self.laid() {
                self.panes[i].ex.on_event(&fwd);
            }
            let target = self.tip_target();
            self.tip.set(target);
            self.tip.restart();
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
            // 서버 헤더 행: 우클릭 = 연결 해제 메뉴 · 좌클릭 = 소비만(선택 없음).
            if let Some(first) = self.header_at(p) {
                if matches!(ev, InputEvent::RightDown { .. }) {
                    self.open_header_menu(first, p);
                }
                return true;
            }
            let Some(i) = self.pane_at(p) else {
                return false;
            };
            if !matches!(ev, InputEvent::MouseUp { .. }) && i != self.shown {
                // 다른 서버의 트리를 눌렀다 — 선택·키보드 대상은 전체에 하나.
                self.switch_pane(i);
            }
            return self.panes[i].ex.on_event(ev);
        }
        match ev {
            InputEvent::MouseMove { x, y } => {
                self.cursor = Point { x: *x, y: *y };
                // ★ T-254(09-29): 고정 헤더가 덮는 자리에서는 그 아래 가려진 행에 hover를 주지 않는다(클릭은 `header_at`이 먼저 먹지만
                //   MouseMove는 칸으로 흘러갔다) → 칸에는 영역 밖의 점을 준다(있던 hover도 걷힌다).
                let p = hover_point(self.pinned, Point { x: *x, y: *y });
                let fwd = InputEvent::MouseMove { x: p.x, y: p.y };
                let mut any = false;
                for i in self.laid() {
                    any |= self.panes[i].ex.on_event(&fwd);
                }
                // 툴팁 목표 = 지금 커서 아래 노드(없으면 None → 대기 취소).
                let target = self.tip_target();
                self.tip.set(target);
                any
            }
            // (휠은 위의 공용 스크롤이 처리한다.)
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => false,
            _ => {
                if self.key_crosses_pane(ev) {
                    return true;
                }
                let r = self.cur_mut().on_event(ev);
                // 키로 선택을 옮겼으면 공용 스크롤이 따라간다(펼침·접힘으로 높이도 바뀔 수 있다).
                self.relayout();
                self.reveal_selection();
                r
            }
        }
    }

    fn on_filter_event(&mut self, fe: FilterEvent) {
        match fe {
            FilterEvent::Changed => self.apply_filter(),
            FilterEvent::LeaveDown => {
                // 이력 끝에서 ↓/Tab = 트리로(첫 일치가 있으면 그 행).
                self.filter.set_focused(false);
                let i = self.shown;
                let f = self.focused;
                self.panes[i].ex.set_focused(f);
                if !self.panes[i].ex.select_next_hit(true) {
                    self.panes[i].ex.select_visible(0);
                }
                self.relayout();
                self.reveal_selection();
            }
            FilterEvent::Consumed | FilterEvent::None | FilterEvent::Side(_) => {}
        }
    }

    /// 선택·키보드 대상을 다른 서버 칸으로(마우스 클릭 · 키보드 경계 넘기 공용) — 선택은 전체에 하나.
    fn switch_pane(&mut self, i: usize) {
        let f = self.focused;
        self.panes[self.shown].ex.set_focused(false);
        self.panes[self.shown].ex.clear_selection();
        self.shown = i;
        self.panes[i].ex.set_focused(f);
    }

    /// 키가 칸 경계를 넘거나 페이지 이동이면 세트가 목적지를 고른다([`cross_pane`]). 처리했으면 참(사건 소비).
    fn key_crosses_pane(&mut self, ev: &InputEvent) -> bool {
        let InputEvent::Key { key, .. } = ev else {
            return false;
        };
        if !self.focused || self.panes[self.shown].ex.typeahead_active() {
            return false;
        }
        let laid = self.laid();
        let Some(at) = laid.iter().position(|&i| i == self.shown) else {
            return false;
        };
        let cur = &self.panes[self.shown].ex;
        let page = (self.bounds.h / cur.row_px().max(1)).max(1) as usize;
        let (i, idx) = match cross_pane(*key, cur.selected_pos(), at, laid.len(), page) {
            CrossPane::Stay => return false,
            CrossPane::Within(k) => (self.shown, k),
            CrossPane::Prev(up) => {
                let i = laid[at - 1];
                let len = self.panes[i].ex.visible_count();
                (i, len.saturating_sub(1 + up))
            }
            CrossPane::Next(down) => (laid[at + 1], down),
            CrossPane::First => (laid[0], 0),
            CrossPane::Last => (laid[laid.len() - 1], usize::MAX),
        };
        if i != self.shown {
            self.switch_pane(i);
        }
        self.panes[i].ex.select_visible(idx);
        self.relayout();
        self.reveal_selection();
        true
    }

    /// 우클릭 메뉴가 놓일 수 있는 영역(창 전체) — 탐색기 폭에 가두면 메뉴가 누른 자리에서 왼쪽으로 밀린다(사용자 09-21
    /// "누른 자리에 최대한 가깝게"). 메뉴는 창의 팝업 층에서 그린다([`Self::paint_popups`]).
    pub(crate) fn set_menu_area(&mut self, r: Rect) {
        self.menu_area = r;
    }

    /// 팝업 층 — 창의 다른 컨트롤을 모두 그린 **뒤에** 부른다(CLAUDE.md §3 팝업 규칙).
    pub(crate) fn paint_popups(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        for p in &self.panes {
            p.ex.paint_menu(dc, th);
        }
        self.menu.paint(dc, th);
        self.filter.paint_popup(dc, th);
        // ★ 노드 툴팁 카드(`explorer.tooltip` · T-249) — 메뉴가 열려 있지 않을 때 · 창 안으로 맞춤(61 §2-2).
        if self.tooltip && !self.menu_open() {
            // 툴팁 = 확정된 목표(머문 노드)가 **지금도 커서 아래**일 때만(스크롤·구조 변경으로 어긋났으면 안 보임 · 사용자 09-30).
            let cur = hover_point(self.pinned, self.cursor);
            let settled = self.tip.settled().copied();
            if let Some((text, r)) = settled
                .filter(|&(pi, node)| {
                    self.panes
                        .get(pi)
                        .is_some_and(|p| p.ex.hover_under(cur) && p.ex.node_at(cur) == Some(node))
                })
                .and_then(|(pi, _)| self.panes[pi].ex.hover_tip())
            {
                let clamp = dc
                    .surface_size()
                    .map_or(self.area, |(w, h)| Rect::new(0, 0, w, h));
                nexa_ctl::draw::draw_tooltip_in(dc, th, r, (clamp.x, clamp.w), &text, self.scale);
            }
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        // 트리가 펼쳐지거나 접히면 칸 높이가 달라진다 → 그릴 때마다 다시 놓는다(칸 수만큼의 덧셈).
        let t0 = std::time::Instant::now();
        self.relayout();
        let t1 = std::time::Instant::now();
        dc.fill_rect(self.area, th.panel_bg);
        self.filter.paint(dc, th, true);
        let t2 = std::time::Instant::now();
        let laid = self.laid();
        for &i in &laid {
            self.panes[i].ex.paint(dc, th);
        }
        let ms = t0.elapsed().as_millis();
        if ms >= 100 {
            eprintln!(
                "[explorer] paint {ms} ms (relayout {} · filter {} · panes {})",
                (t1 - t0).as_millis(),
                (t2 - t1).as_millis(),
                t2.elapsed().as_millis()
            );
        }
        // ★ 서버 헤더(docs/54 §9): 그룹 첫 칸이 자기 글꼴·아이콘으로 그린다 · 부가 = 연결 수.
        let heads = self.headers.clone();
        for (first, r) in heads {
            let vis = r.intersection(&self.bounds);
            if vis.h <= 0 {
                continue;
            }
            let sub = self.header_sub(first);
            self.panes[first].ex.paint_server_header(dc, th, vis, &sub);
        }
        // ★ 고정 헤더(사용자 09-28): 위로 나간 그룹의 헤더를 영역 맨 위(필터 아래)에 — 다음 그룹 헤더에 밀려 나간다 · 아래 구분선.
        if let Some((first, r)) = self.pinned {
            let vis = r.intersection(&self.bounds);
            if vis.h > 0 {
                let sub = self.header_sub(first);
                self.panes[first].ex.paint_server_header(dc, th, vis, &sub);
                dc.fill_rect(Rect::new(vis.x, vis.bottom() - 1, vis.w, 1), th.border);
            }
        }
        let b = self.bounds;
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        self.bars.paint(
            dc,
            th,
            b,
            self.total_w().max(b.w),
            self.total_h().max(b.h),
            self.scroll_x,
            self.scroll,
            self.scale,
        );
        // (우클릭 메뉴는 여기서 그리지 않는다 — 창의 **팝업 층**에서 `paint_popups`로: 탐색기 밖으로 나가도 다른 컨트롤이 덮지 않게.)
        // 타입어헤드 HUD(키보드 대상 칸의 접두 · 탐색기 영역 기준 3×3 위치).
        if self.focused {
            let ex = &self.panes[self.shown].ex;
            let text = ex.typeahead_text();
            if !text.is_empty() {
                nexa_ctl::typeahead::paint_hud(
                    dc,
                    self.bounds,
                    self.scale,
                    ex.typeahead_pos(),
                    &text,
                    th,
                );
            }
        }
    }
}

/// 메모리 맵 보고(docs/80): 서버 칸마다 메타·아이콘 캐시.
impl crate::memstat::MemSource for ExplorerSet {
    fn mem_report(&self, acc: &mut crate::memstat::Acc) {
        for p in &self.panes {
            p.ex.mem_report(acc);
        }
    }
}

/// 고정 헤더가 덮는 자리의 마우스 점 → 칸에 닿지 않는 점(T-254 · 순수).
pub(crate) fn hover_point(pinned: Option<(usize, Rect)>, p: Point) -> Point {
    if pinned.is_some_and(|(_, r)| r.contains(p)) {
        Point { x: -1, y: -1 }
    } else {
        p
    }
}

#[cfg(test)]
mod tests {

    /// T-254: 고정 헤더 안의 점은 칸에 닿지 않는 점으로 바뀐다(그 아래 가려진 행에 hover 없음).
    #[test]
    fn pinned_header_blocks_hover_underneath() {
        let pinned = Some((0usize, Rect::new(0, 0, 200, 24)));
        assert_eq!(
            hover_point(pinned, Point { x: 10, y: 10 }),
            Point { x: -1, y: -1 }
        );
        assert_eq!(
            hover_point(pinned, Point { x: 10, y: 40 }),
            Point { x: 10, y: 40 }
        );
        assert_eq!(
            hover_point(None, Point { x: 10, y: 10 }),
            Point { x: 10, y: 10 }
        );
    }
    use super::*;

    /// 서버 헤더 해제 메뉴(docs/54 §9 · MC/DC): auto = 1개 바로 · 2개 이상 고르기 · always = 1개여도 고르기 · all = 늘 전부.
    #[test]
    fn disc_menu_by_mode_and_count() {
        assert_eq!(disc_menu(DiscPick::Auto, 1), DiscMenu::Direct);
        assert_eq!(disc_menu(DiscPick::Auto, 2), DiscMenu::Pick);
        assert_eq!(disc_menu(DiscPick::Always, 1), DiscMenu::Pick);
        assert_eq!(disc_menu(DiscPick::All, 3), DiscMenu::AllOnly);
        assert_eq!(disc_menu(DiscPick::All, 1), DiscMenu::AllOnly);
        assert_eq!(DiscPick::parse("always"), DiscPick::Always);
        assert_eq!(
            DiscPick::parse("nope"),
            DiscPick::Always,
            "기본 = 항상 고르기(D-144)"
        );
    }

    fn spec(host: &str, db: &str, user: &str) -> ConnectSpec {
        ConnectSpec {
            user: Some(user.into()),
            password: None,
            host: Some(host.into()),
            port: Some(1433),
            database: Some(db.into()),
            role: None,
            dialect: Some(nsql_core::Dialect::Mssql),
            schema: None,
            env: Default::default(),
        }
    }

    /// 서버 묶음(docs/54 §9): 같은 호스트의 연결 둘 = 한 그룹(헤더 1) · 다른 호스트 = 다른 그룹 · 파일 방언(호스트 없음) = 헤더 없음 ·
    /// 배치 순서 = 그룹 인접 · 세션 0 = 기본(keep_offline 끔)이면 트리에서 빠지고, 켜면 오프라인 행으로 남는다.
    #[test]
    fn groups_by_host_and_removes_disconnected() {
        let mut set = ExplorerSet::new(Arc::new(|| {}), true);
        // 기본(연결별 칸): 같은 DB의 다른 계정도 따로 칸.
        set.connect(&spec("s1", "D1", "u"), "p", false, false);
        set.connect(&spec("s1", "D1", "u2"), "p", false, false);
        assert_eq!(
            set.panes.iter().filter(|p| p.key.is_some()).count(),
            2,
            "기본 = 계정마다 칸(권한 분리)"
        );
        // 카탈로그 공유(설정 켬): 같은 카탈로그의 계정은 한 칸.
        let mut set = ExplorerSet::new(Arc::new(|| {}), true);
        set.set_share_catalog(true);
        for sp in [
            spec("s1", "D1", "u"),
            spec("s2", "X", "u"),
            spec("s1", "D2", "u"),
            spec("s1", "D1", "u2"),
        ] {
            set.connect(&sp, "p", false, false);
        }
        let g = set.groups();
        assert_eq!(g.len(), 2, "호스트 둘 = 그룹 둘");
        assert_eq!(
            g[0].0.len(),
            2,
            "s1의 D1·D2가 한 그룹(카탈로그 단위 · D1의 계정 둘은 한 칸)"
        );
        assert_eq!(set.panes[g[0].0[0]].conns.len(), 2, "D1 칸에 연결(계정) 둘");
        assert_eq!(
            set.group_conns(g[0].0[0]).len(),
            3,
            "헤더 연결 수 = 계정 셋"
        );
        assert!(g[0].1 && g[1].1, "호스트 있는 서버 = 헤더");
        let laid = set.laid();
        assert_eq!(laid.len(), 3);
        assert_eq!(laid[0], g[0].0[0]);
        assert_eq!(laid[1], g[0].0[1], "같은 서버의 연결은 인접");
        set.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        assert_eq!(set.headers.len(), 2, "그룹마다 헤더 행 하나");
        // D1의 계정 u2만 끊김 → 칸은 남고 연결만 하나 준다 · D2 세션이 0 → 기본은 트리에서 빠진다.
        let live = [spec("s1", "D1", "u"), spec("s2", "X", "u")];
        set.sync_refs(&live);
        assert_eq!(
            set.groups()[0].0.len(),
            1,
            "해제된 카탈로그(D2) 칸이 사라진다"
        );
        assert_eq!(
            set.panes[set.groups()[0].0[0]].conns.len(),
            1,
            "D1 칸은 남은 계정 하나"
        );
        // keep_offline 켬 = 남는다(오프라인).
        set.set_keep_offline(true);
        set.connect(&spec("s1", "D2", "u"), "p", false, false);
        set.sync_refs(&live);
        assert_eq!(set.groups()[0].0.len(), 2, "오프라인 행으로 남는다");
        assert_eq!(set.group_online(set.groups()[0].0[0]).len(), 1);
    }

    /// ★ 고정 헤더 판정(사용자 09-28): 제자리면 None · 위로 나가고 내용이 걸리면 영역 맨 위 · 다음 헤더가 다가오면 밀림 ·
    /// 그룹이 다 지나가면 다음 그룹 차례.
    #[test]
    fn pinned_header_follows_scroll_and_is_pushed_by_next() {
        let b = Rect::new(0, 100, 200, 300);
        let hh = 20;
        let sp = |y0: i32| {
            vec![
                (0usize, Rect::new(0, y0, 200, hh), y0 + 200),
                (1usize, Rect::new(0, y0 + 200, 200, hh), y0 + 400),
            ]
        };
        assert_eq!(
            ExplorerSet::pinned_header(&sp(100), b),
            None,
            "헤더가 제자리"
        );
        let p = ExplorerSet::pinned_header(&sp(50), b).expect("pinned");
        assert_eq!((p.0, p.1.y), (0, 100), "첫 그룹 헤더가 맨 위에 붙는다");
        // 다음 헤더 y = -85+200 = 115 → 고정 헤더는 115-20 = 95로 밀린다(5px 위로 나감).
        let p = ExplorerSet::pinned_header(&sp(-85), b).expect("pinned");
        assert_eq!((p.0, p.1.y), (0, 95), "다음 그룹 헤더에 밀린다");
        // 첫 그룹 끝(y0+200 = 100)이 영역 위와 같으면 첫 그룹은 지났다 · 둘째 헤더는 제자리(100) → None.
        assert_eq!(ExplorerSet::pinned_header(&sp(-100), b), None);
        let p = ExplorerSet::pinned_header(&sp(-150), b).expect("pinned");
        assert_eq!(p.0, 1, "둘째 그룹 차례");
        assert_eq!(p.1.y, 100);
    }

    /// ★ 파일 방언(SQLite) = 파일마다 서버 하나 + 연결 행(사용자 09-28 · 다른 서버와 같은 구조): 파일 둘 = 서버 둘 · 헤더 둘.
    #[test]
    fn sqlite_files_get_server_headers() {
        let sq = |db: &str| ConnectSpec {
            user: None,
            password: None,
            host: None,
            port: None,
            database: Some(db.into()),
            role: None,
            dialect: Some(nsql_core::Dialect::Sqlite),
            schema: None,
            env: Default::default(),
        };
        // 파일은 임시 폴더 아래에만(접속이 파일을 만든다 · 61 §2 "실제 폴더에 쓰지 않는다").
        let dir = std::env::temp_dir().join(format!("nsql-exp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let a = dir.join("a.sqlite").to_string_lossy().into_owned();
        let bf = dir.join("b.sqlite").to_string_lossy().into_owned();
        let mut set = ExplorerSet::new(Arc::new(|| {}), true);
        set.connect(&sq(&a), "A", false, false);
        set.connect(&sq(&bf), "B", false, false);
        let g = set.groups();
        assert_eq!(g.len(), 2, "파일 둘 = 서버 둘");
        assert!(g.iter().all(|(_, h)| *h), "파일 서버도 헤더 있음");
        set.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        assert_eq!(set.headers.len(), 2);
        assert!(
            set.panes[g[0].0[0]].ex.is_grouped(),
            "칸은 묶음 모드(연결 행)"
        );
        drop(set);
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    /// D12 MC/DC — 키보드 칸 경계 넘기(mac 09-21 "서버 루트에서 다른 서버로 키보드 이동 안 됨").
    /// ↑: 첫 행 **이고** 앞 칸이 있을 때만 · ↓: 마지막 행 **이고** 뒤 칸이 있을 때만 · PageUp/Down: 페이지가 칸 안이면 Within ·
    /// 넘치면 이웃 칸 안으로 남은 만큼 · 이웃이 없으면 칸 끝 · Home/End: 첫/마지막 칸이 아닐 때만 · 선택 없음·다른 키 = Stay.
    #[test]
    fn mcdc_cross_pane() {
        use CrossPane::*;
        let pg = 4;
        // ↑ — 조건 둘(첫 행 · 앞 칸 있음)을 하나씩 뒤집는다.
        assert_eq!(cross_pane(Key::Up, Some((0, 5)), 1, 3, pg), Prev(0));
        assert_eq!(
            cross_pane(Key::Up, Some((1, 5)), 1, 3, pg),
            Stay,
            "첫 행이 아니면 트리 안에서"
        );
        assert_eq!(
            cross_pane(Key::Up, Some((0, 5)), 0, 3, pg),
            Stay,
            "첫 칸이면 위가 없다"
        );
        // ↓ — 마지막 행 · 뒤 칸 있음.
        assert_eq!(cross_pane(Key::Down, Some((4, 5)), 1, 3, pg), Next(0));
        assert_eq!(cross_pane(Key::Down, Some((3, 5)), 1, 3, pg), Stay);
        assert_eq!(
            cross_pane(Key::Down, Some((4, 5)), 2, 3, pg),
            Stay,
            "마지막 칸이면 아래가 없다"
        );
        assert_eq!(
            cross_pane(Key::Down, Some((0, 1)), 0, 2, pg),
            Next(0),
            "행 하나뿐인 루트(접힌 서버)"
        );
        // PageDown — 칸 안 · 넘침(뒤 칸 있음 = 남은 만큼 안으로) · 넘침(뒤 칸 없음 = 칸 끝).
        assert_eq!(
            cross_pane(Key::PageDown, Some((0, 10)), 0, 2, pg),
            Within(4)
        );
        assert_eq!(
            cross_pane(Key::PageDown, Some((8, 10)), 0, 2, pg),
            Next(2),
            "10행 중 8 + 4 = 다음 칸 2번째"
        );
        assert_eq!(
            cross_pane(Key::PageDown, Some((9, 10)), 0, 2, pg),
            Next(3),
            "마지막 행에서도 페이지만큼"
        );
        assert_eq!(
            cross_pane(Key::PageDown, Some((8, 10)), 1, 2, pg),
            Within(9),
            "뒤 칸 없음 = 칸 끝"
        );
        assert_eq!(
            cross_pane(Key::PageDown, Some((5, 10)), 0, 2, pg),
            Within(9),
            "딱 끝에 닿으면 칸 안"
        );
        // PageUp — 대칭.
        assert_eq!(cross_pane(Key::PageUp, Some((6, 10)), 1, 2, pg), Within(2));
        assert_eq!(
            cross_pane(Key::PageUp, Some((1, 10)), 1, 2, pg),
            Prev(2),
            "1 - 4 = 앞 칸 마지막에서 2행 위"
        );
        assert_eq!(cross_pane(Key::PageUp, Some((0, 10)), 1, 2, pg), Prev(3));
        assert_eq!(
            cross_pane(Key::PageUp, Some((1, 10)), 0, 2, pg),
            Within(0),
            "앞 칸 없음 = 첫 행"
        );
        assert_eq!(
            cross_pane(Key::PageUp, Some((4, 10)), 1, 2, pg),
            Within(0),
            "딱 첫 행에 닿으면 칸 안"
        );
        assert_eq!(
            cross_pane(Key::PageDown, Some((0, 3)), 0, 2, 0),
            Within(1),
            "페이지 0은 1로"
        );
        // Home/End — 칸 위치만 본다.
        assert_eq!(cross_pane(Key::Home, Some((3, 5)), 2, 3, pg), First);
        assert_eq!(
            cross_pane(Key::Home, Some((3, 5)), 0, 3, pg),
            Stay,
            "첫 칸의 Home = 그 트리의 첫 행"
        );
        assert_eq!(
            cross_pane(Key::Home, None, 2, 3, pg),
            First,
            "선택이 없어도 첫 서버로"
        );
        assert_eq!(cross_pane(Key::End, Some((0, 5)), 0, 3, pg), Last);
        assert_eq!(cross_pane(Key::End, Some((0, 5)), 2, 3, pg), Stay);
        // 선택 없음 · 다른 키 · 칸 하나.
        assert_eq!(
            cross_pane(Key::Up, None, 1, 3, pg),
            Stay,
            "선택이 없으면 트리가 첫 행을 고른다"
        );
        assert_eq!(cross_pane(Key::Down, None, 1, 3, pg), Stay);
        assert_eq!(cross_pane(Key::PageDown, None, 1, 3, pg), Stay);
        assert_eq!(cross_pane(Key::Right, Some((0, 5)), 1, 3, pg), Stay);
        assert_eq!(cross_pane(Key::Left, Some((0, 5)), 1, 3, pg), Stay);
        assert_eq!(
            cross_pane(Key::Down, Some((4, 5)), 0, 1, pg),
            Stay,
            "서버 하나"
        );
        assert_eq!(cross_pane(Key::End, Some((0, 5)), 0, 1, pg), Stay);
    }

    /// ★ 필터 상자 드래그가 트리·바깥에서 놓여도 끝난다(사용자 09-30 "검색 내용에 마우스가 Release 되지 않는 경우").
    #[test]
    fn filter_drag_released_outside_ends() {
        let mut set = ExplorerSet::new(Arc::new(|| {}), true);
        set.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        set.filter.set_text("abcdefghij");
        let fb = set.filter.bounds();
        assert!(fb.w > 0 && fb.h > 0, "필터 상자가 있어야 한다: {fb:?}");
        let y = fb.y + fb.h / 2;
        let down = |x: i32, y: i32| InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        };
        set.on_event(&down(fb.x + 6, y));
        assert!(set.filter.is_focused());
        assert!(
            set.focused_textbox().is_some_and(|tb| tb.is_dragging()),
            "누르면 드래그 시작"
        );
        set.on_event(&InputEvent::MouseMove {
            x: fb.x + 60,
            y: y + 200,
        });
        assert!(set.focused_textbox().is_some_and(|tb| tb.is_dragging()));
        // 트리 영역(상자 밖)에서 놓는다 → 드래그 끝.
        set.on_event(&InputEvent::MouseUp {
            x: fb.x + 60,
            y: y + 200,
        });
        assert!(
            set.focused_textbox().is_some_and(|tb| !tb.is_dragging()),
            "밖에서 놓아도 드래그가 끝나야 한다"
        );
    }

    /// ★ 상자 클릭으로 이력 드롭다운이 열린 채 드래그 → 밖에서 놓아도 드래그가 끝나고 드롭다운은 닫힌다(사용자 09-30 재현 경로).
    #[test]
    fn filter_drag_with_history_dropdown_ends() {
        let mut set = ExplorerSet::new(Arc::new(|| {}), true);
        set.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        let mut h = crate::search_history::SearchHistory::new(20);
        h.push("explorer", "old query");
        set.filter.set_history(h.shared(), "explorer");
        set.filter.set_text("abcdefghij");
        let fb = set.filter.bounds();
        let y = fb.y + fb.h / 2;
        set.on_event(&InputEvent::MouseDown {
            x: fb.x + 6,
            y,
            shift: false,
            primary: false,
        });
        assert!(set.focused_textbox().is_some_and(|tb| tb.is_dragging()));
        set.on_event(&InputEvent::MouseMove {
            x: fb.x + 80,
            y: y + 300,
        });
        set.on_event(&InputEvent::MouseUp {
            x: fb.x + 80,
            y: y + 300,
        });
        assert!(
            set.focused_textbox().is_some_and(|tb| !tb.is_dragging()),
            "드롭다운이 열려 있어도 놓임은 상자에 닿아야 한다"
        );
        assert!(
            !set.filter.popup_open(),
            "드래그를 끝낸 놓임은 드롭다운 선택이 아니다"
        );
    }

    /// ★ 단순 클릭(누르고 같은 자리에서 놓음)은 이력 드롭다운을 연 채 둔다(사용자 09-30 "클릭하면 드롭박스가 잠깐 떴다 사라짐").
    #[test]
    fn filter_click_keeps_history_dropdown_open() {
        let mut set = ExplorerSet::new(Arc::new(|| {}), true);
        set.set_bounds(Rect::new(0, 0, 300, 600), 1.0);
        let mut h = crate::search_history::SearchHistory::new(20);
        h.push("explorer", "old query");
        set.filter.set_history(h.shared(), "explorer");
        // 빈 글 = 이력 전부가 드롭다운에(글이 있으면 그 글이 든 항목만).
        set.filter.set_text("");
        let fb = set.filter.bounds();
        let (x, y) = (fb.x + 6, fb.y + fb.h / 2);
        set.on_event(&InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        });
        assert!(set.filter.popup_open(), "클릭 = 드롭다운 열림");
        set.on_event(&InputEvent::MouseUp { x, y });
        assert!(set.filter.popup_open(), "같은 자리 놓임에 닫히면 안 된다");
        assert!(set.focused_textbox().is_some_and(|tb| !tb.is_dragging()));
    }
}
