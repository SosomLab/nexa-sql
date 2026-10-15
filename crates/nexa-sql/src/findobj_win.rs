//! ★ **객체 찾기 창**(docs/113 · 사용자 10-15): 모달리스 · 메인 소유 · **연결된 전체 서버**(탐색기 칸 = 카탈로그) 대상
//! 이름 · 설명 · 컬럼 · 소스 본문 검색. 조건 1행(검색어 · Aa · ab · (.*) · 검색/중지) · 범위(서버 · 스키마) · 대상 4 · 종류 4 ·
//! 결과 목록(객체 하나 = 한 행 · 종류 아이콘 · 일치 요약) · 미리보기(일치 줄 · 강조) · 더블클릭/Enter = 소스 탭 · 우클릭 = 탐색기 메뉴 부분집합.
//! 엔진은 서버별 메타 스레드(`explorer.rs` `Req::Find` 3단계 · 점증 · 취소) — 이 창은 응답을 **합쳐 보여 주기만** 한다(113 §4).
use crate::exp_icons::{self, IconKind};
use crate::explorer::{FindHit, FindQuery, FindStage, FindWhat};
use nexa_ctl::controls::combo::{Combo, ComboItem};
use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::controls::{Button, Checkbox, ComboControl};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, IconImage, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, ScrollBars, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_catalog::find::FindPat;
use nsql_catalog::{ObjectInfo, ObjectKind};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;
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
    /// 검색 시작(조건 · 서버 하나 또는 전체) — 호스트가 탐색기에 요청하고 `started(n)`으로 서버 수를 돌려준다.
    Search(Box<FindQuery>, Option<ConnectSpec>),
    Stop(u64),
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

impl Row {
    fn key(&self) -> (String, String, String, String, ObjectKind) {
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

    /// 전체 이름(서버 종류별 · 인용 없음 · 28 §11 규칙의 단순판 = SQL Server `DB.스키마.이름`).
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
    List,
}

/// 미리보기 한 줄 = (줄 번호 · 글 · 강조 구간들(바이트)).
struct PreLine {
    no: Option<u32>,
    text: String,
    spans: Vec<(usize, usize)>,
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
    // 조건
    query: TextBox,
    schema: TextBox,
    opt_case: Checkbox,
    opt_word: Checkbox,
    opt_regex: Checkbox,
    run_btn: Button,
    server: Combo,
    servers: Vec<(Option<ConnectSpec>, String)>,
    targets: Vec<Checkbox>,
    target_labels: Vec<String>,
    kinds: Vec<Checkbox>,
    kind_labels: Vec<String>,
    // 결과
    rows: Vec<Row>,
    index: HashMap<(String, String, String, String, ObjectKind), usize>,
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
    split_drag: Option<i32>,
    split_band: Rect,
    pre_rect: Rect,
    pre_lines: Vec<PreLine>,
    pre_scroll: i32,
    pre_head: String,
    // 진행
    rev: u64,
    running: bool,
    servers_total: usize,
    servers_done: usize,
    started: Instant,
    elapsed_ms: u128,
    notes: Vec<String>,
    status: String,
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
        let target_labels = tmsgs.iter().map(|m| t(*m).to_string()).collect();
        let kinds = kmsgs.iter().map(|m| Checkbox::new(t(*m), true)).collect();
        let kind_labels = kmsgs.iter().map(|m| t(*m).to_string()).collect();
        Self {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            memo: crate::wingeom::Memo::default(),
            last: None,
            query: TextBox::new(t(Msg::FoQueryHint)),
            schema: TextBox::new(t(Msg::FoSchemaHint)),
            opt_case: Checkbox::new("Aa", false),
            opt_word: Checkbox::new("ab", false),
            opt_regex: Checkbox::new("(.*)", false),
            run_btn: Button::new(t(Msg::FoSearch)),
            server: Combo::new(vec![ComboItem::new("", t(Msg::FoServerAll))], 0),
            servers: Vec::new(),
            targets,
            target_labels,
            kinds,
            kind_labels,
            rows: Vec::new(),
            index: HashMap::new(),
            sel: None,
            hover: None,
            scroll_y: 0,
            bars: ScrollBars::new(),
            list_rect: Rect::default(),
            row_h: 0,
            focus: Focus::Query,
            last_click: None,
            split: 0.62,
            split_drag: None,
            split_band: Rect::default(),
            pre_rect: Rect::default(),
            pre_lines: Vec::new(),
            pre_scroll: 0,
            pre_head: String::new(),
            rev: 0,
            running: false,
            servers_total: 0,
            servers_done: 0,
            started: Instant::now(),
            elapsed_ms: 0,
            notes: Vec::new(),
            status: t(Msg::FoStatusIdle).to_string(),
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

    /// 설정(`findobj.*`)을 받는다 — 열 때·설정 변경 때.
    pub(crate) fn set_limits(&mut self, max_hits: usize, lines_per_object: usize) {
        self.max_hits = max_hits;
        self.lines_per_object = lines_per_object;
    }

    /// 기억된 대상·종류 체크(`findobj.targets`/`kinds` = 쉼표 목록).
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

    /// 지금 체크 상태(설정에 남기기).
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

    /// 서버 콤보 채우기(연결된 칸 목록 · 열 때·연결 변동 때).
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
            self.focus = Focus::Query;
            self.query.set_focused(true);
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
                default_size: (980.0, 680.0),
                ime: true,
            },
        ) else {
            return;
        };
        self.scale = o.scale;
        self.surface = o.surface;
        self.window = Some(o.window);
        self.focus = Focus::Query;
        self.query.set_focused(true);
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.last = self.window.as_deref().and_then(crate::winhost::last_geom);
        self.surface = None;
        self.window = None;
        self.menu.close();
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

    /// 조건 → 질의(비어 있으면 None).
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
                case_sensitive: self.opt_case.is_checked(),
                whole_word: self.opt_word.is_checked(),
            },
            regex: self.opt_regex.is_checked(),
            names: tg[0],
            comments: tg[1],
            columns: tg[2],
            source: tg[3],
            kinds,
            kinds_all,
            schemas,
            max_hits: self.max_hits,
            lines_per_object: self.lines_per_object,
        })
    }

    fn selected_server(&self) -> Option<ConnectSpec> {
        let v = self.server.selected_value();
        v.parse::<usize>()
            .ok()
            .and_then(|i| self.servers.get(i))
            .and_then(|(k, _)| k.clone())
    }

    /// 검색 시작(Enter · 버튼 · 기동 명령) — 결과 비우고 rev 올림 · 호스트 동작.
    fn start(&mut self) -> FoAction {
        let Some(q) = self.build_query() else {
            self.status = t(Msg::FoStatusNeedQuery).to_string();
            self.redraw();
            return FoAction::None;
        };
        self.rev += 1;
        self.rows.clear();
        self.index.clear();
        self.sel = None;
        self.hover = None;
        self.scroll_y = 0;
        self.pre_lines.clear();
        self.pre_head.clear();
        self.notes.clear();
        self.running = true;
        self.servers_total = 0;
        self.servers_done = 0;
        self.started = Instant::now();
        self.elapsed_ms = 0;
        self.run_btn.set_label(t(Msg::FoStop));
        self.status = tf(Msg::FoStatusRunning, &["0", "?", "0", "0.0"]);
        self.redraw();
        FoAction::Search(Box::new(q), self.selected_server())
    }

    /// 호스트가 요청한 서버 수를 알린다(0 = 연결된 서버 없음).
    pub(crate) fn started(&mut self, servers: usize) {
        self.servers_total = servers;
        if servers == 0 {
            self.running = false;
            self.run_btn.set_label(t(Msg::FoSearch));
            self.status = t(Msg::FoNoServers).to_string();
        }
        self.refresh_status();
        self.redraw();
    }

    fn stop(&mut self) -> FoAction {
        if !self.running {
            return FoAction::None;
        }
        self.running = false;
        self.elapsed_ms = self.started.elapsed().as_millis();
        self.run_btn.set_label(t(Msg::FoSearch));
        self.refresh_status();
        self.redraw();
        FoAction::Stop(self.rev)
    }

    /// 서버 한 곳의 단계 응답을 합친다(rev가 다르면 버림).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_hits(
        &mut self,
        server: Option<ConnectSpec>,
        label: String,
        rev: u64,
        _stage: FindStage,
        hits: Vec<FindHit>,
        done: bool,
        note: Option<String>,
    ) {
        if rev != self.rev || !self.running {
            return;
        }
        for h in hits {
            let probe = Row {
                server: server.clone(),
                label: label.clone(),
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
        }
        if let Some(n) = note {
            let n = format!("{label}: {n}");
            if !self.notes.contains(&n) {
                self.notes.push(n);
            }
        }
        if done {
            self.servers_done += 1;
            if self.servers_total > 0 && self.servers_done >= self.servers_total {
                self.running = false;
                self.elapsed_ms = self.started.elapsed().as_millis();
                self.run_btn.set_label(t(Msg::FoSearch));
            }
        }
        self.sort_rows();
        self.refresh_status();
        self.redraw();
    }

    /// 자연 정렬(서버 → 스키마 → 이름) · 선택 행 유지.
    fn sort_rows(&mut self) {
        let keep = self.sel.map(|i| self.rows[i].key());
        self.rows.sort_by(|a, b| {
            nexa_ctl::natural::cmp_names(&a.label, &b.label)
                .then_with(|| nexa_ctl::natural::cmp_names(&a.schema, &b.schema))
                .then_with(|| nexa_ctl::natural::cmp_names(&a.name, &b.name))
        });
        self.index.clear();
        for (i, r) in self.rows.iter().enumerate() {
            self.index.insert(r.key(), i);
        }
        if let Some(k) = keep {
            self.sel = self.index.get(&k).copied();
            self.rebuild_preview();
        }
    }

    fn refresh_status(&mut self) {
        let secs = if self.running {
            self.started.elapsed().as_millis()
        } else {
            self.elapsed_ms
        } as f64
            / 1000.0;
        let mut s = if self.running {
            tf(
                Msg::FoStatusRunning,
                &[
                    &self.servers_done.to_string(),
                    &self.servers_total.to_string(),
                    &self.rows.len().to_string(),
                    &format!("{secs:.1}"),
                ],
            )
        } else if self.servers_total == 0 && self.rows.is_empty() && self.rev == 0 {
            t(Msg::FoStatusIdle).to_string()
        } else {
            tf(
                Msg::FoStatusDone,
                &[
                    &self.rows.len().to_string(),
                    &self.servers_total.to_string(),
                    &format!("{secs:.1}"),
                ],
            )
        };
        if let Some(n) = self.notes.first() {
            s.push_str(" · ");
            s.push_str(n);
        }
        self.status = s;
    }

    /// 진행 중이면 상태줄 시각을 갱신(호스트 틱).
    pub(crate) fn tick(&mut self) -> bool {
        if self.running && self.window.is_some() {
            self.refresh_status();
            return true;
        }
        false
    }

    // ── 미리보기 ───────────────────────────────────────────────────────────────────────────────────

    fn select(&mut self, i: Option<usize>) {
        if self.sel != i {
            self.sel = i;
            self.pre_scroll = 0;
            self.rebuild_preview();
        }
    }

    fn spans(&self, text: &str) -> Vec<(usize, usize)> {
        let needle = self.query.text().trim().to_string();
        if needle.is_empty() || self.opt_regex.is_checked() {
            return Vec::new();
        }
        let (hay, nd) = if self.opt_case.is_checked() {
            (text.to_string(), needle)
        } else {
            (text.to_lowercase(), needle.to_lowercase())
        };
        if hay.len() != text.len() {
            // 소문자화로 길이가 달라지는 글(특수 문자)은 강조 없이.
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
        let Some(i) = self.sel else {
            self.pre_head.clear();
            return;
        };
        let r = self.rows[i].clone();
        self.pre_head = format!(
            "{} ({} · {})",
            r.full_name(),
            format!("{:?}", r.kind).to_uppercase(),
            r.label
        );
        if r.name_hit {
            let text = r.name.clone();
            let spans = self.spans(&text);
            self.pre_lines.push(PreLine {
                no: None,
                text,
                spans,
            });
        }
        if let Some(c) = &r.comment {
            let text = format!("» {c}");
            let spans = self.spans(&text);
            self.pre_lines.push(PreLine {
                no: None,
                text,
                spans,
            });
        }
        for (c, ty) in &r.cols {
            let text = format!("{c}  {ty}");
            let spans = self.spans(&text);
            self.pre_lines.push(PreLine {
                no: None,
                text,
                spans,
            });
        }
        for (ln, text) in &r.src {
            let spans = self.spans(text);
            self.pre_lines.push(PreLine {
                no: Some(*ln),
                text: text.clone(),
                spans,
            });
        }
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

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) || self.row_h <= 0 {
            return None;
        }
        let i = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h) as usize;
        (i < self.rows.len()).then_some(i)
    }

    fn ensure_visible(&mut self, i: usize) {
        let top = i as i32 * self.row_h;
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
    }

    // ── 사건 ───────────────────────────────────────────────────────────────────────────────────────

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> FoAction {
        let mut inv = Invalidations::default();
        // 열린 우클릭 메뉴가 먼저(바깥 클릭 = 닫고 통과 · 팝업 규칙).
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
                // 바깥 클릭 = 닫고 그대로 진행.
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
                for b in [
                    &mut self.run_btn,
                    &mut self.open_btn,
                    &mut self.reveal_btn,
                    &mut self.close_btn,
                ] {
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
                if let Some(start) = self.split_drag {
                    let _ = start;
                    let total = (self.pre_rect.bottom() - self.list_rect.y).max(1) as f32;
                    let rel = (y - self.list_rect.y) as f32 / total;
                    self.split = rel.clamp(0.2, 0.9);
                    self.redraw();
                    return FoAction::None;
                }
                let mv = InputEvent::MouseMove { x, y };
                let p = Point { x, y };
                self.query.on_event(&mv, &mut inv);
                self.schema.on_event(&mv, &mut inv);
                if self.query.popup_open() || self.schema.popup_open() {
                    self.redraw();
                    return FoAction::None;
                }
                self.server.on_event(&mv, &mut inv);
                for cb in self.all_checks() {
                    cb.on_event(&mv, &mut inv);
                }
                for b in [
                    &mut self.run_btn,
                    &mut self.open_btn,
                    &mut self.reveal_btn,
                    &mut self.close_btn,
                ] {
                    b.on_event(&mv, &mut inv);
                }
                let h = self.row_at(p);
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
                    let content_h = (self.rows.len() as i32 * self.row_h).max(self.list_rect.h);
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
                    self.split_drag = None;
                }
                // 상자 팝업(편집 메뉴)이 열려 있으면 그 클릭은 상자에만.
                if self.query.popup_open() {
                    self.query.on_event(&e, &mut inv);
                    self.redraw();
                    return FoAction::None;
                }
                if self.schema.popup_open() {
                    self.schema.on_event(&e, &mut inv);
                    self.redraw();
                    return FoAction::None;
                }
                // 콤보 드롭다운이 열려 있으면 콤보가 먼저.
                if self.server.is_open() {
                    self.server.on_event(&e, &mut inv);
                    self.server.take_changed();
                    self.redraw();
                    return FoAction::None;
                }
                if !up && self.split_band.contains(p) {
                    self.split_drag = Some(y);
                    return FoAction::None;
                }
                // 마우스 라우팅 규칙: 누름은 커서 아래 컨트롤에만 · 뗌은 전부.
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
                if up || self.server.bounds().contains(p) {
                    self.server.on_event(&e, &mut inv);
                    self.server.take_changed();
                }
                for cb in self.all_checks() {
                    if up || cb.bounds().contains(p) {
                        cb.on_event(&e, &mut inv);
                    }
                    cb.set_focused(false);
                    cb.take_toggled();
                }
                let mut clicked: Option<usize> = None;
                for (k, b) in [
                    &mut self.run_btn,
                    &mut self.open_btn,
                    &mut self.reveal_btn,
                    &mut self.close_btn,
                ]
                .into_iter()
                .enumerate()
                {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    b.set_focused(false);
                    if b.take_clicked() {
                        clicked = Some(k);
                    }
                }
                if !up {
                    if let Some(i) = self.row_at(p) {
                        self.own_focus(Focus::List);
                        let now = Instant::now();
                        let dbl = matches!(self.last_click, Some((j, t)) if j == i && now.duration_since(t).as_millis() < 450);
                        self.last_click = Some((i, now));
                        self.select(Some(i));
                        if dbl {
                            self.last_click = None;
                            self.redraw();
                            return self.activate(i, self.first_line(i));
                        }
                    } else if self.list_rect.contains(p) {
                        self.own_focus(Focus::List);
                    }
                    // 미리보기 줄 더블클릭 = 그 줄로.
                    if self.pre_rect.contains(p) {
                        if let (Some(i), Some(ln)) = (self.sel, self.preview_line_at(p)) {
                            let now = Instant::now();
                            let dbl = matches!(self.last_click, Some((j, t)) if j == usize::MAX - ln as usize && now.duration_since(t).as_millis() < 450);
                            self.last_click = Some((usize::MAX - ln as usize, now));
                            if dbl {
                                self.last_click = None;
                                return self.activate(i, ln);
                            }
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
                        if let Some(i) = self.sel {
                            return self.activate(i, self.first_line(i));
                        }
                    }
                    Some(2) => {
                        if let Some(i) = self.sel {
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
                if self.query.bounds().contains(p) {
                    self.query
                        .on_event(&InputEvent::RightDown { x, y }, &mut inv);
                } else if self.schema.bounds().contains(p) {
                    self.schema
                        .on_event(&InputEvent::RightDown { x, y }, &mut inv);
                } else if let Some(i) = self.row_at(p) {
                    self.select(Some(i));
                    self.own_focus(Focus::List);
                    self.open_menu(i);
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                let tb = self.focused_box();
                if let Some(tb) = tb {
                    tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
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
                            if let Some(i) = self.sel {
                                return self.activate(i, self.first_line(i));
                            }
                        } else {
                            return self.start();
                        }
                    }
                    Key::Named(NamedKey::Tab) => {
                        let next = match self.focus {
                            Focus::Query => Focus::Schema,
                            Focus::Schema => Focus::List,
                            Focus::List => Focus::Query,
                        };
                        self.own_focus(next);
                        self.redraw();
                        return FoAction::None;
                    }
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp)
                        if self.focus == Focus::List
                            || !self.rows.is_empty() && self.focus != Focus::Schema =>
                    {
                        if self.rows.is_empty() {
                            return FoAction::None;
                        }
                        let down = kev.logical_key == Key::Named(NamedKey::ArrowDown);
                        let i = match self.sel {
                            None => 0,
                            Some(i) if down => (i + 1).min(self.rows.len() - 1),
                            Some(i) => i.saturating_sub(1),
                        };
                        self.own_focus(Focus::List);
                        self.select(Some(i));
                        self.ensure_visible(i);
                        self.redraw();
                        return FoAction::None;
                    }
                    Key::Character(c) if self.primary && self.focus == Focus::List => {
                        let lower = crate::input::shortcut_letter(kev)
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| c.to_lowercase());
                        if lower == "c" {
                            if let Some(i) = self.sel {
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
                    if let Some(tb) = self.focused_box() {
                        tb.on_event(&e, &mut inv);
                    }
                    self.redraw();
                }
            }
            _ => {}
        }
        FoAction::None
    }

    fn focused_box(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Query => Some(&mut self.query),
            Focus::Schema => Some(&mut self.schema),
            Focus::List => None,
        }
    }

    fn all_checks(&mut self) -> Vec<&mut Checkbox> {
        let mut v: Vec<&mut Checkbox> =
            vec![&mut self.opt_case, &mut self.opt_word, &mut self.opt_regex];
        v.extend(self.targets.iter_mut());
        v.extend(self.kinds.iter_mut());
        v
    }

    fn preview_line_at(&self, p: Point) -> Option<u32> {
        let lh = self.row_h.max(1);
        let head_h = lh + (6.0 * self.scale) as i32;
        let y = p.y - self.pre_rect.y - head_h + self.pre_scroll;
        if y < 0 {
            return None;
        }
        let i = (y / lh) as usize;
        self.pre_lines.get(i).and_then(|l| l.no)
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

    // ── 그리기 ─────────────────────────────────────────────────────────────────────────────────────

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
            let opt_w = px(44.0);
            let mut x = wi - pad - btn_w;
            self.run_btn.set_scale(s);
            self.run_btn
                .set_bounds(Rect::new(x, pad, btn_w, box_h), inv);
            for cb in [&mut self.opt_regex, &mut self.opt_word, &mut self.opt_case] {
                x -= opt_w + px(4.0);
                cb.set_scale(s);
                cb.set_bounds(Rect::new(x, pad, opt_w, box_h), inv);
            }
            self.query.set_scale(s);
            self.query
                .set_bounds(Rect::new(pad, pad, x - gap - pad, box_h), inv);
            // ── 2행: 서버 · 스키마
            let y2 = pad + box_h + gap;
            let combo_w = px(260.0);
            self.server.set_scale(s);
            self.server
                .set_bounds(Rect::new(pad, y2, combo_w, box_h), inv);
            let lab = t(Msg::FoSchemaLabel);
            let lab_w = dc.text_width(lab);
            let ly2 = dc.text_center_y(y2, box_h);
            dc.text(pad + combo_w + gap, ly2, clip, lab, th.text_dim);
            let sx = pad + combo_w + gap + lab_w + gap;
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
            // ── 바닥: 상태줄 + 버튼
            let btn_h = th_txt + px(14.0);
            let by = hi - pad - btn_h;
            let mut bx = wi - pad;
            for b in [
                &mut self.close_btn,
                &mut self.reveal_btn,
                &mut self.open_btn,
            ] {
                b.set_scale(s);
                let w = dc.text_width(&btn_label(b)) + px(28.0);
                bx -= w;
                b.set_bounds(Rect::new(bx, by, w, btn_h), inv);
                bx -= gap;
            }
            // ── 결과 목록 + 미리보기(스플리터)
            let top = y3 + box_h + gap;
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
            // 머리글
            dc.fill_rect(list_outer, th.field_bg);
            dc.fill_rect(
                Rect::new(list_outer.x, list_outer.y, list_outer.w, hdr_h),
                th.chrome_bg,
            );
            let col_icon = px(24.0);
            let w_name = (list_outer.w as f32 * 0.32) as i32;
            let w_schema = (list_outer.w as f32 * 0.16) as i32;
            let w_server = (list_outer.w as f32 * 0.22) as i32;
            let hy = dc.text_center_y(list_outer.y, hdr_h);
            let mut hx = list_outer.x + col_icon;
            dc.text(hx, hy, clip, t(Msg::FoColName), th.text_dim);
            hx += w_name;
            dc.text(hx, hy, clip, t(Msg::FoColSchema), th.text_dim);
            hx += w_schema;
            dc.text(hx, hy, clip, t(Msg::FoColServer), th.text_dim);
            hx += w_server;
            dc.text(hx, hy, clip, t(Msg::FoColMatch), th.text_dim);
            // 행
            let lr = self.list_rect;
            dc.push_clip(lr);
            let first = (self.scroll_y / self.row_h.max(1)).max(0) as usize;
            let rows_n = self.rows.len();
            let icon_px = px(16.0);
            let mut y = lr.y - (self.scroll_y % self.row_h.max(1));
            let mut i = first;
            while i < rows_n && y < lr.bottom() {
                let r = Rect::new(lr.x, y, lr.w, self.row_h);
                if self.sel == Some(i) {
                    dc.fill_rect(
                        r,
                        if self.focus == Focus::List {
                            th.sel_bg
                        } else {
                            th.sel_bg_inactive
                        },
                    );
                } else if self.hover == Some(i) {
                    dc.fill_rect_alpha(r, th.sel_bg, 0.35);
                }
                let (name, schema, label, summary, kind) = {
                    let row = &self.rows[i];
                    (
                        row.name.clone(),
                        row.schema.clone(),
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
                i += 1;
            }
            dc.pop_clip();
            let content_h = (rows_n as i32 * self.row_h).max(lr.h);
            self.bars
                .paint(&mut dc, th, lr, lr.w, content_h, 0, self.scroll_y, s);
            // 스플리터 띠
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
                    // 강조 구간: 앞 글 폭만큼 띄워 배경을 칠한다.
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
            // 상태줄
            let sy = dc.text_center_y(by, btn_h);
            let status_w = (bx - pad - gap).max(0);
            dc.text(
                pad,
                sy,
                Rect::new(pad, by, status_w, btn_h),
                &self.status,
                if self.running { th.text } else { th.text_dim },
            );
            // 컨트롤 그리기(팝업은 맨 뒤)
            self.query.paint(&mut dc, th);
            self.schema.paint(&mut dc, th);
            for cb in [&mut self.opt_case, &mut self.opt_word, &mut self.opt_regex] {
                cb.paint(&mut dc, th);
            }
            for cb in &mut self.targets {
                cb.paint(&mut dc, th);
            }
            for cb in &mut self.kinds {
                cb.paint(&mut dc, th);
            }
            for b in [
                &mut self.run_btn,
                &mut self.open_btn,
                &mut self.reveal_btn,
                &mut self.close_btn,
            ] {
                b.paint(&mut dc, th);
            }
            self.server.paint(&mut dc, th);
            self.query.paint_popup(&mut dc, th);
            self.schema.paint_popup(&mut dc, th);
            if self.menu.is_open() {
                self.menu.paint(&mut dc, th);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }

    // ── 자체 시험(키 주입 0 · 113 §7) ──────────────────────────────────────────────────────────────

    /// 검색어·옵션을 넣고 검색 시작(`findobj.query:<글>` · `c`=Aa · `w`=낱말 · `r`=정규식 접두 `[cwr]:`).
    pub(crate) fn test_query(&mut self, spec: &str) -> FoAction {
        let (flags, text) = match spec.split_once(':') {
            Some((f, t)) if f.chars().all(|c| matches!(c, 'c' | 'w' | 'r')) && !f.is_empty() => {
                (f, t)
            }
            _ => ("", spec),
        };
        self.opt_case.set_checked(flags.contains('c'));
        self.opt_word.set_checked(flags.contains('w'));
        self.opt_regex.set_checked(flags.contains('r'));
        self.query.set_text(text);
        self.start()
    }

    /// 상태 + 결과 행(`종류|스키마|이름|서버|일치`) + 선택 행 미리보기 줄.
    pub(crate) fn dump(&self) -> String {
        let mut s = format!(
            "status={}|running={}|rows={}|servers={}/{}|notes={}\n",
            self.status,
            self.running,
            self.rows.len(),
            self.servers_done,
            self.servers_total,
            self.notes.join(";")
        );
        for r in &self.rows {
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

    /// n번째 행을 고르고 더블클릭과 같은 동작.
    pub(crate) fn test_pick(&mut self, n: usize) -> FoAction {
        if n >= self.rows.len() {
            return FoAction::None;
        }
        self.own_focus(Focus::List);
        self.select(Some(n));
        self.activate(n, self.first_line(n))
    }
}

fn btn_label(b: &Button) -> String {
    b.label().to_string()
}
