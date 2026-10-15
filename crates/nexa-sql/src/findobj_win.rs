//! ★ **객체 찾기 창**(docs/113 · 사용자 10-15): 모달리스 · 메인 소유 · 창 **전용 엔진**(`findobj_engine` · 탐색기 메타 스레드와 분리 · D-295 실시간 직접 조회)이
//! 서버 × 스키마 × 단계로 잘게 쪼갠 작업을 병렬로 돌리고, 이 창은 작업이 끝날 때마다 결과를 **합쳐**(객체 하나 = 한 행) 보여 준다.
//! 조건 1행(검색어 + 안쪽 토글 Aa · ab · (.*) = 필터 틀 `FindBtn` · 검색/중지) · 범위(서버 · ★ 전 DB(SQL Server) · 스키마) · 대상 4 · 종류 4 · ★ 진행 패널(혜성 링 + 서버/스키마/작업 · 조사 행 · 객체/일치 · 시작·종료·경과) ·
//! ★ 결과 거르기(글 + 일치 종류 4 · 뷰 투영) · 머리 클릭 정렬 · 미리보기(일치 줄 · 강조) · 더블클릭/Enter = 소스 탭 + 일치 줄 · 우클릭 = 탐색기 메뉴 부분집합.
use crate::exp_icons::{self, IconKind};
use crate::findbar::{BtnKind, FindBtn};
use crate::findobj_engine::{EngineMsg, FindHit, FindQuery, FindWhat};
use crate::toolicons;
use nexa_ctl::controls::combo::{Combo, ComboItem};
use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::controls::{Button, Checkbox, ComboControl};
use nexa_ctl::draw::{draw_tooltip_in, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, IconImage, Theme};
use nexa_ctl::{
    BusyRing, BusyState, Control, InputEvent, Invalidations, ScrollBars, TextBox, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_catalog::find::FindPat;
use nsql_catalog::{ObjectInfo, ObjectKind};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Instant, SystemTime};
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 호스트 동작.
#[derive(Clone, Debug)]
pub(crate) enum FoAction {
    None,
    Paint,
    Close,
    /// 검색 시작(조건 · 서버 하나 또는 전체) — 호스트가 엔진에 넘기고 `started(n)`으로 서버 수를 돌려준다.
    Search(Box<FindQuery>, Option<ConnectSpec>),
    Stop,
    OpenSource {
        server: Option<ConnectSpec>,
        obj: ObjectInfo,
        line: u32,
    },
    Reveal {
        server: Option<ConnectSpec>,
        obj: ObjectInfo,
    },
    Copy(String),
    GenSelect {
        server: Option<ConnectSpec>,
        obj: ObjectInfo,
    },
}

/// 결과 한 행 = 객체 하나(여러 일치는 안에 모은다).
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub server: Option<ConnectSpec>,
    pub label: String,
    pub db: Option<String>,
    pub schema: String,
    pub name: String,
    pub kind: ObjectKind,
    pub name_hit: bool,
    pub comment: Option<String>,
    pub cols: Vec<(String, String)>,
    pub src: Vec<(u32, String)>,
}

type RowKey = (String, String, String, String, ObjectKind);

impl Row {
    fn key(&self) -> RowKey {
        (
            self.label.clone(),
            self.db.clone().unwrap_or_default(),
            self.schema.to_lowercase(),
            self.name.to_lowercase(),
            self.kind,
        )
    }

    fn obj(&self) -> ObjectInfo {
        ObjectInfo {
            db: self.db.clone().unwrap_or_default(),
            schema: self.schema.clone(),
            name: self.name.clone(),
            kind: self.kind,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        }
    }

    fn hit_count(&self) -> usize {
        usize::from(self.name_hit)
            + usize::from(self.comment.is_some())
            + self.cols.len()
            + self.src.len()
    }

    /// "일치" 열 글(이름 · 설명 · 컬럼 N · 소스 N줄).
    fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.name_hit {
            parts.push(t(Msg::FoMatchName).to_string());
        }
        if self.comment.is_some() {
            parts.push(t(Msg::FoMatchComment).to_string());
        }
        if !self.cols.is_empty() {
            parts.push(tf(Msg::FoMatchColumns, &[&self.cols.len().to_string()]));
        }
        if !self.src.is_empty() {
            parts.push(tf(Msg::FoMatchSource, &[&self.src.len().to_string()]));
        }
        parts.join(" · ")
    }

    /// ★ 거르기 건초(소문자 · 10-16 사용자 "거르기 동작 모호"): 화면의 모든 열 + 컬럼 이름 + 설명 — 종류 · DB · 스키마 · 이름 · 서버 · 일치 요약.
    fn hay_lower(&self) -> String {
        let cols: Vec<&str> = self.cols.iter().map(|(c, _)| c.as_str()).collect();
        format!(
            "{}
{}
{}
{}
{}
{}
{}
{}",
            self.kind.code(),
            self.db.as_deref().unwrap_or(""),
            self.schema,
            self.name,
            self.label,
            self.summary(),
            cols.join(
                "
"
            ),
            self.comment.as_deref().unwrap_or("")
        )
        .to_lowercase()
    }

    /// 전체 이름(인용 없음 · SQL Server `DB.스키마.이름`).
    fn full_name(&self) -> String {
        let mut v: Vec<&str> = Vec::new();
        if let Some(db) = self.db.as_deref().filter(|d| !d.is_empty()) {
            v.push(db);
        }
        if !self.schema.is_empty() {
            v.push(&self.schema);
        }
        v.push(&self.name);
        v.join(".")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Query,
    Schema,
    Filter,
    List,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SortKey {
    Kind,
    Name,
    Schema,
    Server,
    Hits,
}

/// 토글 툴팁 머무름(ms · 필터 틀과 같은 값).
const TOOLTIP_MS: u128 = 600;

/// 미리보기 한 줄 = (줄 번호 · 글 · 강조 구간들(바이트)).
struct PreLine {
    no: Option<u32>,
    text: String,
    spans: Vec<(usize, usize)>,
}

/// 진행(작업 단위 · 113 §3 진행 패널).
#[derive(Default)]
struct Progress {
    servers_total: usize,
    servers_planned: usize,
    servers_done: usize,
    schemas: usize,
    tasks_total: usize,
    tasks_done: usize,
    scanned: usize,
    hits: usize,
    started_at: Option<SystemTime>,
    ended_at: Option<SystemTime>,
    started: Option<Instant>,
    elapsed_ms: u128,
    notes: Vec<String>,
    errors: Vec<String>,
    /// 서버마다 계획/완료 표식(메시지 중복 방어 · 차례 = 엔진 `server`).
    planned: Vec<bool>,
    done: Vec<bool>,
    /// 마지막으로 끝난 작업(서버 · 스키마 · 단계 · ms) — 진행 둘째 줄.
    last_task: Option<String>,
}

pub(crate) struct FindObjWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    memo: crate::wingeom::Memo,
    last: Option<((i32, i32), (f64, f64))>,
    epoch: Instant,
    // 조건
    query: TextBox,
    schema: TextBox,
    /// 검색어 상자 안쪽 토글 Aa · ab · (.*) = 객체 탐색기 필터 틀과 같은 `FindBtn`(사용자 10-15 "체크박스가 아닌 토글 방식").
    opts: Vec<FindBtn>,
    /// ★ SQL Server 전 DB 검색(설정 `findobj.all_dbs` · 탐색기 검색 설정과 독립 · 사용자 10-15).
    all_dbs: Checkbox,
    run_btn: Button,
    server: Combo,
    servers: Vec<(Option<ConnectSpec>, String)>,
    /// 이번 검색이 엔진에 넘긴 서버(엔진 `server` 번호 = 이 차례).
    run_specs: Vec<ConnectSpec>,
    targets: Vec<Checkbox>,
    target_labels: Vec<String>,
    kinds: Vec<Checkbox>,
    kind_labels: Vec<String>,
    // 결과(마스터) + 보기(거름·정렬)
    rows: Vec<Row>,
    index: HashMap<RowKey, usize>,
    view: Vec<usize>,
    filter: TextBox,
    whats: Vec<Checkbox>,
    what_labels: Vec<String>,
    sort: (SortKey, bool),
    hdr_rects: Vec<(SortKey, Rect)>,
    sel: Option<usize>,
    hover: Option<usize>,
    scroll_y: i32,
    bars: ScrollBars,
    list_rect: Rect,
    row_h: i32,
    focus: Focus,
    last_click: Option<(usize, Instant)>,
    // 미리보기
    split: f32,
    split_drag: bool,
    split_band: Rect,
    pre_rect: Rect,
    pre_lines: Vec<PreLine>,
    pre_scroll: i32,
    pre_head: String,
    // 진행
    rev: u64,
    running: bool,
    prog: Progress,
    ring: BusyRing,
    ring_rect: Rect,
    // 버튼·메뉴
    open_btn: Button,
    reveal_btn: Button,
    close_btn: Button,
    menu: ContextMenu,
    menu_row: Option<usize>,
    icons: HashMap<(IconKind, i32), Rc<IconImage>>,
    max_hits: usize,
    lines_per_object: usize,
}

/// 시각 → 로컬 `HH:MM:SS`(없으면 `-`).
fn fmt_clock(t: Option<SystemTime>) -> String {
    match t {
        None => "-".to_string(),
        Some(t) => {
            let l = nsql_log::local_at(t);
            format!("{:02}:{:02}:{:02}", l.hour, l.min, l.sec)
        }
    }
}

impl FindObjWin {
    pub(crate) fn new() -> Self {
        let tmsgs = [
            Msg::FoTargetName,
            Msg::FoTargetComment,
            Msg::FoTargetColumn,
            Msg::FoTargetSource,
        ];
        let kmsgs = [
            Msg::FoKindRelations,
            Msg::FoKindRoutines,
            Msg::FoKindTriggers,
            Msg::FoKindOthers,
        ];
        let targets = tmsgs.iter().map(|m| Checkbox::new(t(*m), true)).collect();
        let target_labels: Vec<String> = tmsgs.iter().map(|m| t(*m).to_string()).collect();
        let kinds = kmsgs.iter().map(|m| Checkbox::new(t(*m), true)).collect();
        let kind_labels = kmsgs.iter().map(|m| t(*m).to_string()).collect();
        let whats = tmsgs.iter().map(|m| Checkbox::new(t(*m), true)).collect();
        let what_labels = target_labels.clone();
        Self {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            memo: crate::wingeom::Memo::default(),
            last: None,
            epoch: Instant::now(),
            query: TextBox::new(t(Msg::FoQueryHint)),
            schema: TextBox::new(t(Msg::FoSchemaHint)),
            opts: vec![
                FindBtn::new(BtnKind::Case, toolicons::mi_match_case),
                FindBtn::new(BtnKind::Word, toolicons::mi_match_word),
                FindBtn::new(BtnKind::Regex, toolicons::mi_regex),
            ],
            all_dbs: Checkbox::new(t(Msg::FoAllDbs), true),
            run_btn: Button::new(t(Msg::FoSearch)),
            server: Combo::new(vec![ComboItem::new("", t(Msg::FoServerAll))], 0),
            servers: Vec::new(),
            run_specs: Vec::new(),
            targets,
            target_labels,
            kinds,
            kind_labels,
            rows: Vec::new(),
            index: HashMap::new(),
            view: Vec::new(),
            filter: TextBox::new(t(Msg::FoFilterHint)),
            whats,
            what_labels,
            sort: (SortKey::Name, true),
            hdr_rects: Vec::new(),
            sel: None,
            hover: None,
            scroll_y: 0,
            bars: ScrollBars::new(),
            list_rect: Rect::default(),
            row_h: 0,
            focus: Focus::Query,
            last_click: None,
            split: 0.6,
            split_drag: false,
            split_band: Rect::default(),
            pre_rect: Rect::default(),
            pre_lines: Vec::new(),
            pre_scroll: 0,
            pre_head: String::new(),
            rev: 0,
            running: false,
            prog: Progress::default(),
            ring: BusyRing::default(),
            ring_rect: Rect::default(),
            open_btn: Button::new(t(Msg::FoOpenEditor)),
            reveal_btn: Button::new(t(Msg::FoReveal)),
            close_btn: Button::new(t(Msg::SpBtnClose)),
            menu: ContextMenu::new(),
            menu_row: None,
            icons: HashMap::new(),
            max_hits: 2000,
            lines_per_object: 10,
        }
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn opt(&self, k: BtnKind) -> bool {
        self.opts.iter().any(|b| b.kind == k && b.checked)
    }

    fn set_opt(&mut self, k: BtnKind, on: bool) {
        if let Some(b) = self.opts.iter_mut().find(|b| b.kind == k) {
            b.checked = on;
        }
    }

    /// 설정 `findobj.all_dbs` → 창 토글(열 때 · 설정 변경 때).
    pub(crate) fn set_all_dbs(&mut self, on: bool) {
        self.all_dbs.set_checked(on);
    }

    pub(crate) fn all_dbs(&self) -> bool {
        self.all_dbs.is_checked()
    }

    pub(crate) fn set_limits(&mut self, max_hits: usize, lines_per_object: usize) {
        self.max_hits = max_hits;
        self.lines_per_object = lines_per_object;
    }

    pub(crate) fn set_remembered(&mut self, targets: &str, kinds: &str) {
        let on = |list: &str, key: &str| list.split(',').any(|s| s.trim() == key);
        if !targets.trim().is_empty() {
            for (cb, key) in self
                .targets
                .iter_mut()
                .zip(["name", "comment", "column", "source"])
            {
                cb.set_checked(on(targets, key));
            }
        }
        if !kinds.trim().is_empty() {
            for (cb, key) in
                self.kinds
                    .iter_mut()
                    .zip(["relations", "routines", "triggers", "others"])
            {
                cb.set_checked(on(kinds, key));
            }
        }
    }

    pub(crate) fn remembered(&self) -> (String, String) {
        let pick = |cbs: &[Checkbox], keys: [&str; 4]| -> String {
            cbs.iter()
                .zip(keys)
                .filter(|(cb, _)| cb.is_checked())
                .map(|(_, k)| k)
                .collect::<Vec<_>>()
                .join(",")
        };
        (
            pick(&self.targets, ["name", "comment", "column", "source"]),
            pick(&self.kinds, ["relations", "routines", "triggers", "others"]),
        )
    }

    pub(crate) fn set_servers(&mut self, servers: Vec<(Option<ConnectSpec>, String)>) {
        let cur = self.server.selected_value();
        let mut items = vec![ComboItem::new("", t(Msg::FoServerAll))];
        for (i, (_, label)) in servers.iter().enumerate() {
            items.push(ComboItem::new(format!("{i}"), label.clone()));
        }
        let idx = items.iter().position(|it| it.value == cur).unwrap_or(0);
        self.server = Combo::new(items, idx);
        self.servers = servers;
    }

    /// 검색 시작 때 창이 아는 서버 목록(엔진 `server` 번호 = 이 목록 차례).
    pub(crate) fn servers(&self) -> &[(Option<ConnectSpec>, String)] {
        &self.servers
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
        owner: Option<&Window>,
        preset: Option<&str>,
    ) {
        if let Some(q) = preset.filter(|q| !q.trim().is_empty()) {
            self.query.set_text(q);
        }
        if let Some(w) = &self.window {
            w.focus_window();
            self.own_focus(Focus::Query);
            self.redraw();
            return;
        }
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: format!("Nexa SQL - {}", t(Msg::WinFindObjects)),
                theme,
                near,
                dy: 40,
                owner,
                memo: Some(&self.memo),
                default_size: (1000.0, 720.0),
                ime: true,
            },
        ) else {
            return;
        };
        self.scale = o.scale;
        self.surface = o.surface;
        self.window = Some(o.window);
        self.own_focus(Focus::Query);
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.last = self.window.as_deref().and_then(crate::winhost::last_geom);
        self.surface = None;
        self.window = None;
        self.menu.close();
        self.running = false;
        self.ring.set_state(BusyState::Idle);
        self.run_btn.set_label(t(Msg::FoSearch));
    }

    pub(crate) fn set_memo(&mut self, m: crate::wingeom::Memo) {
        self.memo = m;
    }

    pub(crate) fn take_last(&mut self) -> Option<((i32, i32), (f64, f64))> {
        self.last.take()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn rev(&self) -> u64 {
        self.rev
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    // ── 검색 ───────────────────────────────────────────────────────────────────────────────────────

    fn build_query(&self) -> Option<FindQuery> {
        let text = self.query.text().trim().to_string();
        if text.is_empty() {
            return None;
        }
        let tg: Vec<bool> = self.targets.iter().map(Checkbox::is_checked).collect();
        let kd: Vec<bool> = self.kinds.iter().map(Checkbox::is_checked).collect();
        let mut kinds: Vec<ObjectKind> = Vec::new();
        if kd[0] {
            kinds.extend([
                ObjectKind::Table,
                ObjectKind::View,
                ObjectKind::MaterializedView,
                ObjectKind::ExternalTable,
                ObjectKind::ForeignTable,
            ]);
        }
        if kd[1] {
            kinds.extend([
                ObjectKind::Procedure,
                ObjectKind::Function,
                ObjectKind::Package,
                ObjectKind::PackageBody,
                ObjectKind::Type,
                ObjectKind::Aggregate,
            ]);
        }
        if kd[2] {
            kinds.extend([
                ObjectKind::Trigger,
                ObjectKind::SchemaTrigger,
                ObjectKind::EventTrigger,
            ]);
        }
        if kd[3] {
            kinds.extend([
                ObjectKind::Sequence,
                ObjectKind::Synonym,
                ObjectKind::Index,
                ObjectKind::Queue,
                ObjectKind::DbLink,
                ObjectKind::JavaClass,
                ObjectKind::Job,
                ObjectKind::SchedulerJob,
                ObjectKind::SchedulerProgram,
                ObjectKind::Extension,
            ]);
        }
        let kinds_all = kd.iter().all(|k| *k);
        let schemas: Vec<String> = self
            .schema
            .text()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Some(FindQuery {
            pat: FindPat {
                text,
                case_sensitive: self.opt(BtnKind::Case),
                whole_word: self.opt(BtnKind::Word),
            },
            regex: self.opt(BtnKind::Regex),
            names: tg[0],
            comments: tg[1],
            columns: tg[2],
            source: tg[3],
            kinds,
            kinds_all,
            schemas,
            max_hits: self.max_hits,
            lines_per_object: self.lines_per_object,
            all_dbs: self.all_dbs.is_checked(),
        })
    }

    fn selected_server(&self) -> Option<ConnectSpec> {
        let v = self.server.selected_value();
        v.parse::<usize>()
            .ok()
            .and_then(|i| self.servers.get(i))
            .and_then(|(k, _)| k.clone())
    }

    fn start(&mut self) -> FoAction {
        let Some(q) = self.build_query() else {
            self.prog.notes = vec![t(Msg::FoStatusNeedQuery).to_string()];
            self.redraw();
            return FoAction::None;
        };
        self.rev += 1;
        self.rows.clear();
        self.index.clear();
        self.view.clear();
        self.sel = None;
        self.hover = None;
        self.scroll_y = 0;
        self.pre_lines.clear();
        self.pre_head.clear();
        self.prog = Progress {
            started_at: Some(SystemTime::now()),
            started: Some(Instant::now()),
            ..Progress::default()
        };
        self.running = true;
        self.ring.set_state(BusyState::Running);
        self.run_btn.set_label(t(Msg::FoStop));
        self.redraw();
        FoAction::Search(Box::new(q), self.selected_server())
    }

    /// 호스트가 엔진에 넘긴 서버들(차례 = 엔진 `server` 번호).
    pub(crate) fn started(&mut self, specs: Vec<ConnectSpec>) {
        self.prog.servers_total = specs.len();
        self.prog.planned = vec![false; specs.len()];
        self.prog.done = vec![false; specs.len()];
        self.run_specs = specs;
        if self.prog.servers_total == 0 {
            self.finish();
            self.prog.notes.push(t(Msg::FoNoServers).to_string());
        }
        self.redraw();
    }

    fn finish(&mut self) {
        if !self.running {
            return;
        }
        self.running = false;
        self.prog.ended_at = Some(SystemTime::now());
        self.prog.elapsed_ms = self.prog.started.map_or(0, |s| s.elapsed().as_millis());
        self.ring.set_state(BusyState::Done);
        self.run_btn.set_label(t(Msg::FoSearch));
    }

    fn stop(&mut self) -> FoAction {
        if !self.running {
            return FoAction::None;
        }
        self.finish();
        self.prog.notes.push(t(Msg::FoStopped).to_string());
        self.redraw();
        FoAction::Stop
    }

    /// 엔진 메시지를 합친다(rev가 다르면 버림).
    pub(crate) fn push_msg(&mut self, m: EngineMsg) {
        match m {
            EngineMsg::Planned {
                rev,
                server,
                schemas,
                tasks,
                note,
                label,
            } => {
                if rev != self.rev || !self.running {
                    return;
                }
                if self.prog.planned.get(server).copied().unwrap_or(true) {
                    return;
                }
                self.prog.planned[server] = true;
                self.prog.servers_planned += 1;
                self.prog.schemas += schemas;
                self.prog.tasks_total += tasks;
                if let Some(n) = note {
                    self.note(format!("{label}: {n}"));
                }
            }
            EngineMsg::Task {
                rev,
                server,
                label,
                stage,
                schema,
                hits,
                scanned,
                note,
                ms,
            } => {
                if rev != self.rev || !self.running {
                    return;
                }
                self.prog.tasks_done += 1;
                self.prog.scanned += scanned;
                self.prog.last_task = Some(format!("{label} · {schema} · {stage:?} · {ms} ms"));
                let spec = self.run_specs.get(server).cloned();
                for h in hits {
                    self.merge(spec.clone(), &label, h);
                }
                if let Some(n) = note {
                    self.note(format!("{label}: {n}"));
                }
                self.apply_view();
            }
            EngineMsg::ServerDone {
                rev,
                server,
                label,
                err,
            } => {
                if rev != self.rev || !self.running {
                    return;
                }
                if self.prog.done.get(server).copied().unwrap_or(true) {
                    return;
                }
                self.prog.done[server] = true;
                self.prog.servers_done += 1;
                if let Some(e) = err {
                    self.prog.errors.push(format!("{label}: {e}"));
                }
                if self.prog.servers_done >= self.prog.servers_total {
                    self.finish();
                }
            }
        }
        self.redraw();
    }

    fn note(&mut self, n: String) {
        if !self.prog.notes.contains(&n) {
            self.prog.notes.push(n);
        }
    }

    fn merge(&mut self, server: Option<ConnectSpec>, label: &str, h: FindHit) {
        let probe = Row {
            server,
            label: label.to_string(),
            db: h.db.clone(),
            schema: h.schema.clone(),
            name: h.name.clone(),
            kind: h.kind,
            name_hit: false,
            comment: None,
            cols: Vec::new(),
            src: Vec::new(),
        };
        let key = probe.key();
        let i = match self.index.get(&key) {
            Some(&i) => i,
            None => {
                self.rows.push(probe);
                let i = self.rows.len() - 1;
                self.index.insert(key, i);
                i
            }
        };
        let row = &mut self.rows[i];
        match h.what {
            FindWhat::Name => row.name_hit = true,
            FindWhat::Comment(c) => row.comment = Some(c),
            FindWhat::Column { column, data_type } => row.cols.push((column, data_type)),
            FindWhat::Source { line, text } => row.src.push((line, text)),
        }
        self.prog.hits += 1;
    }

    // ── 보기(거름 · 정렬) ───────────────────────────────────────────────────────────────────────────

    /// 결과 거르기 + 정렬 → `view`. 선택 행은 유지.
    /// ★ 규칙(10-16 사용자 "거르기 동작 모호 · 필요하면 퍼지"): 띄어쓰기로 나눈 **낱말마다 AND** · 낱말은 화면의 모든 열(종류·DB·스키마·이름·서버·일치)
    ///   그리고 컬럼 이름 · 설명의 **부분 문자열**이거나, **이름의 퍼지 부분열**(`spmpo` → `SP_M4P_MPO_…` · Ctrl+P와 같은 점수 함수)이면 일치.
    ///   거른 뒤 스크롤을 새 길이 안으로 당긴다(종전 = 내려 둔 스크롤이 남아 **빈 화면** = "1433 입력 시 아무것도 안 보임"의 원인).
    fn apply_view(&mut self) {
        let keep = self.sel.and_then(|v| self.view.get(v).copied());
        let f = self.filter.text().trim().to_lowercase();
        let words: Vec<String> = f.split_whitespace().map(String::from).collect();
        let wq: Vec<Vec<char>> = words.iter().map(|w| w.chars().collect()).collect();
        let w: Vec<bool> = self.whats.iter().map(Checkbox::is_checked).collect();
        let all_w = w.iter().all(|x| *x);
        let mut v: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                let r = &self.rows[i];
                if !all_w {
                    let ok = (w[0] && r.name_hit)
                        || (w[1] && r.comment.is_some())
                        || (w[2] && !r.cols.is_empty())
                        || (w[3] && !r.src.is_empty());
                    if !ok {
                        return false;
                    }
                }
                if words.is_empty() {
                    return true;
                }
                let hay = r.hay_lower();
                let name_l = r.name.to_lowercase();
                words.iter().zip(&wq).all(|(w, q)| {
                    hay.contains(w.as_str())
                        || crate::palette::fuzzy_score_lower(q, &name_l).is_some()
                })
            })
            .collect();
        let (key, asc) = self.sort;
        let rows = &self.rows;
        let nat = nexa_ctl::natural::cmp_names;
        v.sort_by(|&a, &b| {
            let (ra, rb) = (&rows[a], &rows[b]);
            let o = match key {
                SortKey::Kind => ra
                    .kind
                    .code()
                    .cmp(rb.kind.code())
                    .then_with(|| nat(&ra.name, &rb.name)),
                SortKey::Name => nat(&ra.name, &rb.name).then_with(|| nat(&ra.schema, &rb.schema)),
                SortKey::Schema => {
                    nat(&ra.schema, &rb.schema).then_with(|| nat(&ra.name, &rb.name))
                }
                SortKey::Server => nat(&ra.label, &rb.label)
                    .then_with(|| nat(&ra.schema, &rb.schema))
                    .then_with(|| nat(&ra.name, &rb.name)),
                SortKey::Hits => rb
                    .hit_count()
                    .cmp(&ra.hit_count())
                    .then_with(|| nat(&ra.name, &rb.name)),
            };
            if asc {
                o
            } else {
                o.reverse()
            }
        });
        self.view = v;
        self.sel = keep.and_then(|k| self.view.iter().position(|&i| i == k));
        if self.sel.is_none() {
            self.pre_lines.clear();
            self.pre_head.clear();
        }
        // 스크롤을 새 내용 길이 안으로(빈 화면 방지) · 선택 행이 남았으면 보이게.
        let content_h = self.view.len() as i32 * self.row_h.max(1);
        self.scroll_y = self
            .scroll_y
            .clamp(0, (content_h - self.list_rect.h).max(0));
        if let Some(v) = self.sel {
            self.ensure_visible(v);
        }
    }

    fn set_sort(&mut self, key: SortKey) {
        self.sort = if self.sort.0 == key {
            (key, !self.sort.1)
        } else {
            (key, true)
        };
        self.apply_view();
    }

    // ── 미리보기 ───────────────────────────────────────────────────────────────────────────────────

    fn select(&mut self, v: Option<usize>) {
        if self.sel != v {
            self.sel = v;
            self.pre_scroll = 0;
            self.rebuild_preview();
        }
    }

    fn sel_row(&self) -> Option<usize> {
        self.sel.and_then(|v| self.view.get(v).copied())
    }

    fn spans(&self, text: &str) -> Vec<(usize, usize)> {
        let needle = self.query.text().trim().to_string();
        if needle.is_empty() || self.opt(BtnKind::Regex) {
            return Vec::new();
        }
        let (hay, nd) = if self.opt(BtnKind::Case) {
            (text.to_string(), needle)
        } else {
            (text.to_lowercase(), needle.to_lowercase())
        };
        if hay.len() != text.len() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(i) = hay[from..].find(&nd) {
            out.push((from + i, from + i + nd.len()));
            from += i + nd.len();
            if from >= hay.len() {
                break;
            }
        }
        out
    }

    fn rebuild_preview(&mut self) {
        self.pre_lines.clear();
        let Some(i) = self.sel_row() else {
            self.pre_head.clear();
            return;
        };
        let r = self.rows[i].clone();
        let kind = format!("{:?}", r.kind).to_uppercase();
        self.pre_head = format!("{} ({} · {})", r.full_name(), kind, r.label);
        let mut lines = Vec::new();
        if r.name_hit {
            let text = r.name.clone();
            let spans = self.spans(&text);
            lines.push(PreLine {
                no: None,
                text,
                spans,
            });
        }
        if let Some(c) = &r.comment {
            let text = format!("» {c}");
            let spans = self.spans(&text);
            lines.push(PreLine {
                no: None,
                text,
                spans,
            });
        }
        for (c, ty) in &r.cols {
            let text = format!("{c}  {ty}");
            let spans = self.spans(&text);
            lines.push(PreLine {
                no: None,
                text,
                spans,
            });
        }
        for (ln, text) in &r.src {
            let spans = self.spans(text);
            lines.push(PreLine {
                no: Some(*ln),
                text: text.clone(),
                spans,
            });
        }
        self.pre_lines = lines;
    }

    // ── 동작 ───────────────────────────────────────────────────────────────────────────────────────

    fn activate(&self, i: usize, line: u32) -> FoAction {
        let r = &self.rows[i];
        if r.kind.has_source() {
            FoAction::OpenSource {
                server: r.server.clone(),
                obj: r.obj(),
                line,
            }
        } else {
            FoAction::Reveal {
                server: r.server.clone(),
                obj: r.obj(),
            }
        }
    }

    fn first_line(&self, i: usize) -> u32 {
        self.rows[i].src.first().map_or(0, |(l, _)| *l)
    }

    fn open_menu(&mut self, i: usize) {
        let (x, y) = self.cursor;
        let r = &self.rows[i];
        let mut items = Vec::new();
        if r.kind.has_source() {
            items.push(CtxItem::item("open", t(Msg::ExpOpenSource)));
        }
        items.push(CtxItem::item("reveal", t(Msg::FoReveal)));
        if r.kind.is_relation() {
            items.push(CtxItem::item("select", t(Msg::ExpSelectRows)));
        }
        items.push(CtxItem::Separator);
        items.push(CtxItem::item("copy", t(Msg::ExpCopyName)));
        items.push(CtxItem::item("copyfull", t(Msg::ExpCopyFullName)));
        self.menu_row = Some(i);
        self.menu.set_scale(self.scale);
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let s = w.inner_size();
                Rect::new(0, 0, s.width as i32, s.height as i32)
            })
            .unwrap_or(self.list_rect);
        crate::menu_icons::open_at(
            &mut self.menu,
            x,
            y,
            items,
            host,
            (220.0 * self.scale) as i32,
        );
        self.redraw();
    }

    fn menu_pick(&mut self, id: &str) -> FoAction {
        let Some(i) = self.menu_row.take() else {
            return FoAction::None;
        };
        if i >= self.rows.len() {
            return FoAction::None;
        }
        let r = &self.rows[i];
        match id {
            "open" => FoAction::OpenSource {
                server: r.server.clone(),
                obj: r.obj(),
                line: self.first_line(i),
            },
            "reveal" => FoAction::Reveal {
                server: r.server.clone(),
                obj: r.obj(),
            },
            "select" => FoAction::GenSelect {
                server: r.server.clone(),
                obj: r.obj(),
            },
            "copy" => FoAction::Copy(r.name.clone()),
            "copyfull" => FoAction::Copy(r.full_name()),
            _ => FoAction::None,
        }
    }

    fn view_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) || self.row_h <= 0 {
            return None;
        }
        let i = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h) as usize;
        (i < self.view.len()).then_some(i)
    }

    fn ensure_visible(&mut self, v: usize) {
        let top = v as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + self.list_rect.h {
            self.scroll_y = top + self.row_h - self.list_rect.h;
        }
        self.scroll_y = self.scroll_y.max(0);
    }

    fn own_focus(&mut self, f: Focus) {
        self.focus = f;
        self.query.set_focused(f == Focus::Query);
        self.schema.set_focused(f == Focus::Schema);
        self.filter.set_focused(f == Focus::Filter);
    }

    fn focused_box(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Query => Some(&mut self.query),
            Focus::Schema => Some(&mut self.schema),
            Focus::Filter => Some(&mut self.filter),
            Focus::List => None,
        }
    }

    /// 체크박스 전부(전 DB 1 + 대상 4 + 종류 4 + 일치 종류 4 — 마지막 넷은 보기 갱신).
    fn all_checks(&mut self) -> Vec<&mut Checkbox> {
        let mut v: Vec<&mut Checkbox> = vec![&mut self.all_dbs];
        v.extend(self.targets.iter_mut());
        v.extend(self.kinds.iter_mut());
        v.extend(self.whats.iter_mut());
        v
    }

    fn buttons(&mut self) -> [&mut Button; 4] {
        [
            &mut self.run_btn,
            &mut self.open_btn,
            &mut self.reveal_btn,
            &mut self.close_btn,
        ]
    }

    fn preview_line_at(&self, p: Point) -> Option<u32> {
        let lh = self.row_h.max(1);
        let head_h = lh + (6.0 * self.scale) as i32;
        let y = p.y - self.pre_rect.y - head_h + self.pre_scroll;
        if y < 0 {
            return None;
        }
        self.pre_lines.get((y / lh) as usize).and_then(|l| l.no)
    }

    fn icon(&mut self, kind: ObjectKind, size: i32) -> Rc<IconImage> {
        let k = exp_icons::object_icon(kind);
        self.icons
            .entry((k, size))
            .or_insert_with(|| {
                let base = exp_icons::image(k, k.color());
                Rc::new(base.resized(size.max(1) as u32, size.max(1) as u32))
            })
            .clone()
    }

    /// 호스트 틱 — 진행 중이면 경과·혜성이 움직인다(돌려주는 값 = 다시 그릴 것).
    pub(crate) fn tick(&mut self) -> bool {
        if self.window.is_none() {
            return false;
        }
        let anim = self.ring.tick(self.now_ms());
        let btn_anim = self.opts.iter().any(|b| {
            b.hover.is_animating()
                || b.hover_since
                    .is_some_and(|t0| t0.elapsed().as_millis() < TOOLTIP_MS + 50)
        });
        self.running || anim || btn_anim
    }

    // ── 사건 ───────────────────────────────────────────────────────────────────────────────────────

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> FoAction {
        let mut inv = Invalidations::default();
        if self.menu.is_open() {
            let me = match ev {
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = (position.x as i32, position.y as i32);
                    Some(InputEvent::MouseMove {
                        x: self.cursor.0,
                        y: self.cursor.1,
                    })
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let (x, y) = self.cursor;
                    Some(match (state, button) {
                        (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                            x,
                            y,
                            shift: self.shift,
                            primary: self.primary,
                        },
                        (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                        (ElementState::Pressed, MouseButton::Right) => {
                            InputEvent::RightDown { x, y }
                        }
                        _ => InputEvent::MouseMove { x, y },
                    })
                }
                WindowEvent::KeyboardInput { event: kev, .. }
                    if kev.state == ElementState::Pressed
                        && kev.logical_key == Key::Named(NamedKey::Escape) =>
                {
                    self.menu.close();
                    self.redraw();
                    return FoAction::None;
                }
                WindowEvent::RedrawRequested => return FoAction::Paint,
                WindowEvent::CloseRequested => return FoAction::Close,
                _ => None,
            };
            if let Some(me) = me {
                let outside = self.menu.is_outside_click(&me);
                let consumed = self.menu.on_event(&me) && !outside;
                if let Some(id) = self.menu.take_picked() {
                    self.redraw();
                    return self.menu_pick(&id);
                }
                if consumed || !outside {
                    self.redraw();
                    return FoAction::None;
                }
            }
        }
        match ev {
            WindowEvent::CloseRequested => return FoAction::Close,
            WindowEvent::RedrawRequested => return FoAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in self.buttons() {
                    b.clear_transient();
                }
                for b in &mut self.opts {
                    b.clear_transient();
                }
                self.hover = None;
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                if self.split_drag {
                    let total = (self.pre_rect.bottom() - self.list_rect.y).max(1) as f32;
                    self.split = ((y - self.list_rect.y) as f32 / total).clamp(0.2, 0.9);
                    self.redraw();
                    return FoAction::None;
                }
                let mv = InputEvent::MouseMove { x, y };
                let p = Point { x, y };
                self.query.on_event(&mv, &mut inv);
                self.schema.on_event(&mv, &mut inv);
                self.filter.on_event(&mv, &mut inv);
                if self.query.popup_open() || self.schema.popup_open() || self.filter.popup_open() {
                    self.redraw();
                    return FoAction::None;
                }
                self.server.on_event(&mv, &mut inv);
                for cb in self.all_checks() {
                    cb.on_event(&mv, &mut inv);
                }
                for b in self.buttons() {
                    b.on_event(&mv, &mut inv);
                }
                for b in &mut self.opts {
                    b.on_event(&mv);
                }
                let h = self.view_at(p);
                let hover_changed = h != self.hover;
                self.hover = h;
                if hover_changed || !inv.is_empty() {
                    self.redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let e = crate::input::wheel_event(delta, self.shift);
                if self.server.is_open() {
                    self.server.on_event(&e, &mut inv);
                } else if self.list_rect.contains(p) {
                    let content_h = (self.view.len() as i32 * self.row_h).max(self.list_rect.h);
                    let (_, ny, _) = self.bars.on_event(
                        &e,
                        self.list_rect,
                        self.list_rect.w,
                        content_h,
                        0,
                        self.scroll_y,
                        self.scale,
                    );
                    self.scroll_y = ny.clamp(0, (content_h - self.list_rect.h).max(0));
                } else if self.pre_rect.contains(p) {
                    if let InputEvent::Wheel { delta } = e {
                        let lh = self.row_h.max(1);
                        let max = ((self.pre_lines.len() as i32 + 1) * lh - self.pre_rect.h).max(0);
                        self.pre_scroll = (self.pre_scroll - delta.signum() * lh * 3).clamp(0, max);
                    }
                }
                self.redraw();
            }
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let e = match state {
                    ElementState::Pressed => InputEvent::MouseDown {
                        x,
                        y,
                        shift: self.shift,
                        primary: self.primary,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                let up = matches!(e, InputEvent::MouseUp { .. });
                if up {
                    self.split_drag = false;
                }
                let mut popup_took = false;
                for tb in [&mut self.query, &mut self.schema, &mut self.filter] {
                    if tb.popup_open() {
                        tb.on_event(&e, &mut inv);
                        popup_took = true;
                    }
                }
                if popup_took {
                    self.redraw();
                    return FoAction::None;
                }
                if self.server.is_open() {
                    self.server.on_event(&e, &mut inv);
                    self.server.take_changed();
                    self.redraw();
                    return FoAction::None;
                }
                if !up && self.split_band.contains(p) {
                    self.split_drag = true;
                    return FoAction::None;
                }
                // 안쪽 토글(검색어 상자 위에 겹침) — 먹으면 상자로 가지 않는다.
                let mut opt_hit = false;
                for b in &mut self.opts {
                    if up || b.rect.contains(p) {
                        b.on_event(&e);
                        if b.rect.contains(p) {
                            opt_hit = true;
                        }
                        if b.take_clicked() {
                            b.checked = !b.checked;
                        }
                    }
                }
                if opt_hit {
                    self.redraw();
                    return FoAction::None;
                }
                // 머리 클릭 = 정렬.
                if !up {
                    if let Some((k, _)) = self.hdr_rects.iter().find(|(_, r)| r.contains(p)) {
                        let k = *k;
                        self.set_sort(k);
                        self.redraw();
                        return FoAction::None;
                    }
                }
                if up || self.query.bounds().contains(p) {
                    self.query.on_event(&e, &mut inv);
                    if !up {
                        self.own_focus(Focus::Query);
                    }
                }
                if up || self.schema.bounds().contains(p) {
                    self.schema.on_event(&e, &mut inv);
                    if !up {
                        self.own_focus(Focus::Schema);
                    }
                }
                if up || self.filter.bounds().contains(p) {
                    self.filter.on_event(&e, &mut inv);
                    if !up {
                        self.own_focus(Focus::Filter);
                    }
                }
                if up || self.server.bounds().contains(p) {
                    self.server.on_event(&e, &mut inv);
                    self.server.take_changed();
                }
                let mut what_toggled = false;
                let n_front = 1 + self.targets.len() + self.kinds.len();
                for (k, cb) in self.all_checks().into_iter().enumerate() {
                    if up || cb.bounds().contains(p) {
                        cb.on_event(&e, &mut inv);
                    }
                    cb.set_focused(false);
                    if cb.take_toggled().is_some() && k >= n_front {
                        what_toggled = true;
                    }
                }
                if what_toggled {
                    self.apply_view();
                }
                let mut clicked: Option<usize> = None;
                for (k, b) in self.buttons().into_iter().enumerate() {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    b.set_focused(false);
                    if b.take_clicked() {
                        clicked = Some(k);
                    }
                }
                if !up {
                    if let Some(v) = self.view_at(p) {
                        self.own_focus(Focus::List);
                        let now = Instant::now();
                        let dbl = matches!(self.last_click, Some((j, t)) if j == v && now.duration_since(t).as_millis() < 450);
                        self.last_click = Some((v, now));
                        self.select(Some(v));
                        if dbl {
                            self.last_click = None;
                            self.redraw();
                            if let Some(i) = self.sel_row() {
                                return self.activate(i, self.first_line(i));
                            }
                        }
                    } else if self.list_rect.contains(p) {
                        self.own_focus(Focus::List);
                    }
                    // ★ 미리보기 줄 = **한 번 클릭**으로 편집기 그 줄(사용자 10-16 · 종전 더블클릭 450 ms는 "열리지 않는다"로 보임).
                    if self.pre_rect.contains(p) {
                        if let (Some(i), Some(ln)) = (self.sel_row(), self.preview_line_at(p)) {
                            self.last_click = None;
                            return self.activate(i, ln);
                        }
                    }
                }
                self.redraw();
                match clicked {
                    Some(0) => {
                        return if self.running {
                            self.stop()
                        } else {
                            self.start()
                        };
                    }
                    Some(1) => {
                        if let Some(i) = self.sel_row() {
                            return self.activate(i, self.first_line(i));
                        }
                    }
                    Some(2) => {
                        if let Some(i) = self.sel_row() {
                            let r = &self.rows[i];
                            return FoAction::Reveal {
                                server: r.server.clone(),
                                obj: r.obj(),
                            };
                        }
                    }
                    Some(3) => return FoAction::Close,
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Right && *state == ElementState::Pressed =>
            {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let rd = InputEvent::RightDown { x, y };
                if self.query.bounds().contains(p) {
                    self.query.on_event(&rd, &mut inv);
                } else if self.schema.bounds().contains(p) {
                    self.schema.on_event(&rd, &mut inv);
                } else if self.filter.bounds().contains(p) {
                    self.filter.on_event(&rd, &mut inv);
                } else if let Some(v) = self.view_at(p) {
                    self.select(Some(v));
                    self.own_focus(Focus::List);
                    if let Some(i) = self.sel_row() {
                        self.open_menu(i);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                let is_filter = self.focus == Focus::Filter;
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
                }
                if is_filter {
                    self.apply_view();
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit(text, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => {
                        return if self.running {
                            self.stop()
                        } else {
                            FoAction::Close
                        };
                    }
                    Key::Named(NamedKey::Enter) => {
                        if self.focus == Focus::List {
                            if let Some(i) = self.sel_row() {
                                return self.activate(i, self.first_line(i));
                            }
                        } else if self.focus == Focus::Filter {
                            self.apply_view();
                            self.redraw();
                            return FoAction::None;
                        } else {
                            return self.start();
                        }
                    }
                    Key::Named(NamedKey::Tab) => {
                        let next = match self.focus {
                            Focus::Query => Focus::Schema,
                            Focus::Schema => Focus::Filter,
                            Focus::Filter => Focus::List,
                            Focus::List => Focus::Query,
                        };
                        self.own_focus(next);
                        self.redraw();
                        return FoAction::None;
                    }
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp)
                        if self.focus == Focus::List
                            || (!self.view.is_empty() && self.focus == Focus::Query) =>
                    {
                        if self.view.is_empty() {
                            return FoAction::None;
                        }
                        let down = kev.logical_key == Key::Named(NamedKey::ArrowDown);
                        let v = match self.sel {
                            None => 0,
                            Some(v) if down => (v + 1).min(self.view.len() - 1),
                            Some(v) => v.saturating_sub(1),
                        };
                        self.own_focus(Focus::List);
                        self.select(Some(v));
                        self.ensure_visible(v);
                        self.redraw();
                        return FoAction::None;
                    }
                    Key::Character(c) if self.primary && self.focus == Focus::List => {
                        let lower = crate::input::shortcut_letter(kev)
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| c.to_lowercase());
                        if lower == "c" {
                            if let Some(i) = self.sel_row() {
                                let r = &self.rows[i];
                                return FoAction::Copy(if self.shift {
                                    r.full_name()
                                } else {
                                    r.name.clone()
                                });
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(e) = crate::input::text_key_event(
                    kev,
                    self.shift,
                    self.primary,
                    crate::input::TextKeys::Line,
                ) {
                    let is_filter = self.focus == Focus::Filter;
                    if let Some(tb) = self.focused_box() {
                        tb.on_event(&e, &mut inv);
                    }
                    if is_filter {
                        self.apply_view();
                    }
                    self.redraw();
                }
            }
            _ => {}
        }
        FoAction::None
    }

    // ── 그리기 ─────────────────────────────────────────────────────────────────────────────────────

    /// 진행 패널 두 줄(①: 상태 · 서버 d/T · 스키마 · 작업 d/T · 조사 행 · 객체 · 일치 / ②: 시작 · 종료 · 경과 · 보임 · 안내).
    fn progress_lines(&self) -> (String, String) {
        let p = &self.prog;
        let elapsed = if self.running {
            p.started.map_or(0, |s| s.elapsed().as_millis())
        } else {
            p.elapsed_ms
        } as f64
            / 1000.0;
        let head = if self.running {
            t(Msg::FoStatusRunningHead).to_string()
        } else if p.started.is_none() {
            t(Msg::FoStatusIdle).to_string()
        } else {
            t(Msg::FoStatusDoneHead).to_string()
        };
        let l1 = tf(
            Msg::FoProgressLine1,
            &[
                &head,
                &p.servers_done.to_string(),
                &p.servers_total.to_string(),
                &p.schemas.to_string(),
                &p.tasks_done.to_string(),
                &p.tasks_total.to_string(),
                &p.scanned.to_string(),
                &self.rows.len().to_string(),
                &p.hits.to_string(),
            ],
        );
        let mut l2 = tf(
            Msg::FoProgressLine2,
            &[
                &fmt_clock(p.started_at),
                &fmt_clock(p.ended_at),
                &format!("{elapsed:.1}"),
            ],
        );
        if self.view.len() != self.rows.len() {
            l2.push_str(" · ");
            l2.push_str(&tf(
                Msg::FoShownOf,
                &[&self.view.len().to_string(), &self.rows.len().to_string()],
            ));
        }
        if let Some(n) = p.errors.first().or(p.notes.first()) {
            l2.push_str(" · ");
            l2.push_str(n);
        } else if let Some(lt) = p.last_task.as_deref().filter(|_| self.running) {
            l2.push_str(" · ");
            l2.push_str(lt);
        }
        (l1, l2)
    }

    pub(crate) fn paint(&mut self, font: &Font, th: &Theme, ui_px: f32) {
        let Some(win) = self.window.clone() else {
            return;
        };
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let size = win.inner_size();
        let Some(mut buf) = surface.frame(size) else {
            self.surface = Some(surface);
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(12.0);
            let gap = px(8.0);
            let box_h = th_txt + px(10.0);
            let clip = Rect::new(0, 0, wi, hi);
            let inv = &mut Invalidations::default();
            // ── 1행: 검색어 · Aa ab (.*) · 검색/중지
            let btn_w = px(96.0);
            let x = wi - pad - btn_w;
            self.run_btn.set_scale(s);
            self.run_btn
                .set_bounds(Rect::new(x, pad, btn_w, box_h), inv);
            // 검색어 상자 + 안쪽 토글 Aa · ab · (.*)(필터 틀과 같은 20px · 간격 2 · 오른쪽 끝에서 거꾸로).
            let qrect = Rect::new(pad, pad, x - gap - pad, box_h);
            let tg = px(20.0);
            let tgap = px(2.0);
            let mut tx = qrect.right() - px(2.0) - tg;
            for b in self.opts.iter_mut().rev() {
                b.rect = Rect::new(tx, qrect.y + (box_h - tg) / 2, tg, tg);
                tx -= tg + tgap;
            }
            self.query.set_scale(s);
            self.query.set_bounds(
                Rect::new(
                    qrect.x,
                    qrect.y,
                    (tx + tg + tgap - qrect.x).max(px(80.0)),
                    box_h,
                ),
                inv,
            );
            self.ring_rect = qrect;
            // ── 2행: 서버 · 스키마
            let y2 = pad + box_h + gap;
            let combo_w = px(260.0);
            self.server.set_scale(s);
            self.server
                .set_bounds(Rect::new(pad, y2, combo_w, box_h), inv);
            // 전 DB 토글(SQL Server) — 서버 콤보 오른쪽.
            let adb_w = px(24.0) + dc.text_width(t(Msg::FoAllDbs)) + px(10.0);
            self.all_dbs.set_scale(s);
            self.all_dbs
                .set_bounds(Rect::new(pad + combo_w + gap, y2, adb_w, box_h), inv);
            let lab = t(Msg::FoSchemaLabel);
            let lab_w = dc.text_width(lab);
            let ly2 = dc.text_center_y(y2, box_h);
            let lx = pad + combo_w + gap + adb_w + gap;
            dc.text(lx, ly2, clip, lab, th.text_dim);
            let sx = lx + lab_w + gap;
            self.schema.set_scale(s);
            self.schema
                .set_bounds(Rect::new(sx, y2, wi - pad - sx, box_h), inv);
            // ── 3행: 대상 · 종류
            let y3 = y2 + box_h + gap;
            let mut cx = pad;
            let tl = t(Msg::FoTargets);
            let ly3 = dc.text_center_y(y3, box_h);
            dc.text(cx, ly3, clip, tl, th.text_dim);
            cx += dc.text_width(tl) + gap;
            for (cb, label) in self.targets.iter_mut().zip(self.target_labels.iter()) {
                cb.set_scale(s);
                let w = px(24.0) + dc.text_width(label) + px(10.0);
                cb.set_bounds(Rect::new(cx, y3, w, box_h), inv);
                cx += w + px(6.0);
            }
            cx += px(14.0);
            let kl = t(Msg::FoKinds);
            dc.text(cx, ly3, clip, kl, th.text_dim);
            cx += dc.text_width(kl) + gap;
            for (cb, label) in self.kinds.iter_mut().zip(self.kind_labels.iter()) {
                cb.set_scale(s);
                let w = px(24.0) + dc.text_width(label) + px(10.0);
                cb.set_bounds(Rect::new(cx, y3, w, box_h), inv);
                cx += w + px(6.0);
            }
            // ── 4행: 진행 패널(두 줄 · 혜성 링은 검색어 상자 둘레)
            let y4 = y3 + box_h + gap;
            let prog_h = th_txt * 2 + px(10.0);
            let prog_rect = Rect::new(pad, y4, wi - pad * 2, prog_h);
            dc.fill_round_rect(prog_rect, px(4.0), th.chrome_bg);
            let (l1, l2) = self.progress_lines();
            let c1 = if self.running { th.text } else { th.text_dim };
            dc.text(
                prog_rect.x + px(8.0),
                prog_rect.y + px(4.0),
                prog_rect,
                &l1,
                c1,
            );
            dc.text(
                prog_rect.x + px(8.0),
                prog_rect.y + px(6.0) + th_txt,
                prog_rect,
                &l2,
                th.text_dim,
            );
            // ── 5행: 결과 거르기(글 + 일치 종류 4)
            let y5 = y4 + prog_h + gap;
            let fl = t(Msg::FoFilterLabel);
            let ly5 = dc.text_center_y(y5, box_h);
            dc.text(pad, ly5, clip, fl, th.text_dim);
            let fx = pad + dc.text_width(fl) + gap;
            let mut wx = wi - pad;
            let mut what_w = Vec::new();
            for label in self.what_labels.iter().rev() {
                let w = px(24.0) + dc.text_width(label) + px(10.0);
                wx -= w + px(6.0);
                what_w.push((wx, w));
            }
            what_w.reverse();
            for (cb, (bx, w)) in self.whats.iter_mut().zip(what_w) {
                cb.set_scale(s);
                cb.set_bounds(Rect::new(bx, y5, w, box_h), inv);
            }
            let fw = (wx - gap - fx).max(px(80.0));
            self.filter.set_scale(s);
            self.filter.set_bounds(Rect::new(fx, y5, fw, box_h), inv);
            // ── 바닥: 버튼
            let btn_h = th_txt + px(14.0);
            let by = hi - pad - btn_h;
            let mut bx = wi - pad;
            for b in [
                &mut self.close_btn,
                &mut self.reveal_btn,
                &mut self.open_btn,
            ] {
                b.set_scale(s);
                let w = dc.text_width(b.label()) + px(28.0);
                bx -= w;
                b.set_bounds(Rect::new(bx, by, w, btn_h), inv);
                bx -= gap;
            }
            // ── 결과 목록 + 미리보기(스플리터)
            let top = y5 + box_h + gap;
            let bottom = by - gap;
            let total_h = (bottom - top).max(px(80.0));
            let band_h = px(6.0);
            let list_h = ((total_h - band_h) as f32 * self.split) as i32;
            let hdr_h = th_txt + px(8.0);
            self.row_h = th_txt + px(8.0);
            let list_outer = Rect::new(pad, top, wi - pad * 2, list_h);
            self.list_rect = Rect::new(
                list_outer.x,
                list_outer.y + hdr_h,
                list_outer.w,
                (list_outer.h - hdr_h).max(0),
            );
            self.split_band = Rect::new(pad, list_outer.bottom(), wi - pad * 2, band_h);
            self.pre_rect = Rect::new(
                pad,
                self.split_band.bottom(),
                wi - pad * 2,
                (bottom - self.split_band.bottom()).max(0),
            );
            dc.fill_rect(list_outer, th.field_bg);
            dc.fill_rect(
                Rect::new(list_outer.x, list_outer.y, list_outer.w, hdr_h),
                th.chrome_bg,
            );
            let col_icon = px(24.0);
            let w_kind = px(96.0);
            let w_name = (list_outer.w as f32 * 0.27) as i32;
            let w_schema = (list_outer.w as f32 * 0.14) as i32;
            let w_server = (list_outer.w as f32 * 0.18) as i32;
            let hy = dc.text_center_y(list_outer.y, hdr_h);
            let arrow = if self.sort.1 { " ▲" } else { " ▼" };
            let mut hx = list_outer.x + col_icon;
            let w_match = list_outer.right() - hx - w_kind - w_name - w_schema - w_server;
            self.hdr_rects.clear();
            for (key, msg, w) in [
                (SortKey::Kind, Msg::FoColKind, w_kind),
                (SortKey::Name, Msg::FoColName, w_name),
                (SortKey::Schema, Msg::FoColSchema, w_schema),
                (SortKey::Server, Msg::FoColServer, w_server),
                (SortKey::Hits, Msg::FoColMatch, w_match),
            ] {
                let mut label = t(msg).to_string();
                if self.sort.0 == key {
                    label.push_str(arrow);
                }
                let r = Rect::new(hx, list_outer.y, w.max(px(40.0)), hdr_h);
                dc.text(hx, hy, r, &label, th.text_dim);
                self.hdr_rects.push((key, r));
                hx += w;
            }
            // 행(보기 순서)
            let lr = self.list_rect;
            dc.push_clip(lr);
            let first = (self.scroll_y / self.row_h.max(1)).max(0) as usize;
            let icon_px = px(16.0);
            let mut y = lr.y - (self.scroll_y % self.row_h.max(1));
            let mut v = first;
            let n_view = self.view.len();
            while v < n_view && y < lr.bottom() {
                let i = self.view[v];
                let r = Rect::new(lr.x, y, lr.w, self.row_h);
                if self.sel == Some(v) {
                    let c = if self.focus == Focus::List {
                        th.sel_bg
                    } else {
                        th.sel_bg_inactive
                    };
                    dc.fill_rect(r, c);
                } else if self.hover == Some(v) {
                    dc.fill_rect_alpha(r, th.sel_bg, 0.35);
                }
                let (name, schema, label, summary, kind) = {
                    let row = &self.rows[i];
                    // SQL Server 전 DB 검색 = 스키마 칸에 `DB.스키마`(어느 DB인지 보이게 · 협업 V1 10-15).
                    let schema = match row.db.as_deref().filter(|d| !d.is_empty()) {
                        Some(db) => format!("{db}.{}", row.schema),
                        None => row.schema.clone(),
                    };
                    (
                        row.name.clone(),
                        schema,
                        row.label.clone(),
                        row.summary(),
                        row.kind,
                    )
                };
                let img = self.icon(kind, icon_px);
                dc.image(lr.x + px(4.0), y + (self.row_h - icon_px) / 2, &img, lr);
                let ty = dc.text_center_y(y, self.row_h);
                let mut tx = lr.x + col_icon;
                dc.text(
                    tx,
                    ty,
                    Rect::new(tx, y, w_kind - px(6.0), self.row_h),
                    &kind.code().to_ascii_uppercase().replace('_', " "),
                    th.text_dim,
                );
                tx += w_kind;
                dc.text(
                    tx,
                    ty,
                    Rect::new(tx, y, w_name - px(6.0), self.row_h),
                    &name,
                    th.text,
                );
                tx += w_name;
                dc.text(
                    tx,
                    ty,
                    Rect::new(tx, y, w_schema - px(6.0), self.row_h),
                    &schema,
                    th.text_dim,
                );
                tx += w_schema;
                dc.text(
                    tx,
                    ty,
                    Rect::new(tx, y, w_server - px(6.0), self.row_h),
                    &label,
                    th.text_dim,
                );
                tx += w_server;
                dc.text(
                    tx,
                    ty,
                    Rect::new(tx, y, (lr.right() - tx).max(0), self.row_h),
                    &summary,
                    th.text,
                );
                y += self.row_h;
                v += 1;
            }
            dc.pop_clip();
            let content_h = (n_view as i32 * self.row_h).max(lr.h);
            self.bars
                .paint(&mut dc, th, lr, lr.w, content_h, 0, self.scroll_y, s);
            dc.fill_rect(self.split_band, th.border);
            // 미리보기
            let pr = self.pre_rect;
            dc.fill_rect(pr, th.field_bg);
            dc.push_clip(pr);
            let head_h = self.row_h + px(6.0);
            dc.fill_rect(Rect::new(pr.x, pr.y, pr.w, head_h), th.chrome_bg);
            let head = if self.pre_head.is_empty() {
                t(Msg::FoPreviewNone).to_string()
            } else {
                self.pre_head.clone()
            };
            let hy2 = dc.text_center_y(pr.y, head_h);
            dc.text(pr.x + px(8.0), hy2, pr, &head, th.text);
            let lh = self.row_h;
            let gutter = px(52.0);
            let mut ly = pr.y + head_h - self.pre_scroll;
            for l in &self.pre_lines {
                if ly + lh >= pr.y + head_h && ly < pr.bottom() {
                    let ty = dc.text_center_y(ly, lh);
                    if let Some(n) = l.no {
                        let ns = n.to_string();
                        let nw = dc.text_width(&ns);
                        dc.text(pr.x + gutter - nw - px(8.0), ty, pr, &ns, th.text_dim);
                        dc.fill_rect(Rect::new(pr.x + gutter - px(4.0), ly, 1, lh), th.border);
                    }
                    let tx = pr.x + gutter + px(4.0);
                    for (a, b) in &l.spans {
                        let before = dc.text_width(&l.text[..*a]);
                        let w = dc.text_width(&l.text[*a..*b]);
                        dc.fill_rect_alpha(
                            Rect::new(tx + before, ly + px(2.0), w, lh - px(4.0)),
                            th.warn,
                            0.45,
                        );
                    }
                    dc.text(
                        tx,
                        ty,
                        Rect::new(tx, ly, (pr.right() - tx).max(0), lh),
                        &l.text,
                        th.text,
                    );
                }
                ly += lh;
            }
            dc.pop_clip();
            // 컨트롤(팝업은 맨 뒤) · 혜성 링 = 검색어 상자 둘레(진행 중 계속 · 끝나면 완료 깜빡임)
            self.query.paint(&mut dc, th);
            self.schema.paint(&mut dc, th);
            self.filter.paint(&mut dc, th);
            for cb in self.all_checks() {
                cb.paint(&mut dc, th);
            }
            for b in self.buttons() {
                b.paint(&mut dc, th);
            }
            self.server.paint(&mut dc, th);
            for b in &self.opts {
                b.paint(&mut dc, th, s);
            }
            self.ring.paint(&mut dc, th, self.ring_rect, px(4.0), s);
            if let Some(b) = self.opts.iter().find(|b| {
                b.hover_since
                    .is_some_and(|t0| t0.elapsed().as_millis() >= TOOLTIP_MS)
            }) {
                draw_tooltip_in(&mut dc, th, b.rect, (0, wi), t(b.kind.tip()), s);
            }
            self.query.paint_popup(&mut dc, th);
            self.schema.paint_popup(&mut dc, th);
            self.filter.paint_popup(&mut dc, th);
            if self.menu.is_open() {
                self.menu.paint(&mut dc, th);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }

    // ── 자체 시험(키 주입 0 · 113 §7) ──────────────────────────────────────────────────────────────

    pub(crate) fn test_query(&mut self, spec: &str) -> FoAction {
        let (flags, text) = match spec.split_once(':') {
            Some((f, t)) if !f.is_empty() && f.chars().all(|c| matches!(c, 'c' | 'w' | 'r')) => {
                (f, t)
            }
            _ => ("", spec),
        };
        self.set_opt(BtnKind::Case, flags.contains('c'));
        self.set_opt(BtnKind::Word, flags.contains('w'));
        self.set_opt(BtnKind::Regex, flags.contains('r'));
        self.query.set_text(text);
        self.start()
    }

    /// 결과 거르기 글을 넣는다(자체 시험).
    pub(crate) fn test_filter(&mut self, text: &str) {
        self.filter.set_text(text);
        self.apply_view();
        self.redraw();
    }

    /// 상태 + 진행 두 줄 + 결과 행(보기 순서 · `종류|스키마|이름|서버|일치`) + 선택 행 미리보기 줄.
    pub(crate) fn dump(&self) -> String {
        let (l1, l2) = self.progress_lines();
        let p = &self.prog;
        let mut s = format!(
            "running={}|rows={}|view={}|servers={}/{}|schemas={}|tasks={}/{}|scanned={}|hits={}|notes={}|errors={}|opts={}{}{}|all_dbs={}\n{}\n{}\n",
            self.running,
            self.rows.len(),
            self.view.len(),
            p.servers_done,
            p.servers_total,
            p.schemas,
            p.tasks_done,
            p.tasks_total,
            p.scanned,
            p.hits,
            p.notes.join(";"),
            p.errors.join(";"),
            if self.opt(BtnKind::Case) { 'c' } else { '-' },
            if self.opt(BtnKind::Word) { 'w' } else { '-' },
            if self.opt(BtnKind::Regex) { 'r' } else { '-' },
            self.all_dbs.is_checked(),
            l1,
            l2
        );
        for &i in &self.view {
            let r = &self.rows[i];
            s.push_str(&format!(
                "{:?}|{}|{}|{}|{}\n",
                r.kind,
                r.schema,
                r.name,
                r.label,
                r.summary()
            ));
        }
        for l in &self.pre_lines {
            s.push_str(&format!(
                "pre|{}|{}|{}\n",
                l.no.unwrap_or(0),
                l.text,
                l.spans.len()
            ));
        }
        s
    }

    /// 보기 n번째 행을 고르고 더블클릭과 같은 동작.
    pub(crate) fn test_pick(&mut self, n: usize) -> FoAction {
        if n >= self.view.len() {
            return FoAction::None;
        }
        self.own_focus(Focus::List);
        self.select(Some(n));
        match self.sel_row() {
            Some(i) => self.activate(i, self.first_line(i)),
            None => FoAction::None,
        }
    }
}
