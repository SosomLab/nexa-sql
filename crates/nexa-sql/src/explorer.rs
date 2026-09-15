//! 오브젝트 탐색기(docs/28 · 사용자 09-15) — 메인 창 왼쪽 트리. **메타 전용 세션**을 별도 스레드가 가지며(D-46 separate)
//! 펼친 노드의 **직계 자식만 그때** 읽는다. 오류는 그 노드에만(⚠ + 사유 · 다시 펼치면 재시도) — 편집기 세션·앱 상태 무영향.
//!
//! 노드: Root(접속) → Schema → Folder(종류) → Object → Column(테이블/뷰만). 카탈로그 SQL은 `nsql-catalog`(CLI `nsql cat`과 공용).
//! 동작: 클릭 = 선택 · 글리프/더블클릭 = 펼침 · 더블클릭(테이블·뷰) = `SELECT *` 템플릿 새 탭 · 더블클릭(소스 있는 것) = 소스 새 탭 ·
//! 우클릭 = 메뉴(Select rows · Open source · Refresh · Copy name) · ↑↓←→ Enter.

use crate::exp_icons::{self, IconKind};
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{Color, Theme};
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{InputEvent, Key as CtlKey, ScrollBars};
use nexa_gfx::IconImage;
use nsql_catalog::{ColumnInfo, ObjectInfo, ObjectKind};
use nsql_core::{DbError, Dialect, Session};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Instant;

/// 트리 노드 종류.
#[derive(Clone, Debug)]
enum NodeKind {
    Root,
    Schema(String),
    Folder { schema: String, kind: ObjectKind },
    Object(ObjectInfo),
    Column(ColumnInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LoadState {
    Idle,
    Loading,
    Loaded,
    Error(String),
}

#[derive(Clone, Debug)]
struct Node {
    kind: NodeKind,
    depth: usize,
    children: Vec<usize>,
    expanded: bool,
    expandable: bool,
    state: LoadState,
}

/// 메타 스레드에 보내는 요청(`gen` = 접속 세대 · 옛 세대의 응답은 버린다).
enum Req {
    Open {
        gen: u64,
        spec: ConnectSpec,
    },
    Close,
    Schemas {
        gen: u64,
        node: usize,
    },
    Objects {
        gen: u64,
        node: usize,
        schema: String,
        kind: ObjectKind,
    },
    Columns {
        gen: u64,
        node: usize,
        schema: String,
        table: String,
    },
    Source {
        gen: u64,
        schema: String,
        kind: ObjectKind,
        name: String,
        title: String,
    },
    /// 라이브 로그 폴링(T-71) — 메타 세션으로 V$SESSION 또는 로그 테이블을 읽는다.
    Live {
        gen: u64,
        req: LiveReq,
    },
}

/// 라이브 폴링 결과 — (줄들, 마지막 시각).
pub(crate) type LiveResult = Result<(Vec<String>, Option<String>), String>;

/// 라이브 로그 요청(설정에서 호스트가 조립).
#[derive(Clone, Debug)]
pub(crate) struct LiveReq {
    pub sid: String,
    /// `session` | `table`.
    pub source: String,
    pub table: String,
    pub ts_col: String,
    pub text_col: String,
    /// 마지막으로 본 시각(`YYYY-MM-DD HH24:MI:SS.FF3`) — None이면 서버 현재 시각만 받아 온다(실행 시작 기준점).
    pub since: Option<String>,
}

/// 식별자 검사(테이블·컬럼 설정값) — 영숫자·`_`·`$`·`#`·`.`·`"`만.
fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#' | '.' | '"'))
}

fn live_query(s: &mut dyn Session, req: &LiveReq) -> LiveResult {
    let run = |s: &mut dyn Session, sql: &str| -> Result<nsql_core::ResultSet, String> {
        s.execute(&nsql_core::ExecRequest {
            sql: sql.to_string(),
            params: vec![],
        })
        .map(|r| r.result_sets.into_iter().next().unwrap_or_default())
        .map_err(|e| e.message)
    };
    let cell = |v: &nsql_core::Value| match v {
        nsql_core::Value::Null => String::new(),
        o => o.display(),
    };
    match req.source.as_str() {
        "session" => {
            let sid: i64 = req
                .sid
                .trim()
                .parse()
                .map_err(|_| "live: SID".to_string())?;
            let rs = run(
                s,
                &format!(
                    "SELECT NVL(client_info, ''), NVL(action, ''), NVL(module, ''), status FROM v$session WHERE sid = {sid}"
                ),
            )?;
            let lines = rs
                .rows
                .iter()
                .map(|r| {
                    let parts: Vec<String> = r
                        .iter()
                        .take(3)
                        .map(cell)
                        .filter(|p| !p.is_empty())
                        .collect();
                    parts.join(" · ")
                })
                .filter(|l| !l.is_empty())
                .collect();
            Ok((lines, None))
        }
        "table" => {
            if !ident_ok(&req.table) || !ident_ok(&req.ts_col) || !ident_ok(&req.text_col) {
                return Err("live: table/column name".into());
            }
            let fmt = "YYYY-MM-DD HH24:MI:SS.FF3";
            let Some(since) = &req.since else {
                let rs = run(
                    s,
                    &format!("SELECT TO_CHAR(SYSTIMESTAMP, '{fmt}') FROM dual"),
                )?;
                let now = rs.rows.first().and_then(|r| r.first()).map(cell);
                return Ok((Vec::new(), now));
            };
            let sql = format!(
                "SELECT TO_CHAR({ts}, '{fmt}'), {tx} FROM {tb} WHERE {ts} > TO_TIMESTAMP('{since}', '{fmt}') ORDER BY {ts}",
                ts = req.ts_col,
                tx = req.text_col,
                tb = req.table,
                since = since.replace('\'', "")
            );
            let rs = run(s, &sql)?;
            let mut last = None;
            let lines = rs
                .rows
                .iter()
                .map(|r| {
                    let t = r.first().map(cell).unwrap_or_default();
                    let x = r.get(1).map(cell).unwrap_or_default();
                    last = Some(t.clone());
                    format!("{t}  {x}")
                })
                .collect();
            Ok((lines, last))
        }
        _ => Ok((Vec::new(), None)),
    }
}

enum Resp {
    Opened {
        gen: u64,
        r: Result<(Dialect, String), String>,
    },
    Schemas {
        gen: u64,
        node: usize,
        r: Result<Vec<String>, String>,
    },
    Objects {
        gen: u64,
        node: usize,
        r: Result<Vec<ObjectInfo>, String>,
    },
    Columns {
        gen: u64,
        node: usize,
        r: Result<Vec<ColumnInfo>, String>,
    },
    Source {
        gen: u64,
        title: String,
        r: Result<String, String>,
    },
    Live {
        gen: u64,
        r: LiveResult,
    },
}

/// 호스트가 처리할 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExplorerAction {
    /// 새 편집기 탭에 텍스트(SELECT 템플릿 · 소스).
    OpenSql { title: String, text: String },
    /// 상태줄 한 줄.
    Status(String),
    /// 클립보드에 복사할 텍스트.
    Copy(String),
}

/// 틴트 아이콘 캐시 — `(종류, rgb)` → 이미지.
/// (종류 · 색 · **표시 크기 px**) → 미리 스케일한 아이콘(09-15 사전 스케일 캐시 — 매 프레임 bilinear 샘플링 제거).
type IconCache = HashMap<(IconKind, (u8, u8, u8), i32), Rc<IconImage>>;

/// 아이콘 기본 크기 16×16(논리 px · 기본 글꼴 17px 기준) — 글꼴 크기에 비례해 스케일(사용자 09-15).
const ICON_BASE_PX: f32 = 16.0;
const ICON_REF_FONT_PX: f32 = 17.0;

const ROW_H: f32 = 22.0;
const INDENT: f32 = 14.0;
const DBLCLICK_MS: u128 = 400;

pub(crate) struct Explorer {
    nodes: Vec<Node>,
    bounds: Rect,
    scale: f32,
    scroll: i32,
    bars: ScrollBars,
    selected: Option<usize>,
    hover: Option<usize>,
    /// 호버 행 = 1초에 걸쳐 서서히 진해짐(`IntentFade` Slow · 그리드·접속 목록과 같은 부품 · 사용자 09-15).
    hover_fade: IntentFade,
    tx: mpsc::Sender<Req>,
    rx: mpsc::Receiver<Resp>,
    gen: u64,
    dialect: Option<Dialect>,
    conn_desc: String,
    /// 루트 표시 = 프로필 이름(굵게) + 호스트:포트(흐리게 · docs/28 §1 · 사용자 09-15).
    profile_name: String,
    endpoint: String,
    menu: CtxMenu,
    actions: Vec<ExplorerAction>,
    last_click: Option<(usize, Instant)>,
    visible: bool,
    focused: bool,
    /// 오브젝트 아이콘(설정 `explorer.icons`) · 틴트 이미지 캐시 `(종류, rgb)`.
    icons_on: bool,
    icon_cache: IconCache,
    /// 마지막 페인트의 화면 행(노드 index · 부모) — `row_at`(MouseMove마다)이 다시 펼치지 않게(09-15 C).
    rows_cache: Vec<(Option<usize>, usize)>,
    /// 호스트가 알려 주는 현재 트리 글꼴 크기(논리 px) — 아이콘 스케일 기준.
    font_px: f32,
    /// 소스 요청 중(더블클릭 연타 방지).
    source_pending: bool,
    /// 마지막 페인트의 행 높이(글꼴 높이 + 여백 · 글꼴 크기를 따라간다 · 사용자 09-15).
    row_px: i32,
    /// 로딩 점 애니메이션(300ms 단계) — 로딩 중인 노드가 있을 때만 다시 그린다(nexa-dir2 "Loading…" 자리 · 사용자 09-15).
    dots_step: u64,
    /// 라이브 로그 응답(호스트가 가져간다) · 요청 진행 중 표시.
    live_results: Vec<LiveResult>,
    pub(crate) live_inflight: bool,
}

fn err_s(e: DbError) -> String {
    e.message
}

/// 메타 스레드 — 세션 하나 · 순차 처리 · 요청마다 `catch_unwind`(드라이버 패닉이 UI로 번지지 않게).
fn meta_thread(rx: mpsc::Receiver<Req>, tx: mpsc::Sender<Resp>, wake: Box<dyn Fn() + Send>) {
    let mut session: Option<Box<dyn Session>> = None;
    let mut cur_gen = 0u64;
    while let Ok(req) = rx.recv() {
        let resp = match req {
            Req::Open { gen, spec } => {
                session = None;
                cur_gen = gen;
                let default = spec.dialect.unwrap_or(Dialect::Oracle);
                let r = catch_unwind(AssertUnwindSafe(|| nsql_drivers::open(&spec, default)));
                let r = match r {
                    Ok(Ok(s)) => {
                        let d = s.dialect();
                        let desc = s.describe();
                        session = Some(s);
                        Ok((d, desc))
                    }
                    Ok(Err(e)) => Err(e.message),
                    Err(_) => Err("internal: driver panicked".into()),
                };
                Resp::Opened { gen, r }
            }
            Req::Close => {
                if let Some(mut s) = session.take() {
                    let _ = s.rollback();
                }
                continue;
            }
            Req::Schemas { gen, node } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| nsql_catalog::schemas(s).map_err(err_s));
                Resp::Schemas { gen, node, r }
            }
            Req::Objects {
                gen,
                node,
                schema,
                kind,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::objects(s, &schema, kind).map_err(err_s)
                });
                Resp::Objects { gen, node, r }
            }
            Req::Columns {
                gen,
                node,
                schema,
                table,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::columns(s, &schema, &table).map_err(err_s)
                });
                Resp::Columns { gen, node, r }
            }
            Req::Source {
                gen,
                schema,
                kind,
                name,
                title,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::source(s, &schema, kind, &name).map_err(err_s)
                });
                Resp::Source { gen, title, r }
            }
            Req::Live { gen, req } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| live_query(s, &req));
                Resp::Live { gen, r }
            }
        };
        if tx.send(resp).is_err() {
            break;
        }
        wake();
    }
}

fn with_session<T>(
    session: &mut Option<Box<dyn Session>>,
    f: impl FnOnce(&mut dyn Session) -> Result<T, String>,
) -> Result<T, String> {
    let Some(s) = session.as_mut() else {
        return Err(t(Msg::ExpNotConnected).to_string());
    };
    match catch_unwind(AssertUnwindSafe(|| f(s.as_mut()))) {
        Ok(r) => r,
        Err(_) => Err("internal: catalog panicked".into()),
    }
}

/// 종류 폴더 라벨(i18n).
fn folder_msg(kind: ObjectKind) -> Msg {
    match kind {
        ObjectKind::Table => Msg::ExpTables,
        ObjectKind::View => Msg::ExpViews,
        ObjectKind::MaterializedView => Msg::ExpMatViews,
        ObjectKind::Procedure => Msg::ExpProcedures,
        ObjectKind::Function => Msg::ExpFunctions,
        ObjectKind::Package => Msg::ExpPackages,
        ObjectKind::PackageBody => Msg::ExpPackageBodies,
        ObjectKind::Sequence => Msg::ExpSequences,
        ObjectKind::Trigger => Msg::ExpTriggers,
        ObjectKind::Index => Msg::ExpIndexes,
        ObjectKind::Synonym => Msg::ExpSynonyms,
        ObjectKind::Type => Msg::ExpTypes,
    }
}

/// 종류별 칩 색(작은 사각형 — 글리프 대신 · 폰트 무관).
fn kind_color(kind: ObjectKind, th: &Theme) -> Color {
    match kind {
        ObjectKind::Table => th.accent,
        ObjectKind::View | ObjectKind::MaterializedView => th.syn_string,
        ObjectKind::Procedure
        | ObjectKind::Function
        | ObjectKind::Package
        | ObjectKind::PackageBody => th.syn_keyword,
        ObjectKind::Trigger => th.warn,
        ObjectKind::Sequence | ObjectKind::Index | ObjectKind::Synonym | ObjectKind::Type => {
            th.syn_number
        }
    }
}

impl Explorer {
    pub(crate) fn new(wake: Box<dyn Fn() + Send>, visible: bool) -> Self {
        let (tx, req_rx) = mpsc::channel::<Req>();
        let (resp_tx, rx) = mpsc::channel::<Resp>();
        let _ = std::thread::Builder::new()
            .name("nsql-explorer".into())
            .spawn(move || meta_thread(req_rx, resp_tx, wake));
        let mut e = Explorer {
            nodes: Vec::new(),
            bounds: Rect::default(),
            scale: 1.0,
            scroll: 0,
            bars: ScrollBars::new(),
            selected: None,
            hover: None,
            hover_fade: IntentFade::with_speed(FadeSpeed::Slow),
            tx,
            rx,
            gen: 0,
            dialect: None,
            conn_desc: String::new(),
            profile_name: String::new(),
            endpoint: String::new(),
            menu: CtxMenu::new(),
            actions: Vec::new(),
            last_click: None,
            visible,
            focused: false,
            icons_on: true,
            icon_cache: HashMap::new(),
            rows_cache: Vec::new(),
            font_px: ICON_REF_FONT_PX,
            source_pending: false,
            row_px: 0,
            dots_step: 0,
            live_results: Vec::new(),
            live_inflight: false,
        };
        e.reset_tree();
        e
    }

    fn reset_tree(&mut self) {
        self.nodes = vec![Node {
            kind: NodeKind::Root,
            depth: 0,
            children: Vec::new(),
            expanded: false,
            expandable: false,
            state: LoadState::Idle,
        }];
        self.selected = None;
        self.hover = None;
        self.scroll = 0;
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// 트리 글꼴 크기(논리 px) — 아이콘 = 16 × (글꼴/17) × 배율.
    pub(crate) fn set_font_px(&mut self, px: f32) {
        self.font_px = px.max(1.0);
    }

    pub(crate) fn set_icons(&mut self, on: bool) {
        self.icons_on = on;
        if !on {
            self.icon_cache.clear();
        }
    }

    fn icon_for(&self, n: &Node) -> Option<(IconKind, (u8, u8, u8))> {
        Some(match &n.kind {
            NodeKind::Root => (
                IconKind::Dbms,
                self.dialect
                    .map_or(IconKind::Dbms.color(), exp_icons::dbms_color),
            ),
            NodeKind::Schema(_) => (IconKind::Schema, IconKind::Schema.color()),
            NodeKind::Folder { .. } => (IconKind::Folder, IconKind::Folder.color()),
            NodeKind::Object(o) => {
                let k = match o.kind {
                    ObjectKind::Table => IconKind::Table,
                    ObjectKind::View => IconKind::View,
                    ObjectKind::MaterializedView => IconKind::MatView,
                    ObjectKind::Procedure => IconKind::Procedure,
                    ObjectKind::Function => IconKind::Function,
                    ObjectKind::Package => IconKind::Package,
                    ObjectKind::PackageBody => IconKind::PackageBody,
                    ObjectKind::Sequence => IconKind::Sequence,
                    ObjectKind::Trigger => IconKind::Trigger,
                    ObjectKind::Index => IconKind::Index,
                    ObjectKind::Synonym => IconKind::Synonym,
                    ObjectKind::Type => IconKind::Type,
                };
                (k, k.color())
            }
            NodeKind::Column(_) => (IconKind::Column, IconKind::Column.color()),
        })
    }

    /// 표시 크기로 미리 스케일한 아이콘(캐시) — 페인트는 스케일 없이 그대로 찍는다.
    fn icon_image(&mut self, kind: IconKind, rgb: (u8, u8, u8), size: i32) -> Rc<IconImage> {
        self.icon_cache
            .entry((kind, rgb, size))
            .or_insert_with(|| {
                let base = exp_icons::image(kind, rgb);
                Rc::new(base.resized(size.max(1) as u32, size.max(1) as u32))
            })
            .clone()
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if !on {
            self.menu.close();
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        self.clamp_scroll();
    }

    /// 접속됨 — 메타 세션을 따로 연다(편집기 세션과 분리).
    pub(crate) fn connect(&mut self, spec: &ConnectSpec, profile_name: &str) {
        self.gen += 1;
        self.dialect = None;
        self.conn_desc = spec.redacted();
        self.profile_name = profile_name.to_string();
        self.endpoint = match (&spec.host, spec.port) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.clone(),
            _ => spec.database.clone().unwrap_or_default(),
        };
        self.reset_tree();
        self.nodes[0].state = LoadState::Loading;
        let _ = self.tx.send(Req::Open {
            gen: self.gen,
            spec: spec.clone(),
        });
    }

    pub(crate) fn disconnect(&mut self) {
        self.gen += 1;
        self.dialect = None;
        self.conn_desc.clear();
        self.profile_name.clear();
        self.endpoint.clear();
        self.reset_tree();
        let _ = self.tx.send(Req::Close);
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExplorerAction> {
        std::mem::take(&mut self.actions)
    }

    /// 라이브 로그 폴링 요청(메타 세션 · 진행 중이면 무시).
    pub(crate) fn live_poll(&mut self, req: LiveReq) {
        if self.live_inflight || self.dialect != Some(Dialect::Oracle) {
            return;
        }
        self.live_inflight = true;
        let _ = self.tx.send(Req::Live { gen: self.gen, req });
    }

    pub(crate) fn take_live(&mut self) -> Vec<LiveResult> {
        std::mem::take(&mut self.live_results)
    }

    /// 스레드 응답 반영 — 바뀐 게 있으면 true.
    pub(crate) fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok(resp) = self.rx.try_recv() {
            changed = true;
            match resp {
                Resp::Opened { gen, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok((d, desc)) => {
                            self.dialect = Some(d);
                            self.conn_desc = desc;
                            let root = &mut self.nodes[0];
                            root.expandable = true;
                            root.state = LoadState::Idle;
                            // 접속되면 스키마 1단계는 바로 펼친다(docs/28 §0-1 · 그 아래는 안 읽음).
                            self.toggle(0);
                        }
                        Err(e) => {
                            self.nodes[0].state = LoadState::Error(e);
                        }
                    }
                }
                Resp::Schemas { gen, node, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            let kids: Vec<Node> = list
                                .into_iter()
                                .map(|s| Node {
                                    kind: NodeKind::Schema(s),
                                    depth: 1,
                                    children: Vec::new(),
                                    expanded: false,
                                    expandable: true,
                                    state: LoadState::Idle,
                                })
                                .collect();
                            self.set_children(node, kids);
                            self.select_current_schema();
                        }
                        Err(e) => self.set_error(node, e),
                    }
                }
                Resp::Objects { gen, node, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            let depth = self.nodes[node].depth + 1;
                            let kids: Vec<Node> = list
                                .into_iter()
                                .map(|o| Node {
                                    expandable: o.kind.is_relation(),
                                    kind: NodeKind::Object(o),
                                    depth,
                                    children: Vec::new(),
                                    expanded: false,
                                    state: LoadState::Idle,
                                })
                                .collect();
                            self.set_children(node, kids);
                        }
                        Err(e) => self.set_error(node, e),
                    }
                }
                Resp::Columns { gen, node, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            let depth = self.nodes[node].depth + 1;
                            let kids: Vec<Node> = list
                                .into_iter()
                                .map(|c| Node {
                                    kind: NodeKind::Column(c),
                                    depth,
                                    children: Vec::new(),
                                    expanded: false,
                                    expandable: false,
                                    state: LoadState::Loaded,
                                })
                                .collect();
                            self.set_children(node, kids);
                        }
                        Err(e) => self.set_error(node, e),
                    }
                }
                Resp::Live { gen, r } => {
                    self.live_inflight = false;
                    if gen != self.gen {
                        continue;
                    }
                    self.live_results.push(r);
                }
                Resp::Source { gen, title, r } => {
                    self.source_pending = false;
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(text) => self.actions.push(ExplorerAction::OpenSql { title, text }),
                        Err(e) => self.actions.push(ExplorerAction::Status(e)),
                    }
                }
            }
        }
        changed
    }

    /// 접속 사용자의 스키마(= 접속 설명의 사용자)를 선택해 눈에 띄게.
    fn select_current_schema(&mut self) {
        let user = self
            .conn_desc
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split(['/', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if user.is_empty() {
            return;
        }
        let kids = self.nodes[0].children.clone();
        for i in kids {
            if let NodeKind::Schema(s) = &self.nodes[i].kind {
                if s.eq_ignore_ascii_case(&user) {
                    self.selected = Some(i);
                    self.toggle(i);
                    break;
                }
            }
        }
    }

    fn set_children(&mut self, node: usize, kids: Vec<Node>) {
        // 옛 자식은 버린다(인덱스는 재사용하지 않는다 — 단순함 우선 · 노드 수는 수천 규모).
        let old: Vec<usize> = std::mem::take(&mut self.nodes[node].children);
        for o in old {
            self.detach(o);
        }
        let mut ids = Vec::with_capacity(kids.len());
        for k in kids {
            self.nodes.push(k);
            ids.push(self.nodes.len() - 1);
        }
        self.nodes[node].children = ids;
        self.nodes[node].state = LoadState::Loaded;
        self.nodes[node].expanded = true;
        self.clamp_scroll();
    }

    fn detach(&mut self, i: usize) {
        let kids = std::mem::take(&mut self.nodes[i].children);
        for k in kids {
            self.detach(k);
        }
        self.nodes[i].expanded = false;
        if self.selected == Some(i) {
            self.selected = None;
        }
        if self.hover == Some(i) {
            self.hover = None;
        }
    }

    fn set_error(&mut self, node: usize, e: String) {
        self.nodes[node].state = LoadState::Error(e);
        self.nodes[node].expanded = true;
    }

    /// 펼침/접힘 · 처음 펼치면 로드 요청.
    fn toggle(&mut self, i: usize) {
        if !self.nodes[i].expandable {
            return;
        }
        if self.nodes[i].expanded {
            self.nodes[i].expanded = false;
            self.clamp_scroll();
            return;
        }
        self.nodes[i].expanded = true;
        if self.nodes[i].state == LoadState::Loaded {
            return;
        }
        self.load(i);
    }

    /// (재)로드 요청 — Schema는 로컬로 폴더를 만든다(서버 왕복 0).
    fn load(&mut self, i: usize) {
        let gen = self.gen;
        match self.nodes[i].kind.clone() {
            NodeKind::Root => {
                self.nodes[i].state = LoadState::Loading;
                let _ = self.tx.send(Req::Schemas { gen, node: i });
            }
            NodeKind::Schema(schema) => {
                let Some(d) = self.dialect else { return };
                let depth = self.nodes[i].depth + 1;
                let kids: Vec<Node> = nsql_catalog::kinds_for(d)
                    .iter()
                    .map(|k| Node {
                        kind: NodeKind::Folder {
                            schema: schema.clone(),
                            kind: *k,
                        },
                        depth,
                        children: Vec::new(),
                        expanded: false,
                        expandable: true,
                        state: LoadState::Idle,
                    })
                    .collect();
                self.set_children(i, kids);
            }
            NodeKind::Folder { schema, kind } => {
                self.nodes[i].state = LoadState::Loading;
                let _ = self.tx.send(Req::Objects {
                    gen,
                    node: i,
                    schema,
                    kind,
                });
            }
            NodeKind::Object(o) if o.kind.is_relation() => {
                self.nodes[i].state = LoadState::Loading;
                let _ = self.tx.send(Req::Columns {
                    gen,
                    node: i,
                    schema: o.schema,
                    table: o.name,
                });
            }
            _ => {}
        }
    }

    fn refresh(&mut self, i: usize) {
        let old: Vec<usize> = std::mem::take(&mut self.nodes[i].children);
        for o in old {
            self.detach(o);
        }
        self.nodes[i].state = LoadState::Idle;
        self.nodes[i].expanded = true;
        self.load(i);
    }

    /// 더블클릭/Enter — 테이블·뷰는 SELECT 템플릿 · 소스 있는 것은 소스 · 그 외는 펼침.
    fn activate(&mut self, i: usize) {
        match self.nodes[i].kind.clone() {
            NodeKind::Object(o) if o.kind.is_relation() => {
                let Some(d) = self.dialect else { return };
                self.actions.push(ExplorerAction::OpenSql {
                    title: format!("{}.sql", o.name),
                    text: nsql_catalog::select_template(d, &o.schema, &o.name),
                });
            }
            NodeKind::Object(o) if o.kind.has_source() => self.open_source(&o),
            NodeKind::Column(c) => self.actions.push(ExplorerAction::Copy(c.name)),
            _ => self.toggle(i),
        }
    }

    fn open_source(&mut self, o: &ObjectInfo) {
        if self.source_pending {
            return;
        }
        self.source_pending = true;
        let name = if o.kind == ObjectKind::Function || o.kind == ObjectKind::Procedure {
            // PG 오버로드 — 서명 포함 이름으로.
            if self.dialect == Some(Dialect::Postgres) && !o.extra.is_empty() {
                format!("{}({})", o.name, o.extra)
            } else {
                o.name.clone()
            }
        } else {
            o.name.clone()
        };
        self.actions.push(ExplorerAction::Status(tf(
            Msg::ExpLoadingSource,
            &[&o.name],
        )));
        let _ = self.tx.send(Req::Source {
            gen: self.gen,
            schema: o.schema.clone(),
            kind: o.kind,
            name,
            title: format!("{}.sql", o.name),
        });
    }

    fn visible_rows(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            out.push(i);
            let n = &self.nodes[i];
            if n.expanded {
                for &c in n.children.iter().rev() {
                    stack.push(c);
                }
            }
        }
        out
    }

    fn row_h(&self) -> i32 {
        if self.row_px > 0 {
            self.row_px
        } else {
            (ROW_H * self.scale).round() as i32
        }
    }

    /// 상태 행(자식 자리)을 보일 노드인가 — 로딩은 펼침과 무관(접속 중 루트 포함) · 오류는 펼쳤을 때.
    fn shows_status_row(n: &Node) -> bool {
        match n.state {
            LoadState::Loading => true,
            LoadState::Error(_) => n.expanded,
            _ => false,
        }
    }

    fn content_h(&self) -> i32 {
        let mut n = self.visible_rows().len() as i32;
        // 오류/로딩 행(자식 자리) 하나씩.
        n += self
            .nodes
            .iter()
            .filter(|x| Self::shows_status_row(x))
            .count() as i32;
        n * self.row_h()
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.bounds.h).max(0);
        self.scroll = self.scroll.clamp(0, max);
    }

    /// 화면 행 목록: (노드 index 또는 None = 상태 행, 상태 행의 부모).
    fn screen_rows(&self) -> Vec<(Option<usize>, usize)> {
        let mut out = Vec::new();
        for i in self.visible_rows() {
            out.push((Some(i), i));
            if Self::shows_status_row(&self.nodes[i]) {
                out.push((None, i));
            }
        }
        out
    }

    /// 마우스 아래 행 — **마지막 페인트의 행 캐시**로 판정(구조가 바뀌면 곧 다시 그려져 캐시가 따라온다).
    fn row_at(&self, p: Point) -> Option<(Option<usize>, usize)> {
        if !self.bounds.contains(p) {
            return None;
        }
        let idx = (p.y - self.bounds.y + self.scroll) / self.row_h();
        if idx < 0 {
            return None;
        }
        self.rows_cache.get(idx as usize).copied()
    }

    fn ensure_visible(&mut self, i: usize) {
        let rows = self.screen_rows();
        if let Some(pos) = rows.iter().position(|(n, _)| *n == Some(i)) {
            let top = pos as i32 * self.row_h();
            if top < self.scroll {
                self.scroll = top;
            } else if top + self.row_h() > self.scroll + self.bounds.h {
                self.scroll = top + self.row_h() - self.bounds.h;
            }
            self.clamp_scroll();
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.bars.tick(now_ms);
        let b = self.hover_fade.tick(now_ms);
        // 로딩 점(…)은 300ms마다 한 단계 — 로딩 노드가 있을 때만.
        let mut c = false;
        if self.is_loading() {
            let step = now_ms / 300;
            if step != self.dots_step {
                self.dots_step = step;
                c = true;
            }
        }
        a || b || c
    }

    fn is_loading(&self) -> bool {
        self.nodes.iter().any(|n| n.state == LoadState::Loading)
    }

    /// 호스트의 빠른 타이머(≈30ms)를 유지해야 하는가 — 스크롤바 · 호버 페이드 · 로딩 애니메이션.
    pub(crate) fn bars_visible(&self) -> bool {
        self.visible
            && (self.bars.is_visible() || self.hover_fade.is_animating() || self.is_loading())
    }

    /// 이벤트 — 다시 그려야 하면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        // 열린 우클릭 메뉴가 먼저(바깥 클릭은 닫고 통과 · CLAUDE.md §3).
        if self.menu.is_open() {
            let consumed = self.menu.on_event(ev);
            if let Some(id) = self.menu.take_picked() {
                self.menu_pick(&id);
                return true;
            }
            if consumed {
                return true;
            }
        }
        let content_h = self.content_h();
        let (_, ny, consumed) = self.bars.on_event(
            ev,
            self.bounds,
            self.bounds.w,
            content_h.max(self.bounds.h),
            0,
            self.scroll,
            self.scale,
        );
        if ny != self.scroll {
            self.scroll = ny;
            self.clamp_scroll();
        }
        if consumed {
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let h = self.row_at(Point { x, y }).and_then(|(n, _)| n);
                self.hover_fade.set(h);
                if h != self.hover {
                    self.hover = h;
                    return true;
                }
                false
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                let Some((node, parent)) = self.row_at(p) else {
                    return false;
                };
                let Some(i) = node else {
                    // 상태 행(⚠) 클릭 = 재시도.
                    if matches!(self.nodes[parent].state, LoadState::Error(_)) {
                        self.refresh(parent);
                    }
                    return true;
                };
                self.selected = Some(i);
                let glyph_x = self.bounds.x
                    + ((self.nodes[i].depth as f32 * INDENT + 4.0) * self.scale).round() as i32;
                let glyph_w = (16.0 * self.scale).round() as i32;
                let now = Instant::now();
                let dbl = matches!(self.last_click, Some((j, t)) if j == i && now.duration_since(t).as_millis() < DBLCLICK_MS);
                self.last_click = Some((i, now));
                if dbl {
                    self.last_click = None;
                    self.activate(i);
                } else if x >= glyph_x && x < glyph_x + glyph_w {
                    self.toggle(i);
                }
                true
            }
            InputEvent::RightDown { x, y } => {
                let p = Point { x, y };
                let Some((Some(i), _)) = self.row_at(p) else {
                    return false;
                };
                self.selected = Some(i);
                let mut items = Vec::new();
                match &self.nodes[i].kind {
                    NodeKind::Object(o) => {
                        if o.kind.is_relation() {
                            items.push(CtxItem::item("select", t(Msg::ExpSelectRows)));
                        }
                        if o.kind.has_source() {
                            items.push(CtxItem::item("source", t(Msg::ExpOpenSource)));
                        }
                        items.push(CtxItem::item("copy", t(Msg::ExpCopyName)));
                        if o.kind.is_relation() {
                            items.push(CtxItem::item("refresh", t(Msg::ExpRefresh)));
                        }
                    }
                    NodeKind::Column(_) => items.push(CtxItem::item("copy", t(Msg::ExpCopyName))),
                    NodeKind::Schema(_) => {
                        items.push(CtxItem::item("copy", t(Msg::ExpCopyName)));
                        items.push(CtxItem::item("refresh", t(Msg::ExpRefresh)));
                    }
                    _ => items.push(CtxItem::item("refresh", t(Msg::ExpRefresh))),
                }
                let text_w = (160.0 * self.scale).round() as i32;
                self.menu.open_at(x, y, items, self.bounds, text_w);
                true
            }
            InputEvent::Key { key, .. } if self.focused => {
                let rows: Vec<usize> = self.visible_rows();
                let pos = self
                    .selected
                    .and_then(|s| rows.iter().position(|&r| r == s));
                match key {
                    CtlKey::Down => {
                        let np = pos.map_or(0, |p| (p + 1).min(rows.len().saturating_sub(1)));
                        if let Some(&n) = rows.get(np) {
                            self.selected = Some(n);
                            self.ensure_visible(n);
                        }
                    }
                    CtlKey::Up => {
                        let np = pos.map_or(0, |p| p.saturating_sub(1));
                        if let Some(&n) = rows.get(np) {
                            self.selected = Some(n);
                            self.ensure_visible(n);
                        }
                    }
                    CtlKey::Right => {
                        if let Some(i) = self.selected {
                            if !self.nodes[i].expanded {
                                self.toggle(i);
                            }
                        }
                    }
                    CtlKey::Left => {
                        if let Some(i) = self.selected {
                            if self.nodes[i].expanded {
                                self.toggle(i);
                            }
                        }
                    }
                    CtlKey::Enter => {
                        if let Some(i) = self.selected {
                            self.activate(i);
                        }
                    }
                    CtlKey::Escape => self.menu.close(),
                    _ => return false,
                }
                true
            }
            _ => false,
        }
    }

    fn menu_pick(&mut self, id: &str) {
        let Some(i) = self.selected else { return };
        match id {
            "select" | "source" => self.activate(i),
            "refresh" => self.refresh(i),
            "copy" => {
                let name = match &self.nodes[i].kind {
                    NodeKind::Schema(s) => s.clone(),
                    NodeKind::Object(o) => o.name.clone(),
                    NodeKind::Column(c) => c.name.clone(),
                    _ => String::new(),
                };
                if !name.is_empty() {
                    self.actions.push(ExplorerAction::Copy(name));
                }
            }
            _ => {}
        }
    }

    fn label(&self, i: usize) -> (String, String) {
        let n = &self.nodes[i];
        match &n.kind {
            NodeKind::Root => {
                if self.conn_desc.is_empty() {
                    (t(Msg::ExpNotConnected).to_string(), String::new())
                } else if self.profile_name.is_empty() {
                    (self.endpoint.clone(), String::new())
                } else {
                    (self.profile_name.clone(), self.endpoint.clone())
                }
            }
            NodeKind::Schema(s) => (s.clone(), String::new()),
            NodeKind::Folder { kind, .. } => {
                let base = t(folder_msg(*kind)).to_string();
                if n.state == LoadState::Loaded {
                    (format!("{base} ({})", n.children.len()), String::new())
                } else {
                    (base, String::new())
                }
            }
            NodeKind::Object(o) => {
                let extra = if o.kind == ObjectKind::Function || o.kind == ObjectKind::Procedure {
                    if o.extra.is_empty() {
                        String::new()
                    } else {
                        format!("({})", o.extra)
                    }
                } else {
                    String::new()
                };
                (format!("{}{extra}", o.name), o.status.clone())
            }
            NodeKind::Column(c) => (
                c.name.clone(),
                format!(
                    "{}{}",
                    c.data_type,
                    if c.nullable { "" } else { " NOT NULL" }
                ),
            ),
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let b = self.bounds;
        let s = self.scale;
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        dc.select_font(FontSlot::Base, false);
        let th_txt = dc.text_height();
        let asc = dc.text_ascent();
        // 행 높이·셰브론·칩은 글꼴 높이에 비례(글꼴을 키우면 같이 커진다).
        self.row_px = th_txt + (7.0 * s).round() as i32;
        let row_h = self.row_h();
        let indent = (INDENT * s).round() as i32;
        let chip = (th_txt as f32 * 0.55).round() as i32;
        self.rows_cache = self.screen_rows();
        let rows = std::mem::take(&mut self.rows_cache);
        let first = (self.scroll / row_h).max(0) as usize;
        for (pos, (node, parent)) in rows.iter().enumerate().skip(first) {
            let y = b.y + pos as i32 * row_h - self.scroll;
            if y >= b.bottom() {
                break;
            }
            let rr = Rect::new(b.x, y, b.w, row_h).intersection(&b);
            if rr.h <= 0 {
                continue;
            }
            let ty = y + (row_h - th_txt) / 2;
            // 글리프 시각 중심(대문자·한글 몸통) — 셰브론·칩을 여기에 맞춘다(사용자 09-15 "폰트 기준 세로 중앙").
            let vcy = ty + (asc as f32 * 0.62).round() as i32;
            match node {
                None => {
                    // 상태 행: 로딩 중 / 오류(클릭 = 재시도).
                    let depth = self.nodes[*parent].depth + 1;
                    let x = b.x + ((depth as f32 * INDENT + 8.0) * s).round() as i32;
                    match &self.nodes[*parent].state {
                        LoadState::Loading => {
                            // "Loading" + 점 0~3개(300ms 단계) — 비동기 로드 중임이 보이게(nexa-dir2 · 사용자 09-15).
                            let base = t(Msg::ExpLoading).trim_end_matches('…');
                            let dots = ".".repeat((self.dots_step % 4) as usize);
                            dc.text(x, ty, rr, &format!("{base}{dots}"), th.text_dim);
                        }
                        LoadState::Error(e) => {
                            let msg = format!("⚠ {}", e.lines().next().unwrap_or(""));
                            dc.text(x, ty, rr, &msg, th.warn);
                        }
                        _ => {}
                    }
                }
                Some(i) => {
                    let n = &self.nodes[*i];
                    if self.selected == Some(*i) {
                        dc.fill_rect(
                            rr,
                            if self.focused {
                                th.sel_bg
                            } else {
                                th.sel_bg_inactive
                            },
                        );
                    } else {
                        // 호버 = 전경색을 알파로 덮어 서서히(진행도 × 토큰 알파).
                        let ha = hover_alpha(false, self.hover_fade.value(*i));
                        if ha > 0.0 {
                            dc.fill_rect_alpha(rr, th.text, ha);
                        }
                    }
                    let gx = b.x + ((n.depth as f32 * INDENT + 4.0) * s).round() as i32;
                    // 셰브론(nexa-dir2 파일 그리드와 같은 부품 · 사용자 09-15) — 읽어서 자식이 없으면 그리지 않는다.
                    let empty_loaded = n.state == LoadState::Loaded && n.children.is_empty();
                    // ★ 부분적으로 잘린 마지막 행은 셰브론·아이콘을 그리지 않는다(클립이 없는 도형이라 상태줄 위로 삐져나왔다 · 사용자 09-15).
                    let full_row = rr.h >= row_h;
                    if n.expandable && !empty_loaded && full_row {
                        let cw = th_txt.max(10); // 사용자 09-15: 글꼴 높이의 1.0배
                        let chev = Rect::new(gx, vcy - cw / 2, cw, cw);
                        // 색: 접힘 = 진한 회색 · 마우스 오버 또는 펼침 = 본문색(검정) (사용자 09-15).
                        let color = if n.expanded || self.hover == Some(*i) {
                            th.text
                        } else {
                            th.text_dim
                        };
                        nexa_ctl::controls::draw_chevron_90(dc, chev, color, n.expanded);
                    }
                    let mut x = gx + (16.0 * s).round() as i32;
                    // 아이콘(설정 켬 · DBMS/스키마/폴더/종류별 · 글꼴 높이 크기) 또는 색 칩(끔).
                    if self.icons_on && full_row {
                        if let Some((k, rgb)) = self.icon_for(n) {
                            let sz = (ICON_BASE_PX * self.font_px / ICON_REF_FONT_PX * s)
                                .round()
                                .max(8.0) as i32;
                            let img = self.icon_image(k, rgb, sz);
                            let dst = Rect::new(x, vcy - sz / 2, sz, sz);
                            dc.image_scaled(dst, &img, rr);
                            x += sz + (6.0 * s).round() as i32;
                        }
                    } else if let NodeKind::Object(o) = &n.kind {
                        let cr = Rect::new(x, vcy - chip / 2, chip, chip);
                        dc.fill_round_rect(cr, 2, kind_color(o.kind, th));
                        x += chip + (6.0 * s).round() as i32;
                    } else if let NodeKind::Column(_) = &n.kind {
                        let cr = Rect::new(x + 2, vcy - chip / 4, chip / 2, chip / 2);
                        dc.fill_round_rect(cr, 2, th.text_dim);
                        x += chip + (6.0 * s).round() as i32;
                    }
                    let (label, sub) = self.label(*i);
                    // 메뉴와 같은 글꼴·크기·굵기(DBeaver 캡처 기준 · 사용자 09-15) — 굵게 없음.
                    dc.select_font(FontSlot::Base, false);
                    dc.text(x, ty, rr, &label, th.text);
                    let lw = dc.text_width(&label);
                    dc.select_font(FontSlot::Base, false);
                    if !sub.is_empty() {
                        let color = if sub == "INVALID" {
                            th.danger
                        } else {
                            th.text_dim
                        };
                        dc.text(x + lw + (8.0 * s).round() as i32, ty, rr, &sub, color);
                    }
                    let _ = indent;
                }
            }
        }
        self.bars
            .paint(dc, th, b, b.w, self.content_h().max(b.h), 0, self.scroll, s);
        self.menu.paint(dc, th);
        self.rows_cache = rows;
    }
}
