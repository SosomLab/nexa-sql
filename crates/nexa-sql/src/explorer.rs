//! 오브젝트 탐색기(docs/28 · 사용자 09-15) — 메인 창 왼쪽 트리. **메타 전용 세션**을 별도 스레드가 가지며(D-46 separate)
//! 펼친 노드의 **직계 자식만 그때** 읽는다. 오류는 그 노드에만(⚠ + 사유 · 다시 펼치면 재시도) — 편집기 세션·앱 상태 무영향.
//!
//! 노드: Root(접속) → Schema → Folder(종류) → Object → Column(테이블/뷰만). 카탈로그 SQL은 `nsql-catalog`(CLI `nsql cat`과 공용).
//! 동작: 클릭 = 선택 · 글리프/더블클릭 = 펼침 · 더블클릭(테이블·뷰) = `SELECT *` 템플릿 새 탭 · 더블클릭(소스 있는 것) = 소스 새 탭 ·
//! 우클릭 = 메뉴(Select rows · Open source · Refresh · Copy name) · ↑↓←→ Enter.

use crate::dbms_icons;
use crate::exp_icons::{self, IconKind};
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{Color, Theme};
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{InputEvent, Key as CtlKey, ScrollBars};
use nexa_gfx::IconImage;
use nsql_catalog::{
    ColumnInfo, DetailSection, GenOpts, GenSpec, ObjectInfo, ObjectKind, SchemaOpts, SubIcon,
    SubItem, SubKind,
};
use nsql_core::{DbError, Dialect, Session};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// 트리 노드 종류.
#[derive(Clone, Debug)]
enum NodeKind {
    Root,
    Schema(String),
    Folder {
        schema: String,
        kind: ObjectKind,
    },
    Object(ObjectInfo),
    Column(ColumnInfo),
    /// ★ 객체 아래 하위 폴더(83 §1 · Columns·Constraints·Foreign Keys·…) — 표 `nsql_catalog::sub_kinds`가 만든다.
    Sub {
        owner: Box<ObjectInfo>,
        sub: SubKind,
    },
    /// 하위 폴더의 잎(제약·인덱스·트리거·인자 …).
    Item(SubItem),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LoadState {
    Idle,
    Loading,
    Loaded,
    /// ★ 부분(84 §3): 검색 인덱스가 올린 일치 객체만 든 폴더 — 전체 목록은 순차 완성 큐가(또는 사용자가 펼치면) 채운다.
    Partial,
    Error(String),
}

/// ★ 검색 인덱스 설정 한 벌(`explorer.search_index` · `explorer.index_max` · `explorer.index_hits_max` · 84 §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct IndexCfg {
    pub on: bool,
    /// 스키마 하나의 인덱스 상한(0 = 무제한).
    pub max: usize,
    /// 한 필터에 트리로 올리는 일치 상한.
    pub hits_max: usize,
    /// ★ L1 접속 직후 채움(85 §2): 검색이 없어도 스키마 하나씩 이름 인덱스를 채운다(백그라운드 메타 세션 · 다 읽으면 0).
    pub prefetch: bool,
    /// L1 스키마 사이 간격(ms).
    pub idle_ms: u64,
    /// ★ L2 워머(85 §3): 현재 스키마 관계의 컬럼을 뒤에서 미리 읽는 상한(0 = 끔) · 간격(ms).
    pub warm_columns_max: usize,
    pub warm_idle_ms: u64,
    /// ★ L3 회수(85 §4): 상세 상한 · 상세 TTL(s) · 현재 스키마 밖 컬럼 TTL(s) — 0 = 끔.
    pub detail_max: usize,
    pub detail_ttl_secs: u64,
    pub cols_ttl_secs: u64,
    /// ★ L1 디스크 캐시(85 §9 · `meta.disk_cache`): 접속마다 이름 층을 파일에 남기고 다음 실행 첫 검색·완성을 즉시.
    pub disk_cache: bool,
    /// ★ L2 코멘트 워머(86 §5 · `meta.warm_comments`): 관계 폴더를 읽은 스키마의 테이블·컬럼 코멘트를 뒤에서 채운다.
    pub warm_comments: bool,
}

impl Default for IndexCfg {
    fn default() -> Self {
        Self {
            on: true,
            max: 200_000,
            hits_max: 2000,
            prefetch: true,
            idle_ms: 250,
            warm_columns_max: 200,
            warm_idle_ms: 300,
            detail_max: 64,
            detail_ttl_secs: 300,
            cols_ttl_secs: 600,
            disk_cache: true,
            warm_comments: true,
        }
    }
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

/// 탐색기 타입어헤드 설정 한 벌(`explorer.typeahead*` · 호스트가 만들어 전 칸에 준다).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct TypeAheadCfg {
    pub enabled: bool,
    pub timeout_ms: u64,
    pub filter: nexa_ctl::TypeAheadFilter,
    pub pos: nexa_ctl::HudPos,
}

impl Default for TypeAheadCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: nexa_ctl::TYPEAHEAD_TIMEOUT_MS,
            filter: nexa_ctl::TypeAheadFilter::default(),
            pos: nexa_ctl::HudPos::BottomLeft,
        }
    }
}

/// 메타 스레드에 보내는 요청(`gen` = 접속 세대 · 옛 세대의 응답은 버린다).
enum Req {
    Open {
        gen: u64,
        spec: ConnectSpec,
        /// 비밀번호가 **일회성**(입력 창으로 받은 것)이다 — 접속에만 쓰고 지운다 · 유휴 재개용으로 기억하지 않는다.
        once: bool,
    },
    Close,
    /// 메타 세션만 닫는다(유휴 회수 · docs/52 §2-2) — 스펙은 기억해 두었다가 **다음 요청 때 조용히 다시 연다**.
    Suspend,
    /// ★ 백그라운드 메타 스레드용(09-24): 세션을 **열지 않고** 재개 스펙·세대만 기억한다 — 첫 요청이 올 때 연다(연결 +1은
    ///   미리 읽기/선적재가 실제로 있을 때만 · 26 §8).
    Prepare {
        gen: u64,
        spec: ConnectSpec,
    },
    Schemas {
        gen: u64,
        node: usize,
        opts: SchemaOpts,
    },
    Objects {
        gen: u64,
        node: usize,
        schema: String,
        kind: ObjectKind,
    },
    /// ★ 이름 인덱스(84 §2) — 스키마 하나의 (종류, 이름) 전부 · 검색 중일 때만 · 한 번에 하나.
    Index {
        gen: u64,
        schema: String,
        max: usize,
    },
    /// ★ 검색 매칭을 스레드로(85 §6 · 사용자 09-25 "검색과 애니메이션이 엮이지 않게 상태만 교환"): 백그라운드 메타 스레드가 자기가 읽은
    /// L1 이름 사본에 대해 판정하고 일치만 `Resp::Hits`로 — UI 스레드는 스캔 0 · 노드 삽입만.
    Search {
        gen: u64,
        rev: u64,
        matcher: crate::filterbar::Matcher,
        limit: usize,
    },
    SearchStop {
        gen: u64,
    },
    /// 디스크 캐시의 이름을 스레드 사본에 심는다(85 §9 · 서버가 이미 준 스키마는 건너뜀).
    NamesSeed {
        gen: u64,
        schema: String,
        list: Vec<nsql_catalog::NameEntry>,
    },
    /// 트리가 읽은 전체 목록으로 스레드의 이름 사본을 맞춘다(84 §6 · 인덱스와 트리가 어긋나지 않게).
    NamesUpdate {
        gen: u64,
        schema: String,
        kind: ObjectKind,
        names: Vec<String>,
    },
    Columns {
        gen: u64,
        node: usize,
        schema: String,
        table: String,
    },
    /// 하위 폴더의 잎(제약·인덱스·트리거·인자 … · 83 §1) — `nsql_catalog::sub_items`.
    SubItems {
        gen: u64,
        node: usize,
        owner: ObjectInfo,
        sub: SubKind,
    },
    /// Generate SQL(83 §3) — `nsql_catalog::generate`.
    GenSql {
        gen: u64,
        spec: GenSpec,
    },
    /// 객체 상세(86) — `nsql_catalog::object_details`(급한 세션 · 사용자가 보고 있다).
    Details {
        gen: u64,
        owner: ObjectInfo,
        /// 컬럼 상세면 그 컬럼(코멘트 질의 1).
        col: Option<ColumnInfo>,
        opts: GenOpts,
    },
    /// ★ 테이블·컬럼 코멘트 한 번에(86 §4 · 사용자 09-25 "컬럼 설명을 이미 알면 바로") — 패널이 테이블 단위로 캐시 · 값은 그대로(NULL/공백).
    Comments {
        gen: u64,
        owner: ObjectInfo,
    },
    /// ★ 스키마 전체 코멘트(86 §5 · L2 워머 · 백그라운드 세션 · 질의 2) → 메타 저장소 `set_schema_comments`.
    SchemaComments {
        gen: u64,
        schema: String,
    },
    /// 메타 저장소용 컬럼(자동 완성 즉시 채움 · docs/47 §4 · 트리 노드 없이).
    ColumnsMeta {
        gen: u64,
        schema: String,
        table: String,
        /// 팝업이 지금 기다리는 대상(참)은 큐 맨 앞 · 선적재(거짓)는 뒤(09-23).
        urgent: bool,
        /// 메타 저장 열쇠(스키마 버킷 · 이름) — 사전 객체(`ALL_TABLES` · `sys.tables`)는 DB 소유 스키마로 읽고 `$dict`에 저장(09-24).
        key: (String, String),
        /// 패키지면 컬럼 대신 멤버(`nsql_catalog::package_members` · 09-24).
        pkg: bool,
    },
    /// 자동 완성 즉시 채움 — 스키마 한 종류의 객체 목록(트리 노드 없이 · 09-23 사용자 "`스키마.` 입력 시 그 시점에 캐싱").
    ObjectsMeta {
        gen: u64,
        schema: String,
        kind: ObjectKind,
    },
    /// 권한 반영 사전 뷰(`nsql_catalog::dictionary` · 접속당 한 번).
    DictMeta {
        gen: u64,
    },
    /// 테이블 상세(제약·인덱스 — 완성 상세 카드 · 09-24 · 급한 세션).
    DetailMeta {
        gen: u64,
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
    /// 막힘 감지(docs/56 L3) — 편집기 세션(`sid`) 때문에 기다리는 세션 목록을 메타 세션으로 읽는다.
    Blockers {
        gen: u64,
        sid: String,
    },
    /// 유휴 워터마크(docs/57 T2) — 스키마마다 1행 질의로 "마지막 DDL 시각·객체 수" 지문을 읽는다.
    Watermark {
        gen: u64,
        schemas: Vec<String>,
    },
}

/// 스키마의 변경 지문 질의(방언별 1행 · 지원하지 않으면 `None`). 값 자체는 뜻이 없고 **앞선 값과 다른가**만 본다.
pub(crate) fn watermark_sql(dialect: Dialect, schema: &str) -> Option<String> {
    let s = schema.replace('\'', "''");
    Some(match dialect {
        Dialect::Oracle => format!(
            "SELECT TO_CHAR(MAX(LAST_DDL_TIME),'YYYYMMDDHH24MISS')||':'||COUNT(*) FROM ALL_OBJECTS WHERE OWNER = '{s}'"
        ),
        Dialect::Mssql => format!(
            "SELECT ISNULL(CONVERT(varchar(33), MAX(modify_date), 126), '') + ':' + CAST(COUNT(*) AS varchar(20)) FROM sys.objects WHERE schema_id = SCHEMA_ID('{s}')"
        ),
        // PostgreSQL에는 DDL 시각이 없다 → 개수 · 최대 oid · 컬럼 수 합(ADD/DROP COLUMN) + 루틴 개수·최대 oid.
        Dialect::Postgres => format!(
            "SELECT (SELECT COUNT(*)::text||':'||COALESCE(MAX(c.oid::bigint),0)::text||':'||COALESCE(SUM(c.relnatts),0)::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = '{s}') || '/' || (SELECT COUNT(*)::text||':'||COALESCE(MAX(p.oid::bigint),0)::text FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = '{s}')"
        ),
        Dialect::Mysql => format!(
            "SELECT CONCAT(COUNT(*), ':', COALESCE(MAX(CREATE_TIME), '')) FROM information_schema.tables WHERE table_schema = '{s}'"
        ),
        Dialect::Sqlite => "PRAGMA schema_version".to_string(),
        Dialect::Odbc => return None,
    })
}

fn watermark_query(
    s: &mut dyn Session,
    schemas: &[String],
) -> Vec<(String, Result<String, String>)> {
    let d = s.dialect();
    schemas
        .iter()
        .filter_map(|schema| {
            let sql = watermark_sql(d, schema)?;
            let r = s
                .execute(&nsql_core::ExecRequest {
                    sql,
                    params: vec![],
                })
                .map(|r| {
                    r.result_sets
                        .into_iter()
                        .next()
                        .and_then(|rs| {
                            rs.rows
                                .first()
                                .and_then(|row| row.first().map(|v| v.display()))
                        })
                        .unwrap_or_default()
                })
                .map_err(|e| e.message);
            Some((schema.clone(), r))
        })
        .collect()
}

/// 막힘 감지 결과 — (편집기 세션 id, 기다리는 세션 설명들 또는 오류).
pub(crate) type BlockersResult = (String, Result<Vec<String>, String>);

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

/// docs/56 L3 — "세션 `sid`가 쥔 잠금 때문에 기다리는 세션"을 한 문장으로 읽는다(방언별 카탈로그 · 권한 없으면 오류 → 호스트가 기능을 끈다).
/// `sid`는 숫자만 받는다(문장에 그대로 들어간다). 결과 한 줄 = "세션 사용자 프로그램 대기초".
fn blockers_query(s: &mut dyn Session, sid: &str) -> Result<Vec<String>, String> {
    let Some(sql) = blockers_sql(s.dialect(), sid) else {
        return Ok(Vec::new());
    };
    let rs = s
        .execute(&nsql_core::ExecRequest {
            sql,
            params: vec![],
        })
        .map(|r| r.result_sets.into_iter().next().unwrap_or_default())
        .map_err(|e| e.message)?;
    Ok(rs
        .rows
        .iter()
        .filter_map(|row| row.first())
        .map(|v| v.display().split_whitespace().collect::<Vec<_>>().join(" "))
        .collect())
}

/// 방언별 막힘 질의(지원하지 않는 방언 · 숫자가 아닌 id = `None`).
pub(crate) fn blockers_sql(dialect: Dialect, sid: &str) -> Option<String> {
    let sid = sid.trim();
    if sid.is_empty() || !sid.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(match dialect {
        Dialect::Oracle => format!(
            "SELECT sid || ' ' || NVL(username, '?') || ' ' || NVL(program, '') || ' ' || seconds_in_wait || 's' FROM v$session WHERE blocking_session = {sid}"
        ),
        Dialect::Postgres => format!(
            "SELECT pid || ' ' || COALESCE(usename, '?') || ' ' || COALESCE(application_name, '') || ' ' || COALESCE(EXTRACT(EPOCH FROM (now() - query_start))::int, 0) || 's' FROM pg_stat_activity WHERE {sid} = ANY(pg_blocking_pids(pid))"
        ),
        Dialect::Mssql => format!(
            "SELECT CAST(r.session_id AS varchar(10)) + ' ' + ISNULL(s.login_name, '?') + ' ' + ISNULL(s.program_name, '') + ' ' + CAST(r.wait_time / 1000 AS varchar(12)) + 's' FROM sys.dm_exec_requests r JOIN sys.dm_exec_sessions s ON s.session_id = r.session_id WHERE r.blocking_session_id = {sid}"
        ),
        Dialect::Mysql => format!(
            "SELECT CONCAT(waiting_pid, ' ', wait_age_secs, 's') FROM sys.innodb_lock_waits WHERE blocking_pid = {sid}"
        ),
        Dialect::Sqlite | Dialect::Odbc => return None,
    })
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
        /// 서버가 알려 준 현재 스키마(`nsql_catalog::current_schema` · 실패·빈 값 = None).
        current: Option<String>,
    },
    Objects {
        gen: u64,
        node: usize,
        r: Result<Vec<ObjectInfo>, String>,
    },
    /// 이름 인덱스 한 스키마 분(84 §2) — (항목, 잘림).
    Index {
        gen: u64,
        schema: String,
        r: Result<(Vec<nsql_catalog::NameEntry>, bool), String>,
    },
    /// 검색 일치(스레드 판정 · `rev` = 보낸 검색어 세대 · 스키마 하나 분 또는 전부).
    Hits {
        gen: u64,
        rev: u64,
        hits: Vec<(String, ObjectKind, String)>,
    },
    Columns {
        gen: u64,
        node: usize,
        r: Result<Vec<ColumnInfo>, String>,
    },
    /// 하위 폴더의 잎 목록(83 §1).
    SubItems {
        gen: u64,
        node: usize,
        r: Result<Vec<SubItem>, String>,
    },
    GenSql {
        gen: u64,
        spec: GenSpec,
        r: Result<String, String>,
    },
    Details {
        gen: u64,
        owner: ObjectInfo,
        col: Option<ColumnInfo>,
        r: Result<Vec<nsql_catalog::DetailSection>, String>,
    },
    Comments {
        gen: u64,
        owner: ObjectInfo,
        table: Option<String>,
        cols: Vec<(String, Option<String>)>,
    },
    SchemaComments {
        gen: u64,
        schema: String,
        tables: Vec<(String, Option<String>)>,
        cols: Vec<(String, String, Option<String>)>,
    },
    ColumnsMeta {
        gen: u64,
        schema: String,
        table: String,
        r: Result<Vec<ColumnInfo>, String>,
        key: (String, String),
    },
    ObjectsMeta {
        gen: u64,
        schema: String,
        kind: ObjectKind,
        r: Result<Vec<ObjectInfo>, String>,
    },
    DictMeta {
        gen: u64,
        r: Result<Vec<ObjectInfo>, String>,
    },
    DetailMeta {
        gen: u64,
        schema: String,
        table: String,
        r: Result<nsql_catalog::TableDetail, String>,
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
    Blockers {
        gen: u64,
        r: BlockersResult,
    },
    Watermark {
        gen: u64,
        r: Vec<(String, Result<String, String>)>,
    },
}

/// 호스트가 처리할 요청.
/// ★ 객체 상세 패널의 대상(docs/86 · T-223): 트리 선택에서 뽑는다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DetailTarget {
    Object(ObjectInfo),
    Column {
        owner: ObjectInfo,
        col: ColumnInfo,
    },
    Item {
        owner: ObjectInfo,
        sub: SubKind,
        item: SubItem,
    },
    Schema(String),
}

impl DetailTarget {
    /// 같은 대상 판정용 열쇠.
    pub(crate) fn key(&self) -> String {
        match self {
            DetailTarget::Object(o) => format!("o:{}.{}:{:?}", o.schema, o.name, o.kind),
            DetailTarget::Column { owner, col } => {
                format!("c:{}.{}.{}", owner.schema, owner.name, col.name)
            }
            DetailTarget::Item { owner, sub, item } => {
                format!("i:{}.{}:{:?}:{}", owner.schema, owner.name, sub, item.name)
            }
            DetailTarget::Schema(s) => format!("s:{s}"),
        }
    }
}

/// 상세 캐시 항목(86 §5 · L3).
struct DetailEntry {
    sections: Vec<DetailSection>,
    /// 마지막으로 보인 시각(미사용 회수의 시계).
    used: Instant,
    bytes: usize,
    /// 새로 고침 범위에 들어 다음 클릭에 다시 읽어야 한다(보이는 건 캐시 · 도착하면 교체).
    dirty: bool,
}

/// 상세 캐시 열쇠(`DetailTarget::key` · `o:스키마.이름:종류` · `c:스키마.테이블.컬럼` · `i:…`)가 무효화 범위에 드는가(순수).
fn detail_key_hit(key: &str, schema: Option<&str>, name: Option<&str>) -> bool {
    let body = key.get(2..).unwrap_or("");
    match (schema, name) {
        (None, _) => true,
        (Some(sc), None) => body.starts_with(&format!("{sc}.")),
        (Some(sc), Some(n)) => {
            body.starts_with(&format!("{sc}.{n}:")) || body.starts_with(&format!("{sc}.{n}."))
        }
    }
}

fn detail_bytes(secs: &[DetailSection]) -> usize {
    secs.iter()
        .map(|s| {
            s.rows
                .iter()
                .map(|r| r.iter().map(String::len).sum::<usize>() + 24)
                .sum::<usize>()
                + s.text.as_ref().map_or(0, String::len)
                + 64
        })
        .sum()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExplorerAction {
    /// 새 편집기 탭에 텍스트(SELECT 템플릿 · 소스).
    OpenSql {
        title: String,
        text: String,
    },
    /// 사용자가 알아야 하는 안내(상태줄 + 경고 토스트) — 예: 연결이 해제된 서버에서 새로 고침을 골랐다.
    Notice(String),
    /// 상태줄 한 줄.
    Status(String),
    /// 클립보드에 복사할 텍스트.
    Copy(String),
    /// 오프라인 서버를 탐색기에서 지운다(루트 우클릭 · `ExplorerSet`이 처리).
    RemoveServer,
    /// 루트 메뉴(09-19 연결 모델 · docs/54): 이 서버의 모든 세션 해제 / 다시 연결 / 이 연결로 새 탭 — 스펙은 `ExplorerSet`이 채운다.
    DisconnectServer(Option<ConnectSpec>),
    ConnectServer(Option<ConnectSpec>),
    NewTabHere(Option<ConnectSpec>),
    /// ★ 객체 상세(86 · T-223) — 상세 패널이 받는다.
    Details {
        owner: ObjectInfo,
        col: Option<ColumnInfo>,
        r: Result<Vec<nsql_catalog::DetailSection>, String>,
    },
    /// 테이블·컬럼 코멘트 그대로(86 §4) — 패널 캐시(NULL = None · 공백 = Some("")).
    Comments {
        owner: ObjectInfo,
        table: Option<String>,
        cols: Vec<(String, Option<String>)>,
    },
    /// 스키마 코멘트가 메타에 들어왔다(86 §5) — 패널이 그 스키마 대상이면 메타에서 다시 읽는다.
    CommentsLoaded {
        schema: String,
    },
    /// ★ Generate SQL 결과(83 §3) → 호스트가 SQL Preview 모달을 연다(`server` = 이 칸의 서버 · `ExplorerSet`이 채운다).
    Preview {
        spec: GenSpec,
        r: Result<String, String>,
        server: Option<ConnectSpec>,
    },
    /// ★ 표 우클릭 ▸ Import Data…(89 §3-3 · 09-26) → 호스트가 파일 창 → Import 창(`server` = 이 칸의 서버 · `ExplorerSet`이 채운다).
    Import {
        owner: ObjectInfo,
        server: Option<ConnectSpec>,
    },
}

/// 틴트 아이콘 캐시 — `(종류, rgb)` → 이미지.
/// (종류 · 색 · **표시 크기 px**) → 미리 스케일한 아이콘(09-15 사전 스케일 캐시 — 매 프레임 bilinear 샘플링 제거).
type IconCache = HashMap<(IconKind, (u8, u8, u8), i32), Rc<IconImage>>;
/// 루트 브랜드 아이콘 캐시(이름 · 색 · 크기).
type BrandCache = HashMap<(&'static str, (u8, u8, u8), i32, i32), Rc<IconImage>>;

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
    /// 선택 없음일 때의 캐럿 노드(빈 곳 클릭 · 테두리만 · 키 이동 시작점 · 사용자 09-23).
    caret: Option<usize>,
    hover: Option<usize>,
    /// 호버 행 = 1초에 걸쳐 서서히 진해짐(`IntentFade` Slow · 그리드·접속 목록과 같은 부품 · 사용자 09-15).
    hover_fade: IntentFade,
    tx: mpsc::Sender<Req>,
    /// 백그라운드 메타 스레드(미리 읽기·선적재 · 자기 세션 · 09-24).
    tx_bg: mpsc::Sender<Req>,
    rx: mpsc::Receiver<Resp>,
    /// UI 깨우기(메타 스레드를 교체할 때 다시 쓴다).
    wake: Arc<Mutex<Box<dyn Fn() + Send>>>,
    gen: u64,
    dialect: Option<Dialect>,
    conn_desc: String,
    /// 서버가 알려 준 현재 스키마(접속 뒤 · 트리 선택과 메타의 현재 스키마 판정에 먼저 쓴다).
    server_schema: Option<String>,
    /// 자동 완성이 `스키마.`로 읽어 온 버킷(트리가 펼친 것·현재 스키마·사전은 제외) — 열린 문서가 그 이름을 더 이상 쓰지 않으면
    /// 즉시 해제한다(`reclaim_intel_buckets` · 사용자 09-23 "미사용 판정 즉시 회수").
    intel_buckets: Vec<(String, ObjectKind)>,
    /// ★ 메타에 없는 객체의 컬럼 요청(이번 세션에 만든 테이블 = L1/디스크 캐시에 없음 · 사용자 09-26 `DEMO_BSY A.` 완성 안 됨):
    ///   그 스키마 관계 목록을 다시 읽은 뒤 다시 요청 · 같은 이름은 한 번만(없는 이름을 키마다 되묻지 않게).
    pending_cols: Vec<(String, String, bool)>,
    missing_cols: HashSet<String>,
    /// `intel.from_routines` — 자동 완성 채움에 함수·패키지·프로시저 버킷도(09-24).
    routines: bool,
    /// 접속 직후 현재 스키마·사전 미리 읽기(`intel.preload`).
    preload: bool,
    /// 루트 표시 = 프로필 이름(굵게) + 호스트:포트(흐리게 · docs/28 §1 · 사용자 09-15).
    profile_name: String,
    endpoint: String,
    /// ★ 묶음 모드(docs/54 §9 · 09-25): 같은 서버의 연결들이 한 헤더 아래 놓인다 — 루트 행 = **연결(DB · 계정)** 행 · 들여쓰기 +1.
    grouped: bool,
    /// 연결의 DB(서비스)·계정(묶음 모드 루트 행 라벨).
    conn_db: String,
    conn_user: String,
    /// ★ 탐색기 검색창(docs/28 §7 · 09-25): 걸러진 노드 집합(None = 필터 없음) · 직접 일치 수 · 강조용 질의 낱말(소문자).
    filter_keep: Option<std::collections::HashSet<usize>>,
    filter_hits: usize,
    filter_tokens: Vec<String>,
    /// 보관한 판정기 — 자식이 생기거나 빠질 때마다 `refilter`(⑩ 09-25: 동기 생성 자식이 keep 집합에 없어 숨던 결함).
    filter: Option<crate::filterbar::Matcher>,
    /// 필터가 **대신 펼친** 노드(사용자가 펼친 적 없음) — 필터를 지우면 다시 접는다(사용자 09-25 "UI가 복잡해진다").
    filter_expanded: std::collections::HashSet<usize>,
    /// ★ L1 이름 층 빌더(84 §2 · 85 §2): 스키마 단위로 순차(현재 스키마 먼저 · 한 번에 하나) — 항목은 `MetaStore`(`Coverage::Names`)에 산다(단일 원천) ·
    /// 검색 판정용 사본은 백그라운드 메타 스레드가 든다(85 §6) · `index_done`의 스키마는 다 들어갔다.
    index_done: HashSet<String>,
    index_q: VecDeque<String>,
    index_inflight: Option<String>,
    index_truncated: bool,
    index_cfg: IndexCfg,
    /// 마지막 인덱스 응답 시각(L1 간격 기준).
    index_last_at: Option<Instant>,
    /// ★ L2 워머(85 §3): 현재 스키마 관계의 컬럼을 뒤에서 하나씩(간격 · 상한) — 첫 완성·카드가 즉시 나오게.
    warm_q: VecDeque<nsql_run::meta::ObjId>,
    warm_inflight: Option<nsql_run::meta::ObjId>,
    warm_last: Option<Instant>,
    warm_sent: usize,
    /// ★ L2 코멘트 워머(86 §5): 관계 폴더가 읽힌 스키마를 하나씩 · 진행 중 하나.
    comment_q: VecDeque<String>,
    comment_inflight: Option<String>,
    /// L1 디스크 캐시 파일(접속 자격 열쇠의 해시 · 85 §9) · 이번 접속에 저장했는가.
    cache_path: Option<std::path::PathBuf>,
    cache_saved: bool,
    /// ★ 부분 폴더 완성 큐(84 §4) — 인덱스가 올린 폴더의 전체 목록을 보이는 순서로 하나씩.
    complete_q: VecDeque<usize>,
    completing: Option<usize>,
    /// 한 묶음으로 노드를 여럿 만드는 동안 `refilter`를 미룬다(끝에 한 번).
    batching: bool,
    /// 검색어 세대(스레드 판정 응답 `Resp::Hits`의 짝 맞추기).
    filter_rev: u64,
    /// ★ 노드별 소문자 라벨 캐시(refilter용 · 09-25 92 ms → ms): 노드 인덱스 → 소문자 라벨 · 종류가 바뀌면 비운다.
    lower_labels: Vec<Option<Box<str>>>,
    /// Generate SQL 옵션(설정 `gen.*` · 호스트가 준다).
    gen_opts: GenOpts,
    /// 스키마 목록 옵션(설정 `explorer.show_system_schemas`/`hide_empty_schemas`).
    schema_opts: SchemaOpts,
    /// ★ 카탈로그 공유(docs/54 §10): 이 칸에 붙은 연결들의 계정(루트 행 라벨) — `ExplorerSet`이 준다.
    users: Vec<String>,
    /// 자격만 바꾸는 재접속 중(`rebind`) — 열림 응답에서 트리를 건드리지 않는다.
    rebinding: bool,
    menu: CtxMenu,
    actions: Vec<ExplorerAction>,
    last_click: Option<(usize, Instant)>,
    visible: bool,
    focused: bool,
    /// 오브젝트 아이콘(설정 `explorer.icons`) · 틴트 이미지 캐시 `(종류, rgb)`.
    icons_on: bool,
    icon_cache: IconCache,
    /// ★ 객체 상세 캐시(사용자 09-26 "이미 본 대상은 깜빡임 없이 · 상한/미사용 회수 · 새로 고침 범위는 무효화") —
    ///   열쇠 = `DetailTarget::key` · 값 = 섹션 · `dirty` = 새로 고침(수동·DDL·워터마크)이 닿아 다음 클릭에 다시 읽는다(보이는 건 즉시 · 도착하면 교체).
    detail_cache: HashMap<String, DetailEntry>,
    /// 새로 고침 범위 전파 기록(스키마, 객체) — 호스트가 가져가 상세 패널의 코멘트 캐시도 버린다(T-227 후속 · 09-26).
    detail_invalidated: Vec<(Option<String>, Option<String>)>,
    /// 루트 브랜드 아이콘 캐시(이름 · 색 · 크기 → 그림 · dbms_icons · 사용자 09-22).
    brand_cache: BrandCache,
    /// 마지막 페인트의 화면 행(노드 index · 부모) — `row_at`(MouseMove마다)이 다시 펼치지 않게(09-15 C).
    rows_cache: Vec<(Option<usize>, usize)>,
    /// ★ 메타 저장소(docs/47 · docs/76): 트리에 도착한 스키마·객체·컬럼을 **함께** 담는다 — 자동 완성이 스냅샷만 읽는다.
    meta: nsql_run::meta::MetaStore,
    /// 호스트가 알려 주는 현재 트리 글꼴 크기(논리 px) — 아이콘 스케일 기준.
    font_px: f32,
    /// 소스 요청 중(더블클릭 연타 방지).
    source_pending: bool,
    /// 마지막 페인트의 행 높이(글꼴 높이 + 여백 · 글꼴 크기를 따라간다 · 사용자 09-15).
    row_px: i32,
    /// 로딩 점 애니메이션(300ms 단계) — 로딩 중인 노드가 있을 때만 다시 그린다(nexa-dir2 "Loading…" 자리 · 사용자 09-15).
    dots_step: u64,
    /// 타입어헤드(nexa-ctl 부품 · nexa-beep 이식 · 사용자 09-19): 버퍼+한글 조합+타임아웃. 매칭은 이 파일(`label`).
    typeahead: nexa_ctl::TypeAhead,
    ta_cfg: TypeAheadCfg,
    /// 마지막 틱 시각(ms) — 키 사건에는 시각이 없어 틱이 준 값을 쓴다.
    now_hint: u64,
    /// 라이브 로그 응답(호스트가 가져간다) · 요청 진행 중 표시.
    live_results: Vec<LiveResult>,
    pub(crate) live_inflight: bool,
    /// 막힘 감지 응답(호스트가 가져간다) · 요청 진행 중 표시(docs/56 L3).
    blockers_results: Vec<BlockersResult>,
    blockers_inflight: bool,
    /// 이 서버에 붙은 세션이 하나도 없어 메타 접속을 닫았다 — **트리(읽어 둔 메타)는 남긴다**(docs/52 §2-2 · 다시 붙으면 새로 읽는다).
    offline: bool,
    /// 메타 세션을 유휴로 닫아 두었다(다음 요청 때 메타 스레드가 다시 연다).
    suspended: bool,
    /// 일회성 비밀번호로 붙은 서버 — 메타 세션을 유휴로 닫으면 다시 열 자격이 없으므로 **닫지 않는다**.
    one_time: bool,
    /// 마지막으로 메타 요청을 보낸 시각(유휴 회수 판정).
    last_used: Instant,
    /// 우클릭 메뉴가 놓일 수 있는 영역 — 여러 서버가 세로로 쌓이면 한 칸(`bounds`)이 한 줄 높이일 수 있어 탐색기 전체 영역을 받는다.
    menu_host: Rect,
    /// 보이는 창(호스트가 준다) — 여러 서버의 트리를 **한 트리처럼 이어 붙여** 하나의 스크롤로 움직일 때, 이 칸의 `bounds`는
    /// 내용 전체 높이라 영역 밖으로 나갈 수 있다 → 그리기·히트 테스트는 이 창과의 교집합만(`None` = bounds 그대로).
    clip: Option<Rect>,
    /// 가로 스크롤(호스트 공용 · 09-19 사용자) — 그리기·히트 테스트의 x에서 뺀다.
    scroll_x: i32,
    /// 마지막 그리기에서 잰 내용 폭(가장 긴 행의 오른쪽 끝 + 여백 · bounds.x 기준).
    content_w: i32,
    /// **조용한 갱신**(docs/57 §2-2) 요청 중인 노드 — 응답이 오면 자식을 통째로 갈지 않고 디프로 반영한다.
    soft: HashSet<usize>,
    /// 방금 생긴 노드(디프로 추가됨) → 강조가 끝나는 시각(ms · `now_hint` 기준).
    fresh: Vec<(usize, u64)>,
    /// 새 객체 강조 시간(설정 `meta.refresh_highlight_ms` · 0 = 없음).
    highlight_ms: u64,
    /// "못 찾음" 신호로 마지막에 갱신한 시각(폴더별 60초에 1회 · docs/57 T4).
    missing_at: HashMap<usize, Instant>,
    /// 스키마별 변경 지문(docs/57 T2) · 질의 진행 중 표시.
    watermarks: HashMap<String, String>,
    wm_inflight: bool,
}

/// "객체 없음" 오류문에서 객체 이름을 뽑는다(docs/57 T4) — PostgreSQL `relation "x" does not exist` · SQL Server
/// `Invalid object name 'dbo.x'` · SQLite `no such table: x` · Oracle 23 `table or view "S"."X" does not exist`.
/// 이름이 없는 오류문(옛 Oracle ORA-00942)은 `None` — 호출자가 현재 스키마의 테이블·뷰 폴더로 넓힌다.
pub(crate) fn missing_name(msg: &str) -> Option<String> {
    if let Some(i) = msg.find("no such table:") {
        let rest = msg[i + "no such table:".len()..].trim();
        let name: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ',' && *c != ';')
            .collect();
        return (!name.is_empty()).then_some(name);
    }
    // 마지막 따옴표 구간(`"S"."X"` = X · `'dbo.x'` = dbo.x)을 이름으로.
    for q in ['"', '\''] {
        let parts: Vec<&str> = msg.split(q).collect();
        if parts.len() >= 3 {
            let name = parts[parts.len() - 2].trim();
            if !name.is_empty() && !name.contains(' ') {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// `nsql_core::DdlKind` → 탐색기 폴더 종류(스키마는 폴더가 아니라 루트 목록).
fn folder_kind(k: nsql_core::DdlKind) -> Option<ObjectKind> {
    use nsql_core::DdlKind as D;
    Some(match k {
        D::Table => ObjectKind::Table,
        D::View => ObjectKind::View,
        D::MaterializedView => ObjectKind::MaterializedView,
        D::Index => ObjectKind::Index,
        D::Sequence => ObjectKind::Sequence,
        D::Procedure => ObjectKind::Procedure,
        D::Function => ObjectKind::Function,
        D::Package => ObjectKind::Package,
        D::PackageBody => ObjectKind::PackageBody,
        D::Trigger => ObjectKind::Trigger,
        D::Synonym => ObjectKind::Synonym,
        D::Type => ObjectKind::Type,
        D::Schema => return None,
    })
}

/// 디프용 노드 열쇠 — 같은 부모 아래에서 "같은 것"을 알아보는 값(이름 + 종류 · 대소문자 구분).
fn node_key(k: &NodeKind) -> String {
    match k {
        NodeKind::Root => "root".into(),
        NodeKind::Schema(s) => format!("s:{s}"),
        NodeKind::Folder { schema, kind } => format!("f:{schema}:{kind:?}"),
        NodeKind::Object(o) => format!("o:{:?}:{}:{}", o.kind, o.name, o.extra),
        NodeKind::Column(c) => format!("c:{}", c.name),
        NodeKind::Sub { owner, sub } => format!("sub:{}:{sub:?}", owner.name),
        NodeKind::Item(it) => format!("i:{:?}:{}", it.icon, it.name),
    }
}

fn err_s(e: DbError) -> String {
    e.message
}

/// 메타 스레드 — 세션 하나 · 순차 처리 · 요청마다 `catch_unwind`(드라이버 패닉이 UI로 번지지 않게).
/// 메타 세션을 연다 — 비밀번호 자리가 없는 스펙이면 **세션 자격 금고**에서 빌려 그 접속에만 쓰고 지운다. 금고에도 없으면
/// 서버에 가지 않는다(빈 비밀번호 로그인 시도가 쌓이면 계정이 잠긴다 · docs/26 §8).
fn open_meta(spec: &ConnectSpec, default: Dialect) -> Result<Box<dyn Session>, DbError> {
    if !crate::worker::password_required(spec, default) {
        return nsql_drivers::open(spec, default);
    }
    let lent = crate::worker::remember_session_password()
        .then(|| nsql_vault::session::recall(&crate::worker::cred_id(spec, default)))
        .flatten();
    let Some(secret) = lent else {
        return Err(DbError {
            code: None,
            message: t(Msg::ErrPasswordRequired).into(),
            position: None,
        });
    };
    let mut once = spec.clone();
    once.password = Some(secret.expose().to_string());
    drop(secret);
    let r = nsql_drivers::open(&once, default);
    nsql_core::secret::wipe_opt(&mut once.password);
    // 서버가 거부한 값은 폐기한다(메타 스레드는 묻지 않는다 — 다음 접속 때 워커가 다시 묻는다).
    if let Err(e) = &r {
        if crate::worker::stale_password(spec.dialect.unwrap_or(default), e) {
            nsql_vault::session::forget(&crate::worker::cred_id(spec, default));
        }
    }
    r
}

/// 계측 라벨(요청 종류 · 대상).
fn req_label(r: &Req) -> String {
    match r {
        Req::Open { .. } => "open".into(),
        Req::Prepare { .. } => "prepare".into(),
        Req::Close => "close".into(),
        Req::Suspend => "suspend".into(),
        Req::Schemas { .. } => "schemas".into(),
        Req::Objects { schema, kind, .. } => format!("objects {schema} {kind:?}"),
        Req::Index { schema, .. } => format!("index {schema}"),
        Req::Search { .. } => "search".into(),
        Req::SearchStop { .. } => "search-stop".into(),
        Req::NamesUpdate { schema, kind, .. } => format!("names-update {schema} {kind:?}"),
        Req::NamesSeed { schema, .. } => format!("names-seed {schema}"),
        Req::SubItems { owner, sub, .. } => format!("sub {} {sub:?}", owner.name),
        Req::GenSql { spec, .. } => format!("gen {}", spec.title()),
        Req::Columns { schema, table, .. } => format!("columns {schema}.{table}"),
        Req::ColumnsMeta {
            schema,
            table,
            urgent,
            ..
        } => format!(
            "columns-meta {schema}.{table}{}",
            if *urgent { "" } else { " (bg)" }
        ),
        Req::ObjectsMeta { schema, kind, .. } => format!("objects-meta {schema} {kind:?}"),
        Req::DictMeta { .. } => "dictionary".into(),
        Req::DetailMeta { schema, table, .. } => format!("detail {schema}.{table}"),
        Req::Source { name, .. } => format!("source {name}"),
        _ => "other".into(),
    }
}

/// 메타 워커 큐의 우선순위(낮을수록 먼저 · 09-23).
fn req_prio(r: &Req) -> u8 {
    match r {
        Req::Open { .. } | Req::Prepare { .. } | Req::Close | Req::Suspend => 0,
        Req::Schemas { .. }
        | Req::Objects { .. }
        | Req::Columns { .. }
        | Req::SubItems { .. }
        | Req::GenSql { .. }
        | Req::Details { .. }
        | Req::Comments { .. }
        | Req::Source { .. } => 1,
        Req::ColumnsMeta { urgent: true, .. } | Req::DetailMeta { .. } => 1,
        // 검색 판정·중지 = 즉시(질의 없음 · ms 단위) · 인덱스 = 사용자 클릭(1) 다음 · 백그라운드 메타(3~5) 앞.
        Req::Search { .. }
        | Req::SearchStop { .. }
        | Req::NamesUpdate { .. }
        | Req::NamesSeed { .. } => 1,
        Req::Index { .. } => 2,
        Req::ColumnsMeta { urgent: false, .. } => 3,
        Req::ObjectsMeta { .. } | Req::SchemaComments { .. } => 4,
        Req::DictMeta { .. } => 5,
        _ => 2,
    }
}

fn meta_thread(rx: mpsc::Receiver<Req>, tx: mpsc::Sender<Resp>, wake: Box<dyn Fn() + Send>) {
    let mut session: Option<Box<dyn Session>> = None;
    let mut cur_gen = 0u64;
    // 유휴로 닫힌 뒤 다시 열 스펙(`Suspend`는 남기고 `Close`는 지운다).
    let mut resume: Option<ConnectSpec> = None;
    // ★ 우선순위 큐(사용자 09-23 "컬럼 로딩이 너무 느리다 · alias 대상부터"): 한 세션이 한 번에 한 질의를 하므로, 쌓인 요청
    //   가운데 **급한 것부터**(접속/닫기 → 트리·팝업이 기다리는 컬럼 → 폴링 → 선적재 컬럼 → 스키마 미리 읽기 → 사전) 꺼낸다.
    //   같은 급은 도착 순. 실행 중인 질의는 끊지 않는다(그래서 느린 질의는 잘게 — `nsql_catalog::dictionary`).
    let mut pending: std::collections::VecDeque<Req> = std::collections::VecDeque::new();
    // ★ 검색 스레드 몫(85 §6): 이 스레드가 읽은 L1 이름 사본(스키마별) + 살아 있는 검색어 — 판정은 여기서, UI는 일치만 받는다.
    let mut l1_names: HashMap<String, Vec<nsql_catalog::NameEntry>> = HashMap::new();
    let mut search: Option<(u64, crate::filterbar::Matcher, usize)> = None;
    loop {
        if pending.is_empty() {
            match rx.recv() {
                Ok(r) => pending.push_back(r),
                Err(_) => break,
            }
        }
        while let Ok(r) = rx.try_recv() {
            pending.push_back(r);
        }
        let pick = (0..pending.len())
            .min_by_key(|&i| req_prio(&pending[i]))
            .unwrap_or(0);
        let Some(req) = pending.remove(pick) else {
            break;
        };
        // 유휴로 닫혀 있었으면 카탈로그 요청 앞에서 다시 연다(사용자 동작 1회당 1접속 · 26 §8).
        if session.is_none()
            && !matches!(
                req,
                Req::Open { .. } | Req::Prepare { .. } | Req::Close | Req::Suspend
            )
        {
            if let Some(spec) = resume.as_ref() {
                let default = spec.dialect.unwrap_or(Dialect::Oracle);
                // 재개 전 빠른 판정(docs/53): 끊긴 서버에 메타 스레드가 접속 타임아웃까지 갇히지 않게.
                if reachable(spec) {
                    if let Ok(Ok(s)) = catch_unwind(AssertUnwindSafe(|| open_meta(spec, default))) {
                        session = Some(s);
                    }
                }
            }
        }
        // 계측(09-24 "컬럼 캐싱 속도"): 300 ms를 넘는 메타 질의는 stderr에 종류와 시간을 남긴다(개발자 모드 캡처용).
        let t0 = Instant::now();
        let what = req_label(&req);
        let resp = match req {
            Req::Open {
                gen,
                mut spec,
                once,
            } => {
                session = None;
                cur_gen = gen;
                // 일회성 비밀번호는 재개용 스펙에 넣지 않는다(이 칸은 유휴 회수도 하지 않는다 — `Explorer::one_time`).
                resume = Some(if once {
                    let mut keep = spec.clone();
                    nsql_core::secret::wipe_opt(&mut keep.password);
                    keep
                } else {
                    spec.clone()
                });
                let default = spec.dialect.unwrap_or(Dialect::Oracle);
                // (비밀번호 자리가 없는 스펙 = `open_meta`가 금고에서 빌리거나, 없으면 서버에 가지 않고 거절한다.)
                let r = if reachable(&spec) {
                    catch_unwind(AssertUnwindSafe(|| open_meta(&spec, default)))
                } else {
                    Ok(Err(DbError {
                        code: None,
                        message: tf(
                            Msg::ErrServerUnreachable,
                            &[
                                &format!(
                                    "{}:{}",
                                    spec.host.clone().unwrap_or_default(),
                                    spec.port.unwrap_or(0)
                                ),
                                "2000",
                            ],
                        ),
                        position: None,
                    }))
                };
                if once {
                    nsql_core::secret::wipe_opt(&mut spec.password);
                }
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
            Req::Prepare { gen, spec } => {
                session = None;
                cur_gen = gen;
                resume = Some(spec);
                l1_names.clear();
                search = None;
                continue;
            }
            Req::Search {
                gen,
                rev,
                matcher,
                limit,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let hits = search_hits(&l1_names, &matcher, limit);
                search = Some((rev, matcher, limit));
                let _ = tx.send(Resp::Hits { gen, rev, hits });
                wake();
                continue;
            }
            Req::SearchStop { gen } => {
                if gen == cur_gen {
                    search = None;
                }
                continue;
            }
            Req::NamesSeed { gen, schema, list } => {
                if gen != cur_gen || l1_names.contains_key(&schema) {
                    continue;
                }
                if let Some((rev, m, limit)) = &search {
                    let one: HashMap<String, Vec<nsql_catalog::NameEntry>> =
                        HashMap::from([(schema.clone(), list.clone())]);
                    let hits = search_hits(&one, m, *limit);
                    if !hits.is_empty() {
                        let _ = tx.send(Resp::Hits {
                            gen,
                            rev: *rev,
                            hits,
                        });
                        wake();
                    }
                }
                l1_names.insert(schema, list);
                continue;
            }
            Req::NamesUpdate {
                gen,
                schema,
                kind,
                names,
            } => {
                if gen != cur_gen {
                    continue;
                }
                if let Some(v) = l1_names.get_mut(&schema) {
                    v.retain(|e| e.kind != kind);
                    v.extend(names.into_iter().map(|name| nsql_catalog::NameEntry {
                        schema: schema.clone(),
                        kind,
                        name,
                    }));
                }
                continue;
            }
            Req::Close => {
                resume = None;
                if let Some(mut s) = session.take() {
                    let _ = s.rollback();
                }
                continue;
            }
            Req::Suspend => {
                if let Some(mut s) = session.take() {
                    let _ = s.rollback();
                }
                continue;
            }
            Req::Schemas { gen, node, opts } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::schemas_opt(s, opts).map_err(err_s)
                });
                // 현재 스키마는 서버가 안다(SQL Server `SCHEMA_NAME()` = dbo · PG `current_schema()` · Oracle CURRENT_SCHEMA) —
                //   계정 이름과 같은 스키마를 찾던 종전 규칙은 SQL Server·PG에서 비어 완성에 테이블이 0이었다(사용자 09-23).
                let current = with_session(&mut session, |s| {
                    nsql_catalog::current_schema(s).map_err(err_s)
                })
                .ok()
                .filter(|c| !c.is_empty());
                Resp::Schemas {
                    gen,
                    node,
                    r,
                    current,
                }
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
            Req::Index { gen, schema, max } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::name_index(s, std::slice::from_ref(&schema), max).map_err(err_s)
                });
                // 사본 갱신 + 살아 있는 검색어가 있으면 이 스키마 분 일치를 바로(UI는 스캔 없이 삽입만).
                if let Ok((list, _)) = &r {
                    l1_names.insert(schema.clone(), list.clone());
                    if let Some((rev, m, limit)) = &search {
                        let one: HashMap<String, Vec<nsql_catalog::NameEntry>> =
                            HashMap::from([(schema.clone(), list.clone())]);
                        let hits = search_hits(&one, m, *limit);
                        if !hits.is_empty() {
                            let _ = tx.send(Resp::Hits {
                                gen,
                                rev: *rev,
                                hits,
                            });
                        }
                    }
                }
                Resp::Index { gen, schema, r }
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
            Req::SubItems {
                gen,
                node,
                owner,
                sub,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::sub_items(s, &owner, sub).map_err(err_s)
                });
                Resp::SubItems { gen, node, r }
            }
            Req::GenSql { gen, spec } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::generate(s, &spec).map_err(err_s)
                });
                Resp::GenSql { gen, spec, r }
            }
            Req::Details {
                gen,
                owner,
                col,
                opts,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| match &col {
                    Some(c) => nsql_catalog::column_details(s, &owner, c).map_err(err_s),
                    None => nsql_catalog::object_details(s, &owner, opts).map_err(err_s),
                });
                Resp::Details { gen, owner, col, r }
            }
            Req::SchemaComments { gen, schema } => {
                if gen != cur_gen {
                    continue;
                }
                let (tables, cols) = with_session(&mut session, |s| {
                    Ok(nsql_catalog::schema_comments_raw(s, &schema))
                })
                .unwrap_or_default();
                Resp::SchemaComments {
                    gen,
                    schema,
                    tables,
                    cols,
                }
            }
            Req::Comments { gen, owner } => {
                if gen != cur_gen {
                    continue;
                }
                let (table, cols) = with_session(&mut session, |s| {
                    Ok(nsql_catalog::comments_raw(s, &owner.schema, &owner.name))
                })
                .unwrap_or((None, Vec::new()));
                Resp::Comments {
                    gen,
                    owner,
                    table,
                    cols,
                }
            }
            Req::ColumnsMeta {
                gen,
                schema,
                table,
                urgent: _,
                key,
                pkg,
            } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    if pkg {
                        nsql_catalog::package_members(s, &schema, &table).map_err(err_s)
                    } else {
                        nsql_catalog::columns(s, &schema, &table).map_err(err_s)
                    }
                });
                Resp::ColumnsMeta {
                    gen,
                    schema,
                    table,
                    r,
                    key,
                }
            }
            Req::ObjectsMeta { gen, schema, kind } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::objects(s, &schema, kind).map_err(err_s)
                });
                Resp::ObjectsMeta {
                    gen,
                    schema,
                    kind,
                    r,
                }
            }
            Req::DictMeta { gen } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| nsql_catalog::dictionary(s).map_err(err_s));
                Resp::DictMeta { gen, r }
            }
            Req::DetailMeta { gen, schema, table } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| {
                    nsql_catalog::table_detail(s, &schema, &table).map_err(err_s)
                });
                Resp::DetailMeta {
                    gen,
                    schema,
                    table,
                    r,
                }
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
            Req::Blockers { gen, sid } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| blockers_query(s, &sid));
                Resp::Blockers { gen, r: (sid, r) }
            }
            Req::Watermark { gen, schemas } => {
                if gen != cur_gen {
                    continue;
                }
                let r = with_session(&mut session, |s| Ok(watermark_query(s, &schemas)))
                    .unwrap_or_default();
                Resp::Watermark { gen, r }
            }
        };
        let ms = t0.elapsed().as_millis();
        // 인덱스·검색은 늘 남긴다(스키마당 1줄 · 09-25 "검색이 끝나지 않는다" 진단).
        if ms >= 300
            || what.starts_with("columns")
            || what.starts_with("detail")
            || what.starts_with("index")
        {
            eprintln!("[meta] {what} took {ms} ms");
        }
        if tx.send(resp).is_err() {
            break;
        }
        wake();
    }
}

/// 접속 전 빠른 판정(호스트:포트 TCP · 2초 · 파일 방언은 해당 없음).
fn reachable(spec: &ConnectSpec) -> bool {
    match (&spec.host, spec.port) {
        (Some(h), Some(p)) => {
            crate::probe::probe_once(h, p, Duration::from_secs(2), true)
                == crate::probe::Outcome::Up
        }
        _ => true,
    }
}

/// ★ 검색 판정(스레드 · 85 §6): 스키마 사본 전부에서 이름이 매처에 맞는 것(상한 `limit`) — 순수 함수(시험).
fn search_hits(
    names: &HashMap<String, Vec<nsql_catalog::NameEntry>>,
    m: &crate::filterbar::Matcher,
    limit: usize,
) -> Vec<(String, ObjectKind, String)> {
    let mut out = Vec::new();
    let mut schemas: Vec<&String> = names.keys().collect();
    schemas.sort();
    'outer: for sc in schemas {
        for e in &names[sc] {
            if m.matches(&e.name) {
                out.push((e.schema.clone(), e.kind, e.name.clone()));
                if out.len() >= limit {
                    break 'outer;
                }
            }
        }
    }
    out
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

/// ★ 현재 스키마 판정(순수 · 09-23): 서버가 알려 준 값(목록에 있을 때) → 접속 계정과 같은 이름 → 방언 기본(`public` · `dbo` ·
/// `main`) → 스키마가 하나뿐이면 그것. 대소문자 무시 · 반환은 목록의 표기.
fn pick_current_schema(list: &[String], server: Option<&str>, user: &str) -> Option<String> {
    let find = |name: &str| {
        (!name.is_empty())
            .then(|| list.iter().find(|s| s.eq_ignore_ascii_case(name)).cloned())
            .flatten()
    };
    server
        .and_then(find)
        .or_else(|| find(user))
        .or_else(|| find("public"))
        .or_else(|| find("dbo"))
        .or_else(|| find("main"))
        .or_else(|| (list.len() == 1).then(|| list[0].clone()))
}

/// 종류 폴더 라벨(i18n · 방언별 이름 = DBeaver: Oracle·SQL Server "Table Triggers" · SQL Server·PG "Data Types" · 83 §1).
fn folder_label(dialect: Option<Dialect>, kind: ObjectKind) -> String {
    let msg = match (dialect, kind) {
        (Some(Dialect::Oracle | Dialect::Mssql), ObjectKind::Trigger) => Msg::ExpTableTriggers,
        (Some(Dialect::Mssql | Dialect::Postgres), ObjectKind::Type) => Msg::ExpDataTypes,
        _ => folder_msg(kind),
    };
    t(msg).to_string()
}

/// 하위 폴더 라벨(i18n · DBMS 용어라 영어 그대로).
pub(crate) fn sub_msg(sub: SubKind) -> Msg {
    match sub {
        SubKind::Columns => Msg::SubColumns,
        SubKind::Constraints => Msg::SubConstraints,
        SubKind::UniqueKeys => Msg::SubUniqueKeys,
        SubKind::CheckConstraints => Msg::SubCheckConstraints,
        SubKind::ForeignKeys => Msg::SubForeignKeys,
        SubKind::References => Msg::SubReferences,
        SubKind::Indexes => Msg::SubIndexes,
        SubKind::Triggers => Msg::SubTriggers,
        SubKind::Partitions => Msg::SubPartitions,
        SubKind::Dependencies => Msg::SubDependencies,
        SubKind::Rules => Msg::SubRules,
        SubKind::Policies => Msg::SubPolicies,
        SubKind::ExtendedProperties => Msg::SubExtendedProperties,
        SubKind::Arguments => Msg::SubArguments,
        SubKind::Attributes => Msg::SubAttributes,
        SubKind::Methods => Msg::SubMethods,
        SubKind::Procedures => Msg::SubProcedures,
        SubKind::Functions => Msg::SubFunctions,
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
        ObjectKind::ExternalTable => Msg::ExpExternalTables,
        ObjectKind::ForeignTable => Msg::ExpForeignTables,
        ObjectKind::Aggregate => Msg::ExpAggregates,
        ObjectKind::Queue => Msg::ExpQueues,
        ObjectKind::DbLink => Msg::ExpDbLinks,
        ObjectKind::JavaClass => Msg::ExpJava,
        ObjectKind::Job => Msg::ExpJobs,
        ObjectKind::SchedulerJob => Msg::ExpSchedulerJobs,
        ObjectKind::SchedulerProgram => Msg::ExpSchedulerPrograms,
        ObjectKind::SchemaTrigger => Msg::ExpSchemaTriggers,
        ObjectKind::Extension => Msg::ExpExtensions,
        ObjectKind::EventTrigger => Msg::ExpEventTriggers,
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
        ObjectKind::Trigger | ObjectKind::SchemaTrigger | ObjectKind::EventTrigger => th.warn,
        ObjectKind::ExternalTable | ObjectKind::ForeignTable => th.accent,
        ObjectKind::Aggregate => th.syn_keyword,
        ObjectKind::Sequence
        | ObjectKind::Index
        | ObjectKind::Synonym
        | ObjectKind::Type
        | ObjectKind::Queue
        | ObjectKind::DbLink
        | ObjectKind::JavaClass
        | ObjectKind::Job
        | ObjectKind::SchedulerJob
        | ObjectKind::SchedulerProgram
        | ObjectKind::Extension => th.syn_number,
    }
}

/// 잎 항목 아이콘(카탈로그 힌트 → 탐색기 아이콘 종류).
fn sub_icon_kind(icon: SubIcon) -> IconKind {
    match icon {
        SubIcon::Column | SubIcon::Argument | SubIcon::Attribute => IconKind::Column,
        SubIcon::Key | SubIcon::Check | SubIcon::ForeignKey => IconKind::Constraint,
        SubIcon::Reference | SubIcon::Dependency => IconKind::Synonym,
        SubIcon::Index => IconKind::Index,
        SubIcon::Trigger => IconKind::Trigger,
        SubIcon::Partition => IconKind::Partition,
        SubIcon::Rule | SubIcon::Policy => IconKind::Rule,
        SubIcon::Property => IconKind::Property,
        SubIcon::Method | SubIcon::Procedure => IconKind::Procedure,
        SubIcon::Function => IconKind::Function,
    }
}

impl Explorer {
    /// 메타 스레드 **둘** 시작(급한 것 · 백그라운드 — 요청 채널 둘 · 응답 채널 하나).
    /// ★ 09-24(사용자 "테이블 1개 컬럼 캐싱이 느리다"): 우선순위 큐로도 **실행 중인** 긴 질의(스키마 미리 읽기 `ALL_OBJECTS` · 사전
    ///   `ALL_VIEWS`)는 끊지 못해 그 뒤에 섰다 → 미리 읽기·선적재는 **자기 세션**(`nsql-explorer-bg` · 첫 요청 때 연다)에서 돌리고
    ///   트리·팝업이 기다리는 요청은 급한 세션이 즉시 처리한다. 연결 +1은 백그라운드 요청이 실제로 있을 때만(26 §8).
    fn spawn_meta(
        wake: &Arc<Mutex<Box<dyn Fn() + Send>>>,
    ) -> (mpsc::Sender<Req>, mpsc::Sender<Req>, mpsc::Receiver<Resp>) {
        let (tx, req_rx) = mpsc::channel::<Req>();
        let (tx_bg, bg_rx) = mpsc::channel::<Req>();
        let (resp_tx, rx) = mpsc::channel::<Resp>();
        let resp_bg = resp_tx.clone();
        let mk_wake = |w: Arc<Mutex<Box<dyn Fn() + Send>>>| -> Box<dyn Fn() + Send> {
            Box::new(move || {
                if let Ok(f) = w.lock() {
                    f();
                }
            })
        };
        let wake_fn = mk_wake(Arc::clone(wake));
        let _ = std::thread::Builder::new()
            .name("nsql-explorer".into())
            .spawn(move || meta_thread(req_rx, resp_tx, wake_fn));
        let wake_bg = mk_wake(Arc::clone(wake));
        let _ = std::thread::Builder::new()
            .name("nsql-explorer-bg".into())
            .spawn(move || meta_thread(bg_rx, resp_bg, wake_bg));
        (tx, tx_bg, rx)
    }

    pub(crate) fn new(wake: Box<dyn Fn() + Send>, visible: bool) -> Self {
        let wake = Arc::new(Mutex::new(wake));
        let (tx, tx_bg, rx) = Self::spawn_meta(&wake);
        let mut e = Explorer {
            nodes: Vec::new(),
            bounds: Rect::default(),
            scale: 1.0,
            scroll: 0,
            bars: ScrollBars::new(),
            selected: None,
            caret: None,
            hover: None,
            hover_fade: IntentFade::with_speed(FadeSpeed::Slow),
            tx,
            tx_bg,
            rx,
            wake,
            gen: 0,
            dialect: None,
            conn_desc: String::new(),
            server_schema: None,
            intel_buckets: Vec::new(),
            pending_cols: Vec::new(),
            missing_cols: HashSet::new(),
            routines: true,
            preload: true,
            profile_name: String::new(),
            endpoint: String::new(),
            menu: CtxMenu::new(),
            actions: Vec::new(),
            last_click: None,
            visible,
            focused: false,
            icons_on: true,
            icon_cache: HashMap::new(),
            detail_cache: HashMap::new(),
            detail_invalidated: Vec::new(),
            brand_cache: HashMap::new(),
            rows_cache: Vec::new(),
            meta: nsql_run::meta::MetaStore::new(64 << 20),
            font_px: ICON_REF_FONT_PX,
            source_pending: false,
            row_px: 0,
            dots_step: 0,
            grouped: false,
            conn_db: String::new(),
            conn_user: String::new(),
            filter_keep: None,
            filter_hits: 0,
            filter_tokens: Vec::new(),
            filter: None,
            filter_expanded: std::collections::HashSet::new(),
            gen_opts: GenOpts::default(),
            schema_opts: SchemaOpts::default(),
            users: Vec::new(),
            rebinding: false,
            typeahead: nexa_ctl::TypeAhead::default(),
            ta_cfg: TypeAheadCfg::default(),
            now_hint: 0,
            live_results: Vec::new(),
            live_inflight: false,
            blockers_results: Vec::new(),
            blockers_inflight: false,
            offline: false,
            suspended: false,
            one_time: false,
            last_used: Instant::now(),
            menu_host: Rect::default(),
            clip: None,
            scroll_x: 0,
            content_w: 0,
            soft: HashSet::new(),
            index_done: HashSet::new(),
            index_q: VecDeque::new(),
            index_inflight: None,
            index_truncated: false,
            index_cfg: IndexCfg::default(),
            index_last_at: None,
            warm_q: VecDeque::new(),
            warm_inflight: None,
            warm_last: None,
            warm_sent: 0,
            comment_q: VecDeque::new(),
            comment_inflight: None,
            cache_path: None,
            cache_saved: false,
            complete_q: VecDeque::new(),
            completing: None,
            batching: false,
            filter_rev: 0,
            lower_labels: Vec::new(),
            fresh: Vec::new(),
            highlight_ms: 2000,
            missing_at: HashMap::new(),
            watermarks: HashMap::new(),
            wm_inflight: false,
        };
        e.reset_tree();
        e
    }

    fn reset_tree(&mut self) {
        self.soft.clear();
        self.lower_labels.clear();
        self.index_reset();
        self.complete_q.clear();
        self.completing = None;
        self.warm_q.clear();
        self.warm_inflight = None;
        self.warm_sent = 0;
        self.comment_q.clear();
        self.comment_inflight = None;
        self.fresh.clear();
        self.missing_at.clear();
        self.watermarks.clear();
        self.wm_inflight = false;
        self.meta.clear();
        self.server_schema = None;
        self.intel_buckets.clear();
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

    #[allow(dead_code)] // 호스트는 `ExplorerSet`을 거친다
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
                    ObjectKind::ExternalTable | ObjectKind::ForeignTable => IconKind::Table,
                    ObjectKind::Aggregate => IconKind::Function,
                    ObjectKind::Queue => IconKind::Queue,
                    ObjectKind::DbLink => IconKind::Link,
                    ObjectKind::JavaClass => IconKind::Java,
                    ObjectKind::Job | ObjectKind::SchedulerJob | ObjectKind::SchedulerProgram => {
                        IconKind::Job
                    }
                    ObjectKind::SchemaTrigger | ObjectKind::EventTrigger => IconKind::Trigger,
                    ObjectKind::Extension => IconKind::Package,
                };
                (k, k.color())
            }
            NodeKind::Column(_) => (IconKind::Column, IconKind::Column.color()),
            NodeKind::Sub { .. } => (IconKind::Folder, IconKind::Folder.color()),
            NodeKind::Item(it) => {
                let k = sub_icon_kind(it.icon);
                (k, k.color())
            }
        })
    }

    /// 표시 크기로 미리 스케일한 아이콘(캐시) — 페인트는 스케일 없이 그대로 찍는다.
    /// 루트(서버) 브랜드 아이콘 — `dbms_icons`(내장 SVG 26 · 방언/제품 힌트 → 이름 · 모르면 generic 틀).
    /// 서버 브랜드 아이콘(dbms_icons · 라벨 줄 포함) — 루트 행(묶음 아님) · 서버 헤더(묶음 · `ExplorerSet`) 공용. 돌려주는 값 = 그린 폭.
    #[allow(clippy::too_many_arguments)]
    fn draw_brand(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        x: i32,
        vcy: i32,
        sz: i32,
        rgb: (u8, u8, u8),
        rr: Rect,
    ) -> i32 {
        let s = self.scale;
        let hint = format!("{} {}", self.profile_name, self.conn_desc);
        let pick = dbms_icons::pick(self.dialect, &hint);
        let rgb = pick.color.unwrap_or(rgb);
        let rs = ((sz as f32) * 1.35).round() as i32;
        let dstr = Rect::new(x, vcy - rs / 2, rs, rs);
        let img = self.brand_image(pick.name, rgb, rs, rs);
        dc.image_scaled(dstr, &img, rr);
        if !pick.lines.is_empty() {
            // 세로 배치(사용자 09-22): 원통 안쪽 빈 띠 = 6.0/24 ~ 20.2/24 · 대문자 높이 ≈ 0.64·줄높이(위 0.18 여백) ·
            //   2줄 간격 2px · 1줄 상단 여백 = 2줄 하단 여백(글자와 가장 가까운 테두리 픽셀 사이).
            // 고정폭 굵게(사용자 09-22) — 1줄·2줄 같은 face·size.
            // ★ 틀 안에 맞춘다(사용자 09-23 "라벨이 DB 모양 테두리를 벗어난다"): 가장 긴 줄이 원통 안쪽 폭(≈ 0.78·rs)을
            //   넘거나 줄들이 세로 띠를 넘으면 글꼴을 1px씩 줄인다(`select_font_sized` 음수 증분 · 최대 −8).
            // ★ 09-24(사용자 "줄 사이 1px · 테두리와 글자 사이 1px · 고정폭으로 채움"): 원통 안쪽 = 옆벽 안면
            //   4.6~19.4/24 · 위 타원 바닥 6.0/24 ~ 아래 띠 안면 20.2/24 에서 1px씩 들여온 상자에, **가장 큰**
            //   글꼴(위로 +8부터 1px씩 내려 처음 맞는 크기)로 두 줄(줄 사이 1px)을 채우고 가운데 둔다.
            let m = (1.0 * s).round().max(1.0) as i32;
            let gap = m;
            let top = dstr.y + (rs as f32 * 6.0 / 24.0).round() as i32 + m;
            let bottom = dstr.y + (rs as f32 * 20.2 / 24.0).round() as i32 - m;
            let n = pick.lines.len() as i32;
            let inner_w = (rs as f32 * 14.8 / 24.0).round() as i32 - 2 * m;
            let mut delta = 8.0f32;
            let (lh2, cap, asc, total) = loop {
                dc.select_font_sized(FontSlot::Mono, true, delta);
                let lh2 = dc.text_height();
                let cap = (lh2 as f32 * 0.64).round() as i32;
                let asc = (lh2 as f32 * 0.18).round() as i32;
                let total = cap * n + gap * (n - 1);
                let wmax = pick
                    .lines
                    .iter()
                    .map(|l| dc.text_width(l))
                    .max()
                    .unwrap_or(0);
                if (wmax <= inner_w && total <= bottom - top) || delta <= -8.0 {
                    break (lh2, cap, asc, total);
                }
                delta -= 1.0;
            };
            let _ = lh2;
            let margin = (bottom - top - total) / 2;
            let mut vis_top = top + margin;
            let c = dstr.intersection(&rr);
            for line in &pick.lines {
                let tw = dc.text_width(line);
                let lx = dstr.x + (rs - tw) / 2;
                if !c.is_empty() {
                    dc.text(lx, vis_top - asc, c, line, th.text);
                }
                vis_top += cap + gap;
            }
            dc.select_font(FontSlot::Base, false);
        }
        rs
    }

    /// 폴더 개수 글자: 필터 중이면 "일치/전체"(사용자 09-25) · 아니면 전체.
    fn count_label(&self, n: &Node) -> String {
        // 부분 폴더(84 §3) = 전체 수를 아직 모른다 → "일치/?".
        let total = if n.state == LoadState::Partial {
            "?".to_string()
        } else {
            n.children.len().to_string()
        };
        match &self.filter_keep {
            Some(keep) if self.filter.is_some() => {
                let kept = n.children.iter().filter(|c| keep.contains(c)).count();
                format!("{kept}/{total}")
            }
            _ => total,
        }
    }

    fn base_depth(&self) -> usize {
        usize::from(self.grouped)
    }

    /// 묶음 모드(docs/54 §9): 참이면 루트 행이 연결(DB · 계정) 행이 되고 트리는 한 단 들여쓴다 — 서버 헤더는 `ExplorerSet`이 그린다.
    pub(crate) fn set_grouped(&mut self, on: bool) {
        if self.grouped != on {
            self.grouped = on;
            self.rows_cache = self.screen_rows();
        }
    }

    /// ★ 서버 헤더 행(묶음 모드 · `ExplorerSet`이 그룹 첫 칸에 부탁): 브랜드 아이콘 + 호스트:포트 + 흐린 부가(연결 수).
    pub(crate) fn paint_server_header(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        r: Rect,
        sub: &str,
    ) {
        if r.h <= 0 || r.w <= 0 {
            return;
        }
        let s = self.scale;
        dc.fill_rect(r, th.panel_bg);
        dc.select_font(FontSlot::Base, false);
        let asc = dc.text_ascent();
        let ty = dc.text_center_y(r.y, r.h);
        let vcy = ty + (asc as f32 * 0.62).round() as i32;
        // 시작 x = 필터 상자의 왼쪽(패널 여백 8px)과 맞춘다(사용자 09-25) — 셰브론 자리를 비우지 않는다.
        let mut x = r.x - self.scroll_x + (8.0 * s).round() as i32;
        if self.icons_on {
            let sz = (ICON_BASE_PX * self.font_px / ICON_REF_FONT_PX * s)
                .round()
                .max(8.0) as i32;
            let rgb = self
                .dialect
                .map_or(IconKind::Dbms.color(), exp_icons::dbms_color);
            let adv = self.draw_brand(dc, th, x, vcy, sz, rgb, r);
            x += adv + (6.0 * s).round() as i32;
        }
        dc.select_font(FontSlot::Base, false);
        let label = if self.endpoint.is_empty() {
            self.profile_name.clone()
        } else {
            self.endpoint.clone()
        };
        dc.text(x, ty, r, &label, th.text);
        if !sub.is_empty() {
            let sx0 = x + dc.text_width(&label) + (8.0 * s).round() as i32;
            dc.text(sx0, ty, r, sub, th.text_dim);
        }
    }

    fn brand_image(
        &mut self,
        name: &'static str,
        rgb: (u8, u8, u8),
        w: i32,
        h: i32,
    ) -> Rc<IconImage> {
        self.brand_cache
            .entry((name, rgb, w, h))
            .or_insert_with(|| {
                Rc::new(dbms_icons::image(
                    name,
                    rgb,
                    w.max(1) as u32,
                    h.max(1) as u32,
                ))
            })
            .clone()
    }

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
        if !on {
            self.typeahead.clear();
        }
        self.focused = on;
        if !on {
            self.menu.close();
        }
    }

    #[allow(dead_code)] // 호스트는 `ExplorerSet`을 거친다
    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    /// 우클릭 메뉴 닫기(풀다운과 배타 · 09-22).
    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// 열린 우클릭 메뉴의 영역(하위 메뉴 포함 · 닫혀 있으면 빈 영역) — 호스트가 "메뉴 안의 사건인가"를 판정한다.
    pub(crate) fn menu_bounds(&self) -> Rect {
        self.menu.bounds()
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        self.clamp_scroll();
    }

    /// 접속됨 — 메타 세션을 따로 연다(편집기 세션과 분리).
    /// 보이는 창 지정(이어 붙인 트리의 공용 뷰포트).
    pub(crate) fn set_clip(&mut self, r: Rect) {
        self.clip = Some(r);
    }

    /// 공용 가로 스크롤 값(호스트가 준다).
    pub(crate) fn set_scroll_x(&mut self, x: i32) {
        self.scroll_x = x;
    }

    /// 마지막 그리기에서 잰 내용 폭(가로 스크롤 범위용).
    pub(crate) fn content_width(&self) -> i32 {
        self.content_w
    }

    /// 실제로 보이는 영역 = bounds ∩ 창.
    pub(crate) fn visible_rect(&self) -> Rect {
        match self.clip {
            Some(c) => self.bounds.intersection(&c),
            None => self.bounds,
        }
    }

    /// 선택 행의 세로 범위(창 좌표 · y, 높이) — 호스트가 키보드 이동 뒤 공용 스크롤을 맞춘다.
    pub(crate) fn selected_span(&self) -> Option<(i32, i32)> {
        let sel = self.selected?;
        let pos = self
            .screen_rows()
            .iter()
            .position(|(n, _)| *n == Some(sel))?;
        let rh = self.row_h();
        Some((self.bounds.y + pos as i32 * rh - self.scroll, rh))
    }

    /// 우클릭 메뉴 영역(탐색기 전체).
    pub(crate) fn set_menu_host(&mut self, r: Rect) {
        self.menu_host = r;
    }

    /// 트리 전체 높이(px) — 여러 서버의 트리를 이어 붙일 때 이 트리가 차지하는 높이.
    pub(crate) fn content_height(&self) -> i32 {
        self.content_h().max(self.row_h())
    }

    /// 다른 서버의 트리를 눌렀다 — 선택은 탐색기 전체에 하나만.
    pub(crate) fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// 선택 행이 보이는 행 가운데 몇 번째인지와 행 수 — `ExplorerSet`이 키보드가 이웃 서버 칸으로 넘어갈지 판정한다(mac 09-21).
    pub(crate) fn selected_pos(&self) -> Option<(usize, usize)> {
        let rows = self.visible_rows();
        let sel = self.selected?;
        let pos = rows.iter().position(|&r| r == sel)?;
        Some((pos, rows.len()))
    }

    /// 보이는 행 가운데 `idx`번째를 선택(범위 밖이면 마지막 행) — 세트가 페이지·경계 이동의 목적지를 고른다.
    pub(crate) fn select_visible(&mut self, idx: usize) {
        let rows = self.visible_rows();
        if let Some(&n) = rows.get(idx.min(rows.len().saturating_sub(1))) {
            self.selected = Some(n);
            self.ensure_visible(n);
        }
    }

    /// 보이는 행 수.
    pub(crate) fn visible_count(&self) -> usize {
        self.visible_rows().len()
    }

    /// 행 높이(px) — 세트가 공용 뷰포트로 페이지 크기를 잰다(칸의 bounds는 내용 전체 높이라 쓸 수 없다).
    pub(crate) fn row_px(&self) -> i32 {
        self.row_h()
    }

    /// 타입어헤드 접두사가 살아 있는가 — 그동안 ↑/↓는 이 트리의 매치 안에서만 돈다.
    pub(crate) fn typeahead_active(&self) -> bool {
        !self.typeahead.composing().is_empty()
    }

    /// 우클릭 메뉴(팝업 층) — 모든 서버의 트리를 그린 **뒤에** 그린다(아래 칸이 위 칸의 메뉴를 덮지 않게).
    pub(crate) fn paint_menu(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.menu.paint(dc, th);
    }

    pub(crate) fn is_offline(&self) -> bool {
        self.offline
    }

    /// 이 서버에 붙은 세션이 0이 됐다 — 메타 접속만 닫고 읽어 둔 트리는 남긴다(펼치지 않은 가지는 더 못 읽는다).
    pub(crate) fn go_offline(&mut self) {
        if self.offline || self.conn_desc.is_empty() {
            return;
        }
        self.offline = true;
        self.suspended = false;
        self.gen += 1;
        self.detail_cache.clear();
        self.pending_cols.clear();
        self.missing_cols.clear();
        let _ = self.tx.send(Req::Close);
        let _ = self.tx_bg.send(Req::Close);
        let (tx, tx_bg, rx) = Self::spawn_meta(&self.wake);
        self.tx = tx;
        self.tx_bg = tx_bg;
        self.rx = rx;
        self.live_inflight = false;
        self.source_pending = false;
        for n in &mut self.nodes {
            if n.state == LoadState::Loading {
                n.state = LoadState::Idle;
            }
        }
    }

    /// 유휴 회수 — 한동안 메타 요청이 없었으면 메타 세션만 닫는다(트리·세대 유지 · 다음 요청 때 자동 재개).
    pub(crate) fn suspend_if_idle(&mut self, limit_secs: u64) {
        if limit_secs == 0
            || self.offline
            || self.one_time
            || self.suspended
            || self.conn_desc.is_empty()
            || self.live_inflight
            || self.last_used.elapsed().as_secs() < limit_secs
            || self.nodes.iter().any(|n| n.state == LoadState::Loading)
        {
            return;
        }
        self.suspended = true;
        let _ = self.tx.send(Req::Suspend);
        let _ = self.tx_bg.send(Req::Suspend);
    }

    pub(crate) fn connect(&mut self, spec: &ConnectSpec, profile_name: &str, once: bool) {
        self.one_time = once;
        self.offline = false;
        self.suspended = false;
        self.last_used = Instant::now();
        self.gen += 1;
        self.detail_cache.clear();
        self.pending_cols.clear();
        self.missing_cols.clear();
        self.dialect = None;
        self.conn_desc = spec.redacted();
        self.profile_name = profile_name.to_string();
        self.conn_db = spec.database.clone().unwrap_or_default();
        self.conn_user = spec.user.clone().unwrap_or_default();
        self.endpoint = match (&spec.host, spec.port) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.clone(),
            // 파일 방언(SQLite) = 전체 경로 대신 **파일 이름만**(사용자 09-18 · 전체 경로는 접속 창·탭 툴팁에 있다).
            _ => {
                let db = spec.database.clone().unwrap_or_default();
                std::path::Path::new(&db)
                    .file_name()
                    .map_or(db.clone(), |f| f.to_string_lossy().into_owned())
            }
        };
        self.reset_tree();
        self.cache_path = if self.index_cfg.disk_cache {
            crate::metacache::path_for(&crate::worker::cred_id(spec, Dialect::Oracle))
        } else {
            None
        };
        self.cache_saved = false;
        self.nodes[0].state = LoadState::Loading;
        let _ = self.tx.send(Req::Open {
            gen: self.gen,
            spec: spec.clone(),
            once,
        });
        // 백그라운드 스레드는 열지 않고 준비만(일회성 비밀번호는 넘기지 않는다 — 금고에서 빌리거나 못 열면 미리 읽기만 실패).
        let mut prepared = spec.clone();
        if once {
            nsql_core::secret::wipe_opt(&mut prepared.password);
        }
        let _ = self.tx_bg.send(Req::Prepare {
            gen: self.gen,
            spec: prepared,
        });
    }

    /// 접속 해제 — **서버 상태와 무관하게 즉시**(사용자 09-16: VPN 끊긴 채 "Loading…"이면 해제가 안 됐다).
    /// 메타 스레드가 카탈로그 조회에 갇혀 있을 수 있으므로 기다리지 않는다: 옛 스레드에 Close를 남기고 채널을 버리면
    /// (갇힌 호출이 타임아웃으로 풀린 뒤) 세션을 닫고 스스로 끝난다 · 다음 요청은 새 스레드가 받는다.
    pub(crate) fn disconnect(&mut self) {
        self.offline = false;
        self.suspended = false;
        self.gen += 1;
        self.detail_cache.clear();
        self.pending_cols.clear();
        self.missing_cols.clear();
        self.dialect = None;
        self.conn_desc.clear();
        self.profile_name.clear();
        self.endpoint.clear();
        self.reset_tree();
        let _ = self.tx.send(Req::Close);
        let _ = self.tx_bg.send(Req::Close);
        let (tx, tx_bg, rx) = Self::spawn_meta(&self.wake);
        self.tx = tx;
        self.tx_bg = tx_bg;
        self.rx = rx;
        self.live_inflight = false;
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExplorerAction> {
        std::mem::take(&mut self.actions)
    }

    // ── 메타 저장소(docs/47 · docs/76) — 트리에 도착한 것을 같은 시점에 담는다(서버 접속 추가 0).

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// 접속 문자열의 사용자 = 현재 스키마(`select_current_schema`와 같은 규칙).
    fn conn_user(&self) -> String {
        self.conn_desc
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split(['/', '@'])
            .next()
            .unwrap_or("")
            .to_string()
    }

    fn meta_set_schemas(&mut self, list: &[String], server: Option<&str>) {
        let user = self.conn_user();
        let current = pick_current_schema(list, server, &user);
        self.server_schema = current.clone();
        self.meta.set_schemas(list, current.as_deref());
    }

    /// ★ 접속 직후 미리 읽기(`intel.preload` · 사용자 09-23 "기본 스키마는 접속 시 바로 메모리에"): 현재 스키마의 관계 객체
    /// (테이블·뷰·구체화 뷰·시노님)와 권한 반영 사전 뷰를 메타 세션으로 한 번 요청한다(26 §8 — 접속 성공 뒤 1회 · 재시도 없음).
    fn preload_meta(&mut self) {
        if !self.preload || self.offline {
            return;
        }
        if let Some(cur) = self.server_schema.clone() {
            self.request_objects(&cur);
        }
        self.request_objects(nsql_catalog::DICT_SCHEMA);
    }

    pub(crate) fn set_preload(&mut self, on: bool) {
        self.preload = on;
    }

    pub(crate) fn set_routines(&mut self, on: bool) {
        self.routines = on;
    }

    /// 메모리 맵 보고(docs/80): 메타 저장소(탐색기·완성 공용) · 아이콘 캐시(종류 틴트 + 브랜드).
    pub(crate) fn mem_report(&self, acc: &mut crate::memstat::Acc) {
        use crate::memstat::Cat;
        // 3층(85 §5): L1 이름·목록 · L2 컬럼 · L3 상세.
        let (l1, l2, l3) = self.meta.layer_bytes();
        acc.add(Cat::Meta, l1 as u64);
        acc.add(Cat::MetaCols, l2 as u64);
        acc.add(
            Cat::MetaDetail,
            (l3 + self.detail_cache.values().map(|e| e.bytes).sum::<usize>()) as u64,
        );
        let icons: usize = self
            .icon_cache
            .values()
            .map(|i| i.rgba.len())
            .sum::<usize>()
            + self
                .brand_cache
                .values()
                .map(|i| i.rgba.len())
                .sum::<usize>();
        acc.add(Cat::Icons, icons as u64);
    }

    /// ★ 자동 완성 즉시 채움 — 스키마 하나의 관계 객체(또는 `DICT_SCHEMA` = 사전 뷰). 이미 읽었거나 읽는 중이면 0 ·
    /// 실패한 것은 다시 묻지 않는다(26 §8). 현재 스키마·사전이 아닌 스키마는 회수 대상으로 적어 둔다.
    pub(crate) fn request_objects(&mut self, schema: &str) {
        let Some(d) = self.dialect else { return };
        if self.offline {
            return;
        }
        let snap = self.meta.snapshot();
        fn cov(
            names: &nsql_run::meta::Interner,
            snap: &nsql_run::meta::Snapshot,
            sc: &str,
            k: ObjectKind,
        ) -> nsql_run::meta::Coverage {
            names
                .find(sc)
                .map_or(nsql_run::meta::Coverage::Missing, |s| snap.coverage(s, k))
        }
        if schema == nsql_catalog::DICT_SCHEMA {
            if !matches!(
                cov(&self.meta.names, &snap, schema, ObjectKind::View),
                nsql_run::meta::Coverage::Missing | nsql_run::meta::Coverage::Stale { .. }
            ) {
                return;
            }
            self.meta.mark_loading(schema, ObjectKind::View);
            self.last_used = Instant::now();
            self.suspended = false;
            let _ = self.tx_bg.send(Req::DictMeta { gen: self.gen });
            return;
        }
        let is_current = self
            .server_schema
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(schema));
        let mut sent = false;
        // 관계 넷 + (`intel.from_routines`) 함수·패키지·프로시저 — 종류당 `ALL_OBJECTS` 한 질의(26 §8 · 09-24).
        let mut kinds = vec![
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::MaterializedView,
            ObjectKind::Synonym,
        ];
        if self.routines {
            kinds.extend([
                ObjectKind::Function,
                ObjectKind::Package,
                ObjectKind::Procedure,
            ]);
        }
        for kind in kinds {
            if !nsql_catalog::kinds_for(d).contains(&kind) {
                continue;
            }
            if !matches!(
                cov(&self.meta.names, &snap, schema, kind),
                nsql_run::meta::Coverage::Missing
                    | nsql_run::meta::Coverage::Stale { .. }
                    | nsql_run::meta::Coverage::Names {
                        upgrading: false,
                        ..
                    }
            ) {
                continue;
            }
            self.meta.mark_loading(schema, kind);
            let _ = self.tx_bg.send(Req::ObjectsMeta {
                gen: self.gen,
                schema: schema.to_string(),
                kind,
            });
            sent = true;
            if !is_current
                && !self
                    .intel_buckets
                    .iter()
                    .any(|(s, k)| s.eq_ignore_ascii_case(schema) && *k == kind)
            {
                self.intel_buckets.push((schema.to_string(), kind));
            }
        }
        if sent {
            self.last_used = Instant::now();
            self.suspended = false;
        }
    }

    /// ★ 미사용 판정 즉시 회수(사용자 09-23): `스키마.`로 읽어 온 버킷 가운데 `used(스키마)`가 거짓인 것(열린 문서 어디에도
    /// 그 이름이 없음)을 바로 버린다. 반환 = 비운 버킷 수.
    pub(crate) fn reclaim_intel_buckets(&mut self, used: &dyn Fn(&str) -> bool) -> usize {
        if self.intel_buckets.is_empty() {
            return 0;
        }
        let mut n = 0;
        let mut keep = Vec::with_capacity(self.intel_buckets.len());
        for (schema, kind) in std::mem::take(&mut self.intel_buckets) {
            if used(&schema) {
                keep.push((schema, kind));
            } else if self.meta.drop_bucket(&schema, kind) {
                n += 1;
            }
        }
        self.intel_buckets = keep;
        n
    }

    fn meta_load_objects(&mut self, schema: &str, kind: ObjectKind, list: &[ObjectInfo]) {
        let objs: Vec<nsql_run::meta::NewObj> = list
            .iter()
            .map(|o| nsql_run::meta::NewObj {
                name: o.name.clone(),
                status: match o.status.as_str() {
                    "VALID" => nsql_run::meta::ObjStatus::Valid,
                    "INVALID" => nsql_run::meta::ObjStatus::Invalid,
                    _ => nsql_run::meta::ObjStatus::Unknown,
                },
                modified: None,
                comment: None,
                extra: (!o.extra.is_empty()).then(|| o.extra.clone()),
            })
            .collect();
        self.meta.load_bucket(schema, kind, &objs, Self::now_secs());
    }

    fn meta_set_columns(&mut self, schema: &str, table: &str, list: &[ColumnInfo]) {
        let Some(id) = self
            .meta
            .snapshot()
            .lookup(&self.meta.names, Some(schema), table)
        else {
            return;
        };
        let cols: Vec<nsql_run::meta::NewCol> = list
            .iter()
            .map(|c| nsql_run::meta::NewCol {
                name: c.name.clone(),
                data_type: c.data_type.clone(),
                nullable: Some(c.nullable),
                position: c.position.clamp(0, u16::MAX as i64) as u16,
                default: (!c.default.is_empty()).then(|| c.default.clone()),
                comment: None,
                key: 0,
            })
            .collect();
        self.meta.set_columns(id, &cols, Self::now_secs());
    }

    fn meta_columns_error(&mut self, schema: &str, table: &str, _e: &str) {
        // 실패 = 상태만 되돌린다(Unknown) — 자동 재시도 없음(26 §8).
        let _ = (schema, table);
    }

    /// 자동 완성이 읽는 스냅샷(접속 전이면 빈 스냅샷).
    pub(crate) fn meta_view(
        &self,
    ) -> (
        &nsql_run::meta::Interner,
        std::sync::Arc<nsql_run::meta::Snapshot>,
    ) {
        (&self.meta.names, self.meta.snapshot())
    }

    /// 자동 완성 즉시 채움(47 §4): 그 테이블 컬럼을 메타 세션으로 1건 요청(트리 노드 없이 · 이미 로드/로딩 중이면 0).
    pub(crate) fn request_columns(&mut self, schema: Option<&str>, table: &str, urgent: bool) {
        if self.dialect.is_none() {
            return;
        }
        let snap = self.meta.snapshot();
        let schema_name = match schema {
            Some(s) => s.to_string(),
            None => match snap.current_schema {
                Some(c) => self.meta.names.get(c).to_string(),
                None => return,
            },
        };
        let id = match snap.lookup(&self.meta.names, Some(&schema_name), table) {
            Some(id) => id,
            None => {
                // ★ 사전 객체(`ALL_TABLES` · `sys.tables` · `pg_catalog.pg_class` · 사용자 09-24 "ALL_TABLES 컬럼이 안 온다"):
                //   현재 스키마에 없으면 사전 버킷에서 찾는다(Oracle = 맨이름 · 그 밖 = `스키마.이름`).
                let dict_name = match (schema, self.dialect) {
                    (Some(s), Some(d)) if d != Dialect::Oracle => format!("{s}.{table}"),
                    _ => table.to_string(),
                };
                match snap.lookup(
                    &self.meta.names,
                    Some(nsql_catalog::DICT_SCHEMA),
                    &dict_name,
                ) {
                    Some(id) => id,
                    None => {
                        // 메타에 없는 이름 = 이번 세션에 생겼거나(L1 이름·디스크 캐시 이전) 아직 안 읽은 스키마 → 관계 목록을 다시 읽고 도착하면 재요청(1회).
                        let key =
                            format!("{}.{}", schema_name.to_lowercase(), table.to_lowercase());
                        if self.missing_cols.insert(key) {
                            self.meta.mark_stale(Some(&schema_name));
                            self.pending_cols.push((
                                schema_name.clone(),
                                table.to_string(),
                                urgent,
                            ));
                            self.request_objects(&schema_name);
                        }
                        return;
                    }
                }
            }
        };
        if !matches!(snap.columns(id), nsql_run::meta::ColState::Unknown) {
            return;
        }
        self.request_columns_for(id, urgent);
    }

    /// 객체 id로 컬럼 요청(사전 객체는 DB 소유 스키마로 질의 · 메타에는 자기 버킷 열쇠로 저장 · 09-24).
    fn request_columns_for(&mut self, id: nsql_run::meta::ObjId, urgent: bool) {
        let snap = self.meta.snapshot();
        let Some(o) = snap.object(id) else { return };
        let key_schema = self.meta.names.get(o.schema).to_string();
        let key_name = self.meta.names.get(o.name).to_string();
        let (db_schema, db_table) = if key_schema == nsql_catalog::DICT_SCHEMA {
            match self.dialect {
                Some(Dialect::Oracle) => ("SYS".to_string(), key_name.clone()),
                _ => match key_name.split_once('.') {
                    Some((s, n)) => (s.to_string(), n.to_string()),
                    None => ("main".to_string(), key_name.clone()),
                },
            }
        } else {
            (key_schema.clone(), key_name.clone())
        };
        self.meta.mark_columns_loading(id);
        self.last_used = Instant::now();
        self.suspended = false;
        // 급한 것(팝업이 기다림)은 급한 세션 · 선적재는 백그라운드 세션(09-24).
        let tx = if urgent { &self.tx } else { &self.tx_bg };
        let _ = tx.send(Req::ColumnsMeta {
            gen: self.gen,
            schema: db_schema,
            table: db_table,
            urgent,
            key: (key_schema, key_name),
            pkg: o.kind == ObjectKind::Package,
        });
    }

    /// 완성 상세 카드 — 테이블 상세(제약·인덱스) 1건 요청(급한 세션 · 이미 있거나 읽는 중이면 0 · 09-24).
    pub(crate) fn request_detail(&mut self, id: nsql_run::meta::ObjId) {
        if self.dialect.is_none() || self.offline {
            return;
        }
        let snap = self.meta.snapshot();
        if !matches!(snap.detail(id), nsql_run::meta::DetailState::Unknown) {
            return;
        }
        let Some(o) = snap.object(id) else { return };
        if !o.kind.is_relation() {
            return;
        }
        let (schema, table) = (
            self.meta.names.get(o.schema).to_string(),
            self.meta.names.get(o.name).to_string(),
        );
        self.meta.mark_detail_loading(id);
        self.last_used = Instant::now();
        self.suspended = false;
        // 상세는 카드용(부차) — 백그라운드 세션에서(급한 컬럼 요청을 절대 막지 않게 · 09-24 실측 1.3 s).
        let _ = self.tx_bg.send(Req::DetailMeta {
            gen: self.gen,
            schema,
            table,
        });
    }

    /// 완성 상세 카드 — 객체 id로 컬럼 1건 요청(급한 세션).
    pub(crate) fn request_columns_by_id(&mut self, id: nsql_run::meta::ObjId) {
        if self.dialect.is_none() || self.offline {
            return;
        }
        let snap = self.meta.snapshot();
        if !matches!(snap.columns(id), nsql_run::meta::ColState::Unknown) {
            return;
        }
        let Some(o) = snap.object(id) else { return };
        if !o.kind.is_relation() && o.kind != ObjectKind::Package {
            return;
        }
        self.request_columns_for(id, true);
    }

    /// 라이브 로그 폴링 요청(메타 세션 · 진행 중이면 무시).
    pub(crate) fn live_poll(&mut self, req: LiveReq) {
        if self.live_inflight || self.offline || self.dialect != Some(Dialect::Oracle) {
            return;
        }
        self.last_used = Instant::now();
        self.suspended = false;
        self.live_inflight = true;
        let _ = self.tx.send(Req::Live { gen: self.gen, req });
    }

    pub(crate) fn take_live(&mut self) -> Vec<LiveResult> {
        std::mem::take(&mut self.live_results)
    }

    /// 막힘 감지 요청(메타 세션 · 진행 중이면 무시 · 오프라인이면 안 함 — 접속한 적 없는 서버에 트래픽 0 · 26 §8).
    pub(crate) fn blockers_poll(&mut self, sid: &str) -> bool {
        if self.blockers_inflight || self.offline {
            return false;
        }
        self.last_used = Instant::now();
        self.suspended = false;
        self.blockers_inflight = true;
        let _ = self.tx.send(Req::Blockers {
            gen: self.gen,
            sid: sid.to_string(),
        });
        true
    }

    pub(crate) fn take_blockers(&mut self) -> Vec<BlockersResult> {
        std::mem::take(&mut self.blockers_results)
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
                            if std::mem::take(&mut self.rebinding) {
                                // 자격만 바뀐 재접속(카탈로그 공유 · docs/54 §10) — 트리·메타는 그대로.
                                continue;
                            }
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
                Resp::Schemas {
                    gen,
                    node,
                    r,
                    current,
                } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            self.meta_set_schemas(&list, current.as_deref());
                            let mut kids: Vec<Node> = list
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
                            // DB 수준 폴더(PG Extensions·Event Triggers · 83 §1) = 스키마 목록 뒤 · 스키마 없음("").
                            if let Some(d) = self.dialect {
                                for k in nsql_catalog::db_kinds_for(d) {
                                    kids.push(Node {
                                        kind: NodeKind::Folder {
                                            schema: String::new(),
                                            kind: *k,
                                        },
                                        depth: 1,
                                        children: Vec::new(),
                                        expanded: false,
                                        expandable: true,
                                        state: LoadState::Idle,
                                    });
                                }
                            }
                            if self.soft.remove(&node) {
                                self.diff_children(node, kids);
                            } else {
                                self.set_children(node, kids);
                                self.select_current_schema();
                                self.preload_meta();
                                // 스키마 목록이 새로 왔다 = 인덱스는 처음부터(검색 중이면 바로 다시 채움 — 필터 중 새 접속 · 사용자 09-25 1번 이미지).
                                self.index_invalidate();
                                // ★ 디스크 캐시 심기(85 §9): 지난 실행의 이름을 Names로 — 서버 L1이 곧 덮어쓴다(index_done엔 넣지 않음).
                                self.seed_from_cache();
                            }
                        }
                        // 조용한 갱신의 실패는 옛 트리를 그대로 둔다(오류 행으로 바꾸지 않는다).
                        Err(e) => {
                            if !self.soft.remove(&node) {
                                self.set_error(node, e);
                            }
                        }
                    }
                }
                Resp::Index { gen, schema, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    if self.index_inflight.as_deref() == Some(schema.as_str()) {
                        self.index_inflight = None;
                    }
                    self.index_last_at = Some(Instant::now());
                    match r {
                        Ok((list, trunc)) => {
                            // ★ L1 = MetaStore 한 곳(85 §2): 스키마의 모든 종류를 한 번에(빈 종류 = 부정 캐시) · 완성·검색이 같은 표를 읽는다 ·
                            //   일치는 스레드가 `Resp::Hits`로 따로 준다(UI 스캔 0).
                            if let Some(d) = self.dialect {
                                let names: Vec<(ObjectKind, String)> =
                                    list.into_iter().map(|e| (e.kind, e.name)).collect();
                                self.meta.load_names(
                                    &schema,
                                    nsql_catalog::kinds_for(d),
                                    &names,
                                    Self::now_secs(),
                                );
                            }
                            self.index_truncated |= trunc;
                            self.index_done.insert(schema);
                            if self.l1_complete() {
                                self.arm_warm();
                                self.save_cache();
                            }
                        }
                        // 실패한 스키마 = 트리에 읽힌 것만 검색 대상(다음 무효화 때 다시).
                        Err(_) => {
                            self.index_done.insert(schema);
                        }
                    }
                    if self.filter.is_some() {
                        self.pump_index();
                    } else {
                        // L1은 검색이 없어도 이어 간다(간격은 `l1_step`이).
                        self.index_last_at = Some(Instant::now());
                    }
                }
                Resp::Details { gen, owner, col, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    if let Ok(secs) = &r {
                        let key = match &col {
                            Some(c) => DetailTarget::Column {
                                owner: owner.clone(),
                                col: c.clone(),
                            }
                            .key(),
                            None => DetailTarget::Object(owner.clone()).key(),
                        };
                        self.detail_cache.insert(
                            key,
                            DetailEntry {
                                bytes: detail_bytes(secs),
                                sections: secs.clone(),
                                used: Instant::now(),
                                dirty: false,
                            },
                        );
                    }
                    self.actions.push(ExplorerAction::Details { owner, col, r });
                }
                Resp::Comments {
                    gen,
                    owner,
                    table,
                    cols,
                } => {
                    if gen != self.gen {
                        continue;
                    }
                    self.actions
                        .push(ExplorerAction::Comments { owner, table, cols });
                }
                Resp::SchemaComments {
                    gen,
                    schema,
                    tables,
                    cols,
                } => {
                    if gen != self.gen {
                        continue;
                    }
                    if self.comment_inflight.as_deref() == Some(schema.as_str()) {
                        self.comment_inflight = None;
                        self.warm_last = Some(Instant::now());
                    }
                    self.meta.set_schema_comments(&schema, &tables, &cols);
                    self.actions.push(ExplorerAction::CommentsLoaded { schema });
                }
                Resp::Hits { gen, rev, hits } => {
                    if gen != self.gen || rev != self.filter_rev || self.filter.is_none() {
                        continue;
                    }
                    let t0 = Instant::now();
                    let n = hits.len();
                    let added = self.materialize_hits(&hits);
                    let t1 = Instant::now();
                    // 새 노드가 없으면 다시 거를 것도 없다(입력마다 두 번 걸리던 refilter 하나 절감).
                    if added > 0 {
                        self.refilter();
                    }
                    let t2 = Instant::now();
                    self.pump_complete();
                    let ms = t0.elapsed().as_millis();
                    if ms >= 20 {
                        eprintln!(
                            "[explorer] hits {n} took {ms} ms (materialize {} · refilter {} · nodes {})",
                            (t1 - t0).as_millis(),
                            (t2 - t1).as_millis(),
                            self.nodes.len()
                        );
                    }
                }
                Resp::Objects { gen, node, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    let was_completing = self.completing == Some(node);
                    if was_completing {
                        self.completing = None;
                    }
                    match r {
                        Ok(list) => {
                            if let NodeKind::Folder { schema, kind } = &self.nodes[node].kind {
                                let (s, k) = (schema.clone(), *kind);
                                if !s.is_empty() {
                                    self.meta_load_objects(&s, k, &list);
                                    self.intel_buckets
                                        .retain(|(a, b)| !(a.eq_ignore_ascii_case(&s) && *b == k));
                                    // ★ 86 §5: 관계 폴더가 읽히면 그 스키마 코멘트를 뒤에서(기본 목록 뒤 · 순서대로).
                                    if k.is_relation() {
                                        self.enqueue_schema_comments(&s);
                                    }
                                    // 스레드의 이름 사본도 전체 목록으로(84 §6).
                                    if self.index_done.contains(&s) {
                                        let _ = self.tx_bg.send(Req::NamesUpdate {
                                            gen: self.gen,
                                            schema: s.clone(),
                                            kind: k,
                                            names: list.iter().map(|o| o.name.clone()).collect(),
                                        });
                                    }
                                }
                            }
                            let depth = self.nodes[node].depth + 1;
                            let dlg = self.dialect;
                            let kids: Vec<Node> = list
                                .into_iter()
                                .map(|o| Node {
                                    // 하위 폴더가 하나라도 있으면 펼침(83 §1 표).
                                    expandable: dlg.is_some_and(|d| {
                                        !nsql_catalog::sub_kinds(d, o.kind).is_empty()
                                    }),
                                    kind: NodeKind::Object(o),
                                    depth,
                                    children: Vec::new(),
                                    expanded: false,
                                    state: LoadState::Idle,
                                })
                                .collect();
                            if self.soft.remove(&node) {
                                self.diff_children(node, kids);
                            } else {
                                self.set_children(node, kids);
                            }
                        }
                        Err(e) => {
                            if !self.soft.remove(&node) {
                                self.set_error(node, e);
                            }
                        }
                    }
                    if was_completing {
                        self.pump_complete();
                    }
                }
                Resp::Columns { gen, node, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            // 컬럼 폴더의 주인(관계)만 메타에(83 §1 — 인덱스 컬럼은 `SubItems` 길).
                            let owner = match &self.nodes[node].kind {
                                NodeKind::Object(o) => Some((o.schema.clone(), o.name.clone())),
                                NodeKind::Sub { owner, .. } if owner.kind.is_relation() => {
                                    Some((owner.schema.clone(), owner.name.clone()))
                                }
                                _ => None,
                            };
                            if let Some((s, n)) = owner {
                                self.meta_set_columns(&s, &n, &list);
                            }
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
                            if self.soft.remove(&node) {
                                self.diff_children(node, kids);
                            } else {
                                self.set_children(node, kids);
                            }
                        }
                        Err(e) => {
                            if !self.soft.remove(&node) {
                                self.set_error(node, e);
                            }
                        }
                    }
                }
                Resp::SubItems { gen, node, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            let depth = self.nodes[node].depth + 1;
                            let kids: Vec<Node> = list
                                .into_iter()
                                .map(|it| Node {
                                    kind: NodeKind::Item(it),
                                    depth,
                                    children: Vec::new(),
                                    expanded: false,
                                    expandable: false,
                                    state: LoadState::Loaded,
                                })
                                .collect();
                            if self.soft.remove(&node) {
                                self.diff_children(node, kids);
                            } else {
                                self.set_children(node, kids);
                            }
                        }
                        Err(e) => {
                            if !self.soft.remove(&node) {
                                self.set_error(node, e);
                            }
                        }
                    }
                }
                Resp::ColumnsMeta {
                    gen,
                    schema,
                    table,
                    r,
                    key,
                } => {
                    if gen != self.gen {
                        continue;
                    }
                    if let Some(w) = self.warm_inflight {
                        let done =
                            self.meta
                                .snapshot()
                                .lookup(&self.meta.names, Some(&key.0), &key.1)
                                == Some(w);
                        if done {
                            self.warm_inflight = None;
                            self.warm_last = Some(Instant::now());
                        }
                    }
                    match r {
                        Ok(list) => self.meta_set_columns(&key.0, &key.1, &list),
                        Err(e) => {
                            self.meta_columns_error(&schema, &table, &e);
                            // 실패 = 상태 되돌림(다음 요청 때 다시 · 자동 재시도 없음).
                            if let Some(id) =
                                self.meta
                                    .snapshot()
                                    .lookup(&self.meta.names, Some(&key.0), &key.1)
                            {
                                self.meta.reset_columns(id);
                            }
                        }
                    }
                }
                Resp::ObjectsMeta {
                    gen,
                    schema,
                    kind,
                    r,
                } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => {
                            self.meta_load_objects(&schema, kind, &list);
                            self.retry_pending_cols(&schema);
                            // ★ L2(85 §3): 현재 스키마의 관계 목록이 오면 그 컬럼을 뒤에서 미리 읽는 큐에.
                            if kind.is_relation()
                                && self
                                    .server_schema
                                    .as_deref()
                                    .is_some_and(|c| c.eq_ignore_ascii_case(&schema))
                            {
                                self.enqueue_warm_columns(&schema, kind);
                            }
                        }
                        Err(e) => self.meta.mark_error(&schema, kind, &e, Self::now_secs()),
                    }
                }
                Resp::DetailMeta {
                    gen,
                    schema,
                    table,
                    r,
                } => {
                    if gen != self.gen {
                        continue;
                    }
                    let snap = self.meta.snapshot();
                    let Some(id) = snap.lookup(&self.meta.names, Some(&schema), &table) else {
                        continue;
                    };
                    match r {
                        Ok(d) => {
                            let nd = nsql_run::meta::NewDetail {
                                keys: d
                                    .keys
                                    .into_iter()
                                    .map(|k| (k.name, k.kind, k.cols, k.ref_table))
                                    .collect(),
                                indexes: d
                                    .indexes
                                    .into_iter()
                                    .map(|i| (i.name, i.unique, i.cols))
                                    .collect(),
                                comment: d.comment,
                                col_comments: d.col_comments,
                            };
                            self.meta.set_detail(id, &nd, Self::now_secs());
                        }
                        Err(_) => self.meta.reset_detail(id),
                    }
                }
                Resp::DictMeta { gen, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    match r {
                        Ok(list) => self.meta_load_objects(
                            nsql_catalog::DICT_SCHEMA,
                            ObjectKind::View,
                            &list,
                        ),
                        Err(e) => self.meta.mark_error(
                            nsql_catalog::DICT_SCHEMA,
                            ObjectKind::View,
                            &e,
                            Self::now_secs(),
                        ),
                    }
                }
                Resp::Watermark { gen, r } => {
                    self.wm_inflight = false;
                    if gen != self.gen {
                        continue;
                    }
                    for (schema, v) in r {
                        let Ok(v) = v else { continue };
                        let old = self.watermarks.insert(schema.clone(), v.clone());
                        // 처음 읽은 값은 기준일 뿐 — **앞선 값과 다를 때만** 그 스키마의 읽어 둔 폴더를 조용히 다시 읽는다.
                        if old.is_some_and(|o| o != v) {
                            if let Some(i) = self.schema_node(Some(&schema)) {
                                self.soft_refresh_subtree(i);
                            }
                        }
                    }
                }
                Resp::Live { gen, r } => {
                    self.live_inflight = false;
                    if gen != self.gen {
                        continue;
                    }
                    self.live_results.push(r);
                }
                Resp::Blockers { gen, r } => {
                    self.blockers_inflight = false;
                    if gen != self.gen {
                        continue;
                    }
                    self.blockers_results.push(r);
                }
                Resp::GenSql { gen, spec, r } => {
                    if gen != self.gen {
                        continue;
                    }
                    self.actions.push(ExplorerAction::Preview {
                        spec,
                        r,
                        server: None,
                    });
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
        if self.filter.is_some() && !self.batching {
            self.refilter();
        }
    }

    /// **디프 반영**(docs/57 §2-2): 새 목록과 옛 자식을 열쇠(이름+종류)로 맞춘다 — 남는 노드는 **그대로**(펼침·자식·선택 보존) ·
    /// 새 노드는 새 목록의 자리에 · 사라진 노드만 뗀다(선택돼 있었으면 부모로). 새 노드는 잠깐 강조(선택은 옮기지 않는다).
    fn diff_children(&mut self, node: usize, kids: Vec<Node>) {
        let old: Vec<usize> = std::mem::take(&mut self.nodes[node].children);
        let mut by_key: HashMap<String, usize> = old
            .iter()
            .map(|&i| (node_key(&self.nodes[i].kind), i))
            .collect();
        let until = self.now_hint + self.highlight_ms;
        let mut ids = Vec::with_capacity(kids.len());
        for k in kids {
            match by_key.remove(&node_key(&k.kind)) {
                Some(i) => {
                    // 같은 객체 — 표시 정보(상태·형식)만 새 값으로 · 구조는 보존.
                    self.nodes[i].kind = k.kind;
                    if let Some(c) = self.lower_labels.get_mut(i) {
                        *c = None;
                    }
                    ids.push(i);
                }
                None => {
                    self.nodes.push(k);
                    let i = self.nodes.len() - 1;
                    if self.highlight_ms > 0 && !old.is_empty() {
                        self.fresh.push((i, until));
                    }
                    ids.push(i);
                }
            }
        }
        // 남은 것 = 사라진 객체.
        for (_, gone) in by_key {
            let had_sel = self
                .selected
                .is_some_and(|s| s == gone || self.is_under(s, gone));
            self.detach(gone);
            self.missing_at.remove(&gone);
            if had_sel {
                self.selected = Some(node);
            }
        }
        self.nodes[node].children = ids;
        self.nodes[node].state = LoadState::Loaded;
        self.clamp_scroll();
        if self.filter.is_some() && !self.batching {
            self.refilter();
        }
    }

    /// `i`가 `anc`의 자손인가(깊이가 얕아 재귀로 충분).
    fn is_under(&self, i: usize, anc: usize) -> bool {
        self.nodes[anc]
            .children
            .iter()
            .any(|&c| c == i || self.is_under(i, c))
    }

    /// 조용한 갱신 — 이미 읽어 둔 노드만(안 읽은 폴더는 펼칠 때 새로 읽으므로 할 일이 없다) · 상태·펼침은 건드리지 않는다.
    fn soft_refresh(&mut self, i: usize) -> bool {
        if self.offline || self.nodes[i].state != LoadState::Loaded || self.soft.contains(&i) {
            return false;
        }
        let gen = self.gen;
        let req = match self.nodes[i].kind.clone() {
            NodeKind::Root => Req::Schemas {
                gen,
                node: i,
                opts: self.schema_opts,
            },
            NodeKind::Folder { schema, kind } => Req::Objects {
                gen,
                node: i,
                schema,
                kind,
            },
            NodeKind::Sub { owner, sub } if sub == SubKind::Columns && owner.kind.is_relation() => {
                Req::Columns {
                    gen,
                    node: i,
                    schema: owner.schema.clone(),
                    table: owner.name.clone(),
                }
            }
            NodeKind::Sub { owner, sub } => Req::SubItems {
                gen,
                node: i,
                owner: *owner,
                sub,
            },
            _ => return false,
        };
        self.last_used = Instant::now();
        self.suspended = false;
        self.soft.insert(i);
        let _ = self.tx.send(req);
        true
    }

    /// `root` 아래(자신 포함)의 읽어 둔 노드를 전부 조용히 다시 읽는다 — 돌려주는 값 = 보낸 요청 수.
    fn soft_refresh_subtree(&mut self, root: usize) -> usize {
        let mut n = 0;
        let mut stack = vec![root];
        while let Some(i) = stack.pop() {
            stack.extend(self.nodes[i].children.iter().copied());
            n += usize::from(self.soft_refresh(i));
        }
        n
    }

    /// 노드의 부모(노드는 부모를 들고 있지 않다 — 드문 동작이라 훑어 찾는다).
    fn parent_of(&self, i: usize) -> Option<usize> {
        self.nodes.iter().position(|n| n.children.contains(&i))
    }

    /// 노드가 속한 스키마 이름(루트 = None) — 부모를 따라 올라간다.
    fn schema_of(&self, i: usize) -> Option<String> {
        let mut cur = Some(i);
        while let Some(n) = cur {
            match &self.nodes[n].kind {
                NodeKind::Schema(s) => return Some(s.clone()),
                NodeKind::Folder { schema, .. } => return Some(schema.clone()),
                NodeKind::Object(o) => return Some(o.schema.clone()),
                _ => {}
            }
            cur = self.parent_of(n);
        }
        None
    }

    /// ★ 명시 메타 갱신(79 §4 · T-188): `schema`(None = 이 서버 전부 + 사전)를 `Stale`/`Unknown`으로 표시하고 그 스키마(전부면
    /// 현재 스키마·사전)는 바로 다시 읽는다(백그라운드 세션 · 미리 읽기와 같은 길). 반환 = (표시한 버킷, 비운 객체).
    pub(crate) fn refresh_meta(&mut self, schema: Option<&str>) -> (usize, usize) {
        self.invalidate_details(schema, None);
        let buckets = self.meta.mark_stale(schema);
        let objs = self.meta.mark_columns_unknown(schema);
        self.index_invalidate();
        match schema {
            Some(s) => self.request_objects(s),
            None => {
                let _ = self.meta.mark_stale(Some(nsql_catalog::DICT_SCHEMA));
                if let Some(cur) = self.server_schema.clone() {
                    self.request_objects(&cur);
                }
                self.request_objects(nsql_catalog::DICT_SCHEMA);
            }
        }
        (buckets, objs)
    }

    /// 서버가 말한 현재 스키마.
    pub(crate) fn server_schema(&self) -> Option<&str> {
        self.server_schema.as_deref()
    }

    /// **수동 새로 고침 = 계층형**(docs/57 T3 · 사용자 09-19 우클릭 메뉴 · F5): 고른 노드 **아래 전부**가 대상이다.
    ///
    /// | 고른 곳 | 다시 읽는 것 |
    /// |---|---|
    /// | 서버(루트) | 스키마 목록 + 읽어 둔 모든 종류 폴더 + 컬럼을 읽어 둔 모든 객체 |
    /// | 스키마 | 그 스키마의 읽어 둔 폴더·객체 |
    /// | 종류 폴더(Tables …) | 그 종류의 객체 목록 + 그 아래 컬럼을 읽어 둔 객체 |
    /// | 테이블·뷰 | 그 객체의 컬럼만 |
    /// | 그 밖의 객체 · 컬럼 | 자기가 속한 목록(폴더 / 테이블) 하나 |
    ///
    /// 읽어 둔 것은 **디프**로(펼침·선택·스크롤 보존) · 아직 안 읽었거나 오류인 노드는 새로 읽는다 · `hard` = 캐시를 버리고
    /// 그 노드부터 새로(Shift+F5). 아직 펼쳐 본 적 없는 하위는 대상이 아니다 — 펼칠 때 어차피 새로 읽는다.
    pub(crate) fn refresh_selected(&mut self, hard: bool) {
        let picked = self.selected.unwrap_or(0);
        // 자식이 없는 노드(프로시저 같은 객체 · 컬럼)는 자기가 속한 목록을 다시 읽는다.
        let leaf = match &self.nodes[picked].kind {
            NodeKind::Column(_) | NodeKind::Item(_) => true,
            NodeKind::Object(o) => self
                .dialect
                .is_none_or(|d| nsql_catalog::sub_kinds(d, o.kind).is_empty()),
            _ => false,
        };
        let i = if leaf {
            self.parent_of(picked).unwrap_or(picked)
        } else {
            picked
        };
        // ★ **연결이 해제된 서버**(오프라인 · 사용자 09-21 "새로 고침해도 아무 동작이 없다 — 해제된 상태에 맞는 정보가 보여야"):
        //   새로 읽을 수 없으므로, 고른 자리의 읽어 둔 목록을 **"접속 안 됨" 안내 줄로 바꾼다**(옛 목록을 새로 고친 것처럼 두지 않는다) +
        //   상태줄·경고 토스트. 접힌 노드는 펼치지 않고 읽어 둔 것만 버린다(다음에 펼치면 같은 안내가 나온다). 스키마 = 그 아래 폴더들.
        if self.offline {
            let msg = t(Msg::ExpOfflineRefresh).to_string();
            let targets: Vec<usize> = match &self.nodes[i].kind {
                NodeKind::Root => Vec::new(),
                NodeKind::Schema(_) => self.nodes[i].children.clone(),
                _ => vec![i],
            };
            for n in targets {
                if self.nodes[n].state == LoadState::Idle && self.nodes[n].children.is_empty() {
                    continue;
                }
                let old: Vec<usize> = std::mem::take(&mut self.nodes[n].children);
                for o in old {
                    self.detach(o);
                }
                // 안내 줄은 짧게("접속 안 됨" — 좁은 패널에서 잘리지 않게) · 긴 설명은 토스트·상태줄에.
                self.nodes[n].state = if self.nodes[n].expanded {
                    LoadState::Error(t(Msg::ExpNotConnected).to_string())
                } else {
                    LoadState::Idle
                };
            }
            self.clamp_scroll();
            self.actions.push(ExplorerAction::Notice(msg));
            return;
        }
        let what = self.label(i).0;
        // ★ 메타(완성·툴팁 공용)도 같이 낡음 표시(79 §3 · T-187): 루트 = 전부 · 그 밖 = 그 스키마(버킷 Stale · 컬럼 Unknown · 목록은
        //   유지 → 다음 요청 때 다시 읽음). 트리 자체의 다시 읽기는 아래 종전 규칙.
        match &self.nodes[i].kind {
            NodeKind::Root => {
                let _ = self.meta.mark_stale(None);
                let _ = self.meta.mark_columns_unknown(None);
            }
            _ => {
                if let Some(sc) = self.schema_of(i) {
                    let _ = self.meta.mark_stale(Some(&sc));
                    let _ = self.meta.mark_columns_unknown(Some(&sc));
                }
            }
        }
        let sent = if hard || self.nodes[i].state != LoadState::Loaded {
            if i == 0 {
                self.watermarks.clear();
            }
            if self.nodes[i].expanded {
                self.refresh(i);
                1
            } else {
                // ★ **접혀 있는 노드는 새로 고침으로 펼치지 않는다**(사용자 09-21 — 한 번도 열지 않은 스키마에서 새로 고침을 하면
                //   저절로 펼쳐졌다). 읽어 둔 것이 있으면 버리기만 한다 → 다음에 펼칠 때 새로 읽는다(그것이 곧 새로 고침이다).
                let old: Vec<usize> = std::mem::take(&mut self.nodes[i].children);
                for o in old {
                    self.detach(o);
                }
                self.nodes[i].state = LoadState::Idle;
                0
            }
        } else if leaf {
            // 목록 하나만(그 아래 다른 객체의 컬럼까지 건드리지 않는다).
            usize::from(self.soft_refresh(i))
        } else {
            self.soft_refresh_subtree(i)
        };
        // ★ 고른 결과는 **항상** 상태줄에 남긴다(사용자 09-21 "새로 고침이 안 된다" — 바뀐 것이 없으면 화면이 그대로라
        //   눌렸는지조차 알 수 없었다): 오프라인 = 연결 안 됨 · 그 밖 = 새로 고침함(다시 읽을 것이 없는 스키마 노드 포함).
        let _ = sent;
        self.actions.push(ExplorerAction::Status(tf(
            Msg::StExplorerRefreshed,
            &[&what],
        )));
    }

    /// 이름으로 스키마 노드 찾기(대소문자 무시 · `None` = 접속 계정의 스키마 → 스키마가 하나뿐이면 그것).
    fn schema_node(&self, schema: Option<&str>) -> Option<usize> {
        let kids = &self.nodes[0].children;
        let find = |name: &str| {
            kids.iter().copied().find(
                |&i| matches!(&self.nodes[i].kind, NodeKind::Schema(s) if s.eq_ignore_ascii_case(name)),
            )
        };
        match schema {
            Some(s) => find(s),
            None => {
                let user = self
                    .conn_desc
                    .split("://")
                    .nth(1)
                    .unwrap_or("")
                    .split(['/', '@'])
                    .next()
                    .unwrap_or("");
                self.server_schema
                    .as_deref()
                    .and_then(&find)
                    .or_else(|| find(user))
                    .or_else(|| find("public"))
                    .or_else(|| find("dbo"))
                    .or_else(|| (kids.len() == 1).then(|| kids[0]))
            }
        }
    }

    fn folder_node(&self, schema_node: usize, kind: ObjectKind) -> Option<usize> {
        self.nodes[schema_node].children.iter().copied().find(
            |&i| matches!(&self.nodes[i].kind, NodeKind::Folder { kind: k, .. } if *k == kind),
        )
    }

    /// **실행한 DDL 반영**(docs/57 T1): 그 (스키마, 종류) 폴더 하나만 조용히 다시 읽는다. `default_schema` = 문장에 스키마가
    /// 없을 때 그 세션의 현재 스키마(모르면 접속 계정). 돌려주는 값 = 보낸 요청 수(0 = 읽어 둔 적 없는 자리라 할 일 없음).
    pub(crate) fn apply_ddl(
        &mut self,
        t: &nsql_core::DdlTarget,
        default_schema: Option<&str>,
    ) -> usize {
        use nsql_core::{DdlKind, DdlVerb};
        // 상세 캐시 무효화(DDL이 닿은 객체 · 스키마 없으면 기본 스키마) — 다음 클릭에 다시 읽는다.
        let sc_owned = t
            .schema
            .clone()
            .or_else(|| default_schema.map(str::to_string));
        self.invalidate_details(sc_owned.as_deref(), Some(&t.name));
        let Some(kind) = folder_kind(t.kind) else {
            // 스키마/사용자 = 루트의 스키마 목록.
            return usize::from(self.soft_refresh(0));
        };
        let Some(sn) = self.schema_node(t.schema.as_deref().or(default_schema)) else {
            return 0;
        };
        let Some(folder) = self.folder_node(sn, kind) else {
            return 0;
        };
        let mut n = 0;
        if t.verb.changes_list() {
            n += usize::from(self.soft_refresh(folder));
            // 테이블이 사라지면 딸린 인덱스·트리거 목록도 바뀐다.
            if t.verb == DdlVerb::Drop && t.kind == DdlKind::Table {
                for k in [ObjectKind::Index, ObjectKind::Trigger] {
                    if let Some(f) = self.folder_node(sn, k) {
                        n += usize::from(self.soft_refresh(f));
                    }
                }
            }
        }
        // 구조 변경(ALTER · COMMENT) = 그 객체가 펼쳐져(컬럼을 읽어) 있으면 컬럼만.
        if matches!(t.verb, DdlVerb::Alter | DdlVerb::Comment) {
            let obj = self.nodes[folder].children.iter().copied().find(
                |&i| matches!(&self.nodes[i].kind, NodeKind::Object(o) if o.name.eq_ignore_ascii_case(&t.name)),
            );
            if let Some(o) = obj {
                // 객체 아래 = 하위 폴더(83 §1) → 읽어 둔 폴더(컬럼·제약·인덱스 …)만 다시.
                n += self.soft_refresh_subtree(o);
            }
        }
        n
    }

    /// **"못 찾음" 신호**(docs/57 T4): 실행이 "테이블/뷰 없음"으로 실패했다 — 그 이름이 트리에 **있으면** 트리가 낡은 것 →
    /// 그 폴더를 다시 읽는다. 이름을 모르면(오류문에 이름이 없는 방언) 현재 스키마의 테이블·뷰 폴더. 폴더당 60초에 1회.
    pub(crate) fn note_missing(
        &mut self,
        name: Option<&str>,
        default_schema: Option<&str>,
    ) -> usize {
        let rel = [
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::MaterializedView,
        ];
        let mut folders: Vec<usize> = Vec::new();
        match name {
            Some(full) => {
                let last = full.rsplit('.').next().unwrap_or(full);
                for (i, n) in self.nodes.iter().enumerate() {
                    let NodeKind::Folder { kind, .. } = &n.kind else {
                        continue;
                    };
                    if !rel.contains(kind) || n.state != LoadState::Loaded {
                        continue;
                    }
                    let has = n.children.iter().any(
                        |&c| matches!(&self.nodes[c].kind, NodeKind::Object(o) if o.name.eq_ignore_ascii_case(last)),
                    );
                    if has {
                        folders.push(i);
                    }
                }
            }
            None => {
                if let Some(sn) = self.schema_node(default_schema) {
                    folders.extend(rel.iter().filter_map(|k| self.folder_node(sn, *k)));
                }
            }
        }
        let now = Instant::now();
        let mut n = 0;
        for f in folders {
            let recent = self
                .missing_at
                .get(&f)
                .is_some_and(|t| now.duration_since(*t) < Duration::from_secs(60));
            if !recent && self.soft_refresh(f) {
                self.missing_at.insert(f, now);
                n += 1;
            }
        }
        n
    }

    /// **유휴 워터마크**(docs/57 T2): 읽어 둔 폴더가 있는 스키마마다 지문 1행을 묻는다(`all` = 지문 없이 읽어 둔 것 전부 다시).
    /// 메타 세션이 유휴로 닫혀 있으면 **깨우지 않는다**(주기 폴링이 접속을 되살리면 유휴 회수가 무의미해진다).
    pub(crate) fn watermark_poll(&mut self, all: bool) -> bool {
        if self.offline || self.suspended || self.wm_inflight || self.dialect.is_none() {
            return false;
        }
        if all {
            return self.soft_refresh_subtree(0) > 0;
        }
        let schemas: Vec<String> = self.nodes[0]
            .children
            .iter()
            .filter_map(|&i| match &self.nodes[i].kind {
                NodeKind::Schema(s)
                    if self.nodes[i]
                        .children
                        .iter()
                        .any(|&f| self.nodes[f].state == LoadState::Loaded) =>
                {
                    Some(s.clone())
                }
                _ => None,
            })
            .collect();
        if schemas.is_empty() {
            return false;
        }
        self.wm_inflight = true;
        let _ = self.tx.send(Req::Watermark {
            gen: self.gen,
            schemas,
        });
        true
    }

    /// 새 객체 강조 시간(설정 `meta.refresh_highlight_ms`).
    pub(crate) fn set_highlight_ms(&mut self, ms: u64) {
        self.highlight_ms = ms;
    }

    /// 공용 스크롤 고정용(`ExplorerSet`): 이 칸 안 `off` px 지점의 (노드, 행 안쪽 px).
    pub(crate) fn anchor_at(&self, off: i32) -> Option<(usize, i32)> {
        let h = self.row_h().max(1);
        let rows = self.screen_rows();
        let idx = (off / h).max(0) as usize;
        // 상태 행이면 그 위의 실제 노드로.
        let node = rows.get(idx).map(|(n, parent)| n.unwrap_or(*parent))?;
        Some((node, off - idx as i32 * h))
    }

    /// 노드의 화면 행 위쪽 px(이 칸 기준 · 안 보이면 `None`).
    pub(crate) fn row_top_of(&self, node: usize) -> Option<i32> {
        let h = self.row_h();
        self.screen_rows()
            .iter()
            .position(|(n, _)| *n == Some(node))
            .map(|p| p as i32 * h)
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
        if self.caret == Some(i) {
            self.caret = None;
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
        // 사용자가 직접 접거나 펼친 노드 = 필터가 대신 펼친 기록에서 뺀다(필터 해제 때 건드리지 않는다).
        self.filter_expanded.remove(&i);
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
        if self.nodes[i].state == LoadState::Partial {
            // 부분 폴더를 직접 펼침 = 전체를 조용히 채운다(있는 항목은 그대로 · 84 §4).
            self.request_complete(i);
            return;
        }
        self.load(i);
    }

    /// (재)로드 요청 — Schema는 로컬로 폴더를 만든다(서버 왕복 0).
    fn load(&mut self, i: usize) {
        // 오프라인(이 서버에 붙은 세션 0) = 읽어 둔 것만 보여 준다 — 새로 읽으러 서버에 붙지 않는다(사용자가 끊은 서버).
        if self.offline {
            if !matches!(self.nodes[i].kind, NodeKind::Schema(_)) {
                self.nodes[i].state = LoadState::Error(t(Msg::ExpNotConnected).to_string());
                return;
            }
        } else {
            self.last_used = Instant::now();
            self.suspended = false;
        }
        let gen = self.gen;
        match self.nodes[i].kind.clone() {
            NodeKind::Root => {
                self.nodes[i].state = LoadState::Loading;
                let _ = self.tx.send(Req::Schemas {
                    gen,
                    node: i,
                    opts: self.schema_opts,
                });
            }
            NodeKind::Schema(_) => {
                self.make_folders(i);
                self.nodes[i].expanded = true;
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
            // 객체 = 하위 폴더(표 · 83 §1) — 서버 왕복 없이 즉시 · 잎은 폴더를 펼칠 때 읽는다.
            NodeKind::Object(o) => {
                let Some(d) = self.dialect else { return };
                let subs = nsql_catalog::sub_kinds(d, o.kind);
                if subs.is_empty() {
                    return;
                }
                let depth = self.nodes[i].depth + 1;
                let owner = Box::new(o);
                let kids: Vec<Node> = subs
                    .iter()
                    .map(|sub| Node {
                        kind: NodeKind::Sub {
                            owner: owner.clone(),
                            sub: *sub,
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
            NodeKind::Sub { owner, sub } => {
                self.nodes[i].state = LoadState::Loading;
                let req = if sub == SubKind::Columns && owner.kind.is_relation() {
                    // 관계의 컬럼 = 메타 저장소와 공용 길(`Column` 노드 · 완성에 바로 쓰인다).
                    Req::Columns {
                        gen,
                        node: i,
                        schema: owner.schema.clone(),
                        table: owner.name.clone(),
                    }
                } else {
                    Req::SubItems {
                        gen,
                        node: i,
                        owner: *owner,
                        sub,
                    }
                };
                let _ = self.tx.send(req);
            }
            _ => {}
        }
    }

    fn refresh(&mut self, i: usize) {
        self.invalidate_details_at(i);
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
            NodeKind::Item(it) => self.actions.push(ExplorerAction::Copy(it.name)),
            _ => self.toggle(i),
        }
    }

    /// 잎 노드의 주인(객체 종류 · 하위 폴더) — 부모 `Sub` 노드에서.
    fn item_owner(&self, i: usize) -> Option<(ObjectKind, SubKind)> {
        let p = self.parent_of(i)?;
        match &self.nodes[p].kind {
            NodeKind::Sub { owner, sub } => Some((owner.kind, *sub)),
            _ => None,
        }
    }

    /// 우클릭 ▸ Generate SQL ▸ 항목(83 §3): 객체 = 그 객체 · 잎(제약·인덱스·트리거) = 주인 객체 + `(폴더, 이름)`.
    fn gen_pick(&mut self, i: usize, what: nsql_catalog::GenWhat) {
        let spec = match &self.nodes[i].kind {
            NodeKind::Object(o) => GenSpec {
                owner: o.clone(),
                what,
                sub: None,
                opts: self.gen_opts,
            },
            NodeKind::Item(it) => {
                let Some(p) = self.parent_of(i) else { return };
                let NodeKind::Sub { owner, sub } = &self.nodes[p].kind else {
                    return;
                };
                GenSpec {
                    owner: (**owner).clone(),
                    what,
                    sub: Some((*sub, it.name.clone())),
                    opts: self.gen_opts,
                }
            }
            _ => return,
        };
        self.gen_sql(spec);
    }

    pub(crate) fn set_gen_opts(&mut self, opts: GenOpts) {
        self.gen_opts = opts;
    }

    pub(crate) fn set_schema_opts(&mut self, opts: SchemaOpts) {
        self.schema_opts = opts;
    }

    /// 스키마 목록을 조용히 다시(설정 변경 · 읽어 둔 루트만).
    pub(crate) fn reload_schemas(&mut self) {
        if self.nodes[0].state == LoadState::Loaded && !self.offline {
            self.soft_refresh(0);
        }
    }

    pub(crate) fn set_users(&mut self, users: Vec<String>) {
        self.users = users;
    }

    pub(crate) fn bound_user(&self) -> String {
        self.conn_user.clone()
    }

    /// ★ 자격만 바꾸는 재접속(docs/54 §10): 메타 세션을 지금 붙어 있는 다른 연결의 자격으로 다시 연다 — 트리·메타 저장소는 그대로.
    /// 진행 중이던 요청은 세대가 바뀌어 버려지므로 읽는 중이던 노드는 `Idle`로(다시 펼치면 읽는다).
    pub(crate) fn rebind(&mut self, spec: &ConnectSpec) {
        self.gen += 1;
        self.detail_cache.clear();
        self.pending_cols.clear();
        self.missing_cols.clear();
        self.rebinding = true;
        self.offline = false;
        self.suspended = false;
        self.one_time = false;
        self.last_used = Instant::now();
        self.conn_desc = spec.redacted();
        self.conn_user = spec.user.clone().unwrap_or_default();
        for n in &mut self.nodes {
            if n.state == LoadState::Loading {
                n.state = LoadState::Idle;
            }
        }
        self.soft.clear();
        self.source_pending = false;
        let _ = self.tx.send(Req::Open {
            gen: self.gen,
            spec: spec.clone(),
            once: false,
        });
        let _ = self.tx_bg.send(Req::Prepare {
            gen: self.gen,
            spec: spec.clone(),
        });
    }

    /// 자체 시험(86): `row`번째 보이는 행을 선택한다(클릭과 같은 결과 · 펼치지 않음).
    pub(crate) fn capture_select(&mut self, row: usize) -> bool {
        let rows = self.visible_rows();
        let Some(&i) = rows.get(row) else {
            return false;
        };
        self.selected = Some(i);
        true
    }

    /// ★ 객체 상세 패널의 대상(86): 선택 노드 → 객체/컬럼(주인 포함)/잎(주인·하위 폴더 포함)/스키마.
    pub(crate) fn selected_target(&self) -> Option<DetailTarget> {
        let i = self.selected?;
        let n = self.nodes.get(i)?;
        match &n.kind {
            NodeKind::Object(o) => Some(DetailTarget::Object(o.clone())),
            NodeKind::Schema(s) => Some(DetailTarget::Schema(s.clone())),
            NodeKind::Column(c) => {
                let mut cur = self.parent_of(i);
                while let Some(p) = cur {
                    match &self.nodes[p].kind {
                        NodeKind::Object(o) => {
                            return Some(DetailTarget::Column {
                                owner: o.clone(),
                                col: c.clone(),
                            })
                        }
                        NodeKind::Sub { owner, .. } => {
                            return Some(DetailTarget::Column {
                                owner: (**owner).clone(),
                                col: c.clone(),
                            })
                        }
                        _ => cur = self.parent_of(p),
                    }
                }
                None
            }
            NodeKind::Item(it) => {
                let p = self.parent_of(i)?;
                match &self.nodes[p].kind {
                    NodeKind::Sub { owner, sub } => Some(DetailTarget::Item {
                        owner: (**owner).clone(),
                        sub: *sub,
                        item: it.clone(),
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// ★ 스키마의 종류별 객체 수(86 §3 · 서버 왕복 0): 메타 저장소에 목록이 있는 종류만(L1 이름 층 포함) · (종류, 개수).
    pub(crate) fn schema_kind_counts(&self, schema: &str) -> Vec<(ObjectKind, usize)> {
        let Some(d) = self.dialect else {
            return Vec::new();
        };
        let Some(sc) = self.meta.names.find(schema) else {
            return Vec::new();
        };
        let snap = self.meta.snapshot();
        nsql_catalog::kinds_for(d)
            .iter()
            .filter_map(|&k| match snap.coverage(sc, k) {
                nsql_run::meta::Coverage::Loaded { n, .. }
                | nsql_run::meta::Coverage::Stale { n, .. }
                | nsql_run::meta::Coverage::Names { n, .. } => Some((k, n)),
                _ => None,
            })
            .collect()
    }

    /// 테이블·컬럼 코멘트 요청(86 §4 · 급한 세션 · 테이블 단위 1~2 질의 · 값 그대로).
    pub(crate) fn request_comments(&mut self, owner: ObjectInfo) {
        if self.dialect.is_none() || self.offline {
            return;
        }
        self.last_used = Instant::now();
        self.suspended = false;
        let _ = self.tx.send(Req::Comments {
            gen: self.gen,
            owner,
        });
    }

    /// 객체 상세 요청(86 · L3 = 즉시 · 급한 세션).
    pub(crate) fn request_details(&mut self, owner: ObjectInfo, col: Option<ColumnInfo>) {
        if self.dialect.is_none() || self.offline {
            return;
        }
        self.last_used = Instant::now();
        self.suspended = false;
        let _ = self.tx.send(Req::Details {
            gen: self.gen,
            owner,
            col,
            opts: self.gen_opts,
        });
    }

    /// 관계 목록이 새로 왔다 → 그 스키마에서 기다리던 컬럼 요청을 다시(이제는 객체가 있다 · 없으면 `missing_cols`가 되묻지 않게 막는다).
    fn retry_pending_cols(&mut self, schema: &str) {
        let mine: Vec<(String, String, bool)> = {
            let (a, b): (Vec<_>, Vec<_>) = self
                .pending_cols
                .drain(..)
                .partition(|(sc, _, _)| sc.eq_ignore_ascii_case(schema));
            self.pending_cols = b;
            a
        };
        for (sc, table, urgent) in mine {
            if self
                .meta
                .snapshot()
                .lookup(&self.meta.names, Some(&sc), &table)
                .is_some()
            {
                self.request_columns(Some(&sc), &table, urgent);
            }
        }
    }

    /// 캐시된 상세(있으면 즉시 표시용 사본 · `true` = 새로 고침 범위였거나 TTL이 지나 **다시 읽어 교체**해야 한다).
    pub(crate) fn cached_details(&mut self, key: &str) -> Option<(Vec<DetailSection>, bool)> {
        let ttl = Duration::from_secs(self.index_cfg.detail_ttl_secs.max(1));
        let e = self.detail_cache.get_mut(key)?;
        let stale = e.dirty || e.used.elapsed() > ttl;
        e.used = Instant::now();
        Some((e.sections.clone(), stale))
    }

    /// 새로 고침 범위 전파: 스키마(`name` 없음) 또는 객체 하나의 상세를 무효화(다음 클릭에 다시 읽는다).
    fn invalidate_details(&mut self, schema: Option<&str>, name: Option<&str>) {
        for (k, e) in &mut self.detail_cache {
            if detail_key_hit(k, schema, name) {
                e.dirty = true;
            }
        }
        self.detail_invalidated
            .push((schema.map(str::to_string), name.map(str::to_string)));
    }

    /// 호스트가 가져가는 무효화 범위(1회성) — 상세 패널의 테이블 코멘트 캐시를 같은 범위로 버리게.
    pub(crate) fn take_detail_invalidations(&mut self) -> Vec<(Option<String>, Option<String>)> {
        std::mem::take(&mut self.detail_invalidated)
    }

    /// 노드 기준 무효화(수동 새로 고침) — 루트 = 전부 · 스키마/폴더 = 그 스키마 · 객체/하위/컬럼 = 그 객체.
    fn invalidate_details_at(&mut self, i: usize) {
        let mut cur = Some(i);
        while let Some(n) = cur {
            match &self.nodes[n].kind {
                NodeKind::Root => {
                    self.invalidate_details(None, None);
                    return;
                }
                NodeKind::Schema(sc) | NodeKind::Folder { schema: sc, .. } => {
                    let sc = sc.clone();
                    self.invalidate_details(Some(&sc), None);
                    return;
                }
                NodeKind::Object(o) => {
                    let (sc, nm) = (o.schema.clone(), o.name.clone());
                    self.invalidate_details(Some(&sc), Some(&nm));
                    return;
                }
                NodeKind::Sub { owner, .. } => {
                    let (sc, nm) = (owner.schema.clone(), owner.name.clone());
                    self.invalidate_details(Some(&sc), Some(&nm));
                    return;
                }
                _ => cur = self.parent_of(n),
            }
        }
    }

    /// 상세 캐시 회수(85 §4 · 유휴 30초 틱): TTL 지난 미사용 항목 제거 · 개수 상한(`meta.detail_max`)은 오래된 것부터.
    fn reclaim_details(&mut self) -> usize {
        let ttl = Duration::from_secs(self.index_cfg.detail_ttl_secs.max(1));
        let before = self.detail_cache.len();
        self.detail_cache.retain(|_, e| e.used.elapsed() <= ttl);
        let max = self.index_cfg.detail_max.max(1);
        if self.detail_cache.len() > max {
            let mut by_age: Vec<(Instant, String)> = self
                .detail_cache
                .iter()
                .map(|(k, e)| (e.used, k.clone()))
                .collect();
            by_age.sort();
            for (_, k) in by_age.into_iter().take(self.detail_cache.len() - max) {
                self.detail_cache.remove(&k);
            }
        }
        before - self.detail_cache.len()
    }

    /// Generate SQL 요청(새로고침도 같은 길) — 메타 세션에서 만든 뒤 `ExplorerAction::Preview`.
    pub(crate) fn gen_sql(&mut self, spec: GenSpec) {
        if self.offline {
            self.actions
                .push(ExplorerAction::Status(t(Msg::ExpNotConnected).to_string()));
            return;
        }
        self.last_used = Instant::now();
        self.suspended = false;
        self.actions.push(ExplorerAction::Status(tf(
            Msg::StGenerating,
            &[&spec.title()],
        )));
        let _ = self.tx.send(Req::GenSql {
            gen: self.gen,
            spec,
        });
    }

    /// 소스 열기(스펙) — 패키지 본문은 `open_body`(같은 길 · 종류만 `PackageBody`).
    fn open_source(&mut self, o: &ObjectInfo) {
        self.open_source_kind(o, o.kind, format!("{}.sql", o.name));
    }

    /// ★ 패키지 **본문** 열기(사용자 09-26 "Body는 어떻게 열고 수정하나") — `CREATE OR REPLACE PACKAGE BODY …`를 새 탭에 · 고쳐서 실행하면 컴파일.
    fn open_body(&mut self, o: &ObjectInfo) {
        self.open_source_kind(o, ObjectKind::PackageBody, format!("{}.body.sql", o.name));
    }

    fn open_source_kind(&mut self, o: &ObjectInfo, kind: ObjectKind, title: String) {
        if self.offline {
            self.actions
                .push(ExplorerAction::Status(t(Msg::ExpNotConnected).to_string()));
            return;
        }
        self.last_used = Instant::now();
        self.suspended = false;
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
            kind,
            name,
            title,
        });
    }

    fn visible_rows(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            if self.filter_keep.as_ref().is_some_and(|k| !k.contains(&i)) {
                continue;
            }
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

    /// ★ 검색창 필터(docs/28 §7 · 사용자 09-25 "노드 자체 일치 · 자식 일치 두 조건으로 미일치와 그 부모를 모두 제거 · 최소 레벨은
    /// 유지"): 판정기를 보관하고 거른다(비면 해제). 남는 노드 = ① 라벨 일치 ② 일치 자손이 있음(조상) ③ 일치 노드의 자손(펼치면 그
    /// 안은 전부) — 그 밖(미일치 · 아직 안 읽은 폴더 포함)은 전부 숨긴다. **최소 표시 레벨 = 루트(연결 행)**(서버 헤더는
    /// `ExplorerSet`이 늘 그린다 · 일치 0이면 헤더에 "일치 0"). 일치 자손을 품은 읽어 둔 노드는 펼친다.
    pub(crate) fn apply_filter(&mut self, m: Option<crate::filterbar::Matcher>) {
        let t0 = Instant::now();
        self.filter = m.filter(|m| !m.is_empty());
        self.filter_rev += 1;
        if self.filter.is_some() {
            // ★ 84 §1 세 단계: ① 선별(인덱스 · 스키마 순차) → ② 일치를 부분 노드로 즉시 → ③ 그 폴더들을 순차 완성.
            //   판정은 스레드가(85 §6 · `Req::Search` → `Resp::Hits`) — UI는 일치만 받아 노드로 올린다.
            self.ensure_index();
            self.send_search();
        } else {
            // 검색이 끝나면 완성 큐·인덱스 읽기는 멈춘다(미사용 즉시 회수 · 진행 중인 하나는 응답을 그대로 받는다).
            self.complete_q.clear();
            self.index_q.clear();
            let _ = self.tx_bg.send(Req::SearchStop { gen: self.gen });
        }
        let t1 = Instant::now();
        self.refilter();
        let t2 = Instant::now();
        self.pump_complete();
        let ms = t0.elapsed().as_millis();
        if ms >= 20 {
            eprintln!(
                "[explorer] apply_filter {ms} ms (prepare {} · refilter {} · nodes {})",
                (t1 - t0).as_millis(),
                (t2 - t1).as_millis(),
                self.nodes.len()
            );
        }
    }

    pub(crate) fn set_index_cfg(&mut self, c: IndexCfg) {
        if self.index_cfg != c {
            self.index_cfg = c;
            self.index_invalidate();
        }
    }

    /// ★ 뒤에서 할 일이 남았는가(L1 스키마 남음 · L2 컬럼 큐 · 검색 완성) — 호스트가 빠른 타이머를 유지해 `prefetch_step`이 돌게(85 §2 · 09-25 결함:
    /// 유휴면 틱이 멈춰 L1이 진행되지 않았다).
    pub(crate) fn background_pending(&self) -> bool {
        if self.offline || self.dialect.is_none() || self.nodes[0].state != LoadState::Loaded {
            return false;
        }
        let c = self.index_cfg;
        let l1 = c.on
            && c.prefetch
            && (self.index_inflight.is_some()
                || self
                    .schema_names()
                    .iter()
                    .any(|s| !self.index_done.contains(s)));
        l1 || !self.warm_q.is_empty()
            || self.warm_inflight.is_some()
            || !self.comment_q.is_empty()
            || self.comment_inflight.is_some()
            || self.search_busy()
    }

    /// 진단 한 줄(기동 명령 `explorer.stat` · 09-25): 스키마 수 · L1 완료 수 · 진행 중 · 큐 · 완성 · 일치 · 검색 중.
    pub(crate) fn stat_line(&self) -> String {
        format!(
            "schemas={} l1_done={} inflight={:?} index_q={} completing={:?} complete_q={} hits={} busy={} warm_q={} warm_inflight={:?} offline={} root={:?} filter={} details={}",
            self.schema_names().len(),
            self.index_done.len(),
            self.index_inflight,
            self.index_q.len(),
            self.completing,
            self.complete_q.len(),
            self.filter_hits,
            self.search_busy(),
            self.warm_q.len(),
            self.warm_inflight,
            self.offline,
            self.nodes[0].state,
            self.filter.is_some(),
            self.detail_cache.len()
        )
    }

    /// 검색이 아직 진행 중인가(인덱스 읽기·완성 큐 중 하나라도) — 필터 틀의 진행 표시용(84 §8).
    pub(crate) fn search_busy(&self) -> bool {
        self.filter.is_some()
            && !self.offline
            && (self.index_inflight.is_some()
                || !self.index_q.is_empty()
                || self.completing.is_some()
                || !self.complete_q.is_empty())
    }

    /// 인덱스가 상한에서 잘렸는가(헤더 안내용).
    pub(crate) fn index_truncated(&self) -> bool {
        self.index_truncated
    }

    /// 인덱스 진행(읽은 스키마, 전체) — 헤더 "인덱싱 n/N"용 · 검색 중이 아니거나 다 읽었으면 None.
    pub(crate) fn index_progress(&self) -> Option<(usize, usize)> {
        if self.filter.is_none() || !self.index_cfg.on || self.offline {
            return None;
        }
        let total = self.schema_names().len();
        let done = self.index_done.len().min(total);
        (done < total).then_some((done, total))
    }

    /// ★ 유휴 선적재 한 걸음(84 §7 · 호스트가 주기적으로 부른다): 검색이 없고 · 인덱스가 켜져 있고 · 온라인이며 · 마지막 응답 뒤 `idle_ms`가 지났으면
    /// 아직 없는 스키마 하나를 **백그라운드 메타 세션**으로 읽는다(사용자 클릭 세션을 막지 않는다). 다 읽으면 트래픽 0.
    pub(crate) fn prefetch_step(&mut self, now: Instant) -> bool {
        // 감시(09-25 "검색이 끝나지 않는다"): 검색 중이면 큐를 매 틱 한 번 더 민다 — 응답 경로가 어떤 이유로 끊겨도 멈추지 않게(비용 0).
        if self.filter.is_some() {
            self.pump_index();
            self.pump_complete();
        }
        let l1 = self.l1_step(now);
        let l2 = self.warm_step(now);
        l1 || l2
    }

    /// L1 완성 여부(스키마 전부 들어갔는가).
    fn l1_complete(&self) -> bool {
        let names = self.schema_names();
        !names.is_empty() && names.iter().all(|s| self.index_done.contains(s))
    }

    /// ★ L2 워머 준비(85 §3): L1이 끝나면 현재 스키마의 관계 목록(상태 포함)을 승격하고 사전을 읽는다 — 컬럼 큐는 그 응답에서 채운다.
    fn arm_warm(&mut self) {
        if self.offline || !self.preload {
            return;
        }
        if let Some(cur) = self.server_schema.clone() {
            self.request_objects(&cur);
        }
        self.request_objects(nsql_catalog::DICT_SCHEMA);
    }

    /// 현재 스키마 `kind` 관계 가운데 컬럼이 없는 것을 큐에(상한 `warm_columns_max` · 이름순 = 목록 순).
    fn enqueue_warm_columns(&mut self, schema: &str, kind: ObjectKind) {
        let cap = self.index_cfg.warm_columns_max;
        if cap == 0 {
            return;
        }
        let snap = self.meta.snapshot();
        let Some(sc) = self.meta.names.find(schema) else {
            return;
        };
        for h in snap.prefix(&self.meta.names, sc, kind, "", usize::MAX) {
            if self.warm_sent + self.warm_q.len() >= cap {
                break;
            }
            if matches!(snap.columns(h.id), nsql_run::meta::ColState::Unknown)
                && !self.warm_q.contains(&h.id)
            {
                self.warm_q.push_back(h.id);
            }
        }
    }

    /// 스키마 코멘트를 큐에(이미 읽었거나 대기 중이면 0 · `meta.warm_comments`).
    fn enqueue_schema_comments(&mut self, schema: &str) {
        if !self.index_cfg.warm_comments
            || self.meta.has_schema_comments(schema)
            || self.comment_inflight.as_deref() == Some(schema)
            || self.comment_q.iter().any(|s| s == schema)
        {
            return;
        }
        self.comment_q.push_back(schema.to_string());
    }

    /// 스키마 코멘트 한 걸음(86 §5): 진행 중 없고 간격이 지났으면 큐 앞 스키마를 백그라운드 세션으로.
    fn comment_step(&mut self, now: Instant) -> bool {
        if self.offline || self.comment_inflight.is_some() || self.comment_q.is_empty() {
            return false;
        }
        if self.warm_last.is_some_and(|t| {
            now.duration_since(t).as_millis() < u128::from(self.index_cfg.warm_idle_ms)
        }) {
            return false;
        }
        while let Some(sc) = self.comment_q.pop_front() {
            if self.meta.has_schema_comments(&sc) {
                continue;
            }
            self.comment_inflight = Some(sc.clone());
            let _ = self.tx_bg.send(Req::SchemaComments {
                gen: self.gen,
                schema: sc,
            });
            return true;
        }
        false
    }

    /// 객체(관계)의 코멘트 그대로 — 메타에 스키마 코멘트가 있을 때만(86 §5 · 왕복 0).
    pub(crate) fn comments_of(&self, owner: &ObjectInfo) -> Option<nsql_run::meta::CommentsOf> {
        self.meta.comments_of(&owner.schema, &owner.name)
    }

    /// L2 한 걸음: 간격이 지났고 진행 중이 없으면 큐 앞의 컬럼 하나를 백그라운드 세션으로.
    fn warm_step(&mut self, now: Instant) -> bool {
        if self.comment_step(now) {
            return true;
        }
        if self.offline || self.warm_inflight.is_some() || self.warm_q.is_empty() {
            return false;
        }
        if self.warm_last.is_some_and(|t| {
            now.duration_since(t).as_millis() < u128::from(self.index_cfg.warm_idle_ms)
        }) {
            return false;
        }
        while let Some(id) = self.warm_q.pop_front() {
            if !matches!(
                self.meta.snapshot().columns(id),
                nsql_run::meta::ColState::Unknown
            ) {
                continue;
            }
            self.warm_inflight = Some(id);
            self.warm_sent += 1;
            self.request_columns_for(id, false);
            return true;
        }
        false
    }

    /// ★ L3 회수(85 §4 · 호스트 유휴 틱): 상세 TTL/상한 · 현재 스키마 밖 컬럼 TTL. 반환 = (상세, 컬럼) 비운 수.
    pub(crate) fn reclaim_meta(&mut self) -> (usize, usize) {
        let c = self.index_cfg;
        let keep = self
            .server_schema
            .as_deref()
            .and_then(|s| self.meta.names.find(s));
        let _ = self.reclaim_details();
        self.meta.reclaim(
            Self::now_secs(),
            keep,
            c.detail_max,
            c.detail_ttl_secs,
            c.cols_ttl_secs,
        )
    }

    /// L1 한 걸음(85 §2): 접속 직후부터 스키마 하나씩(간격 `idle_ms`) — 검색 중이면 `pump_index`가 대신 돈다.
    fn l1_step(&mut self, now: Instant) -> bool {
        let c = self.index_cfg;
        if !c.on
            || !c.prefetch
            || self.offline
            || self.dialect.is_none()
            || self.filter.is_some()
            || self.index_inflight.is_some()
            || self.nodes[0].state != LoadState::Loaded
        {
            return false;
        }
        if self
            .index_last_at
            .is_some_and(|t| now.duration_since(t).as_millis() < u128::from(c.idle_ms))
        {
            return false;
        }
        let Some(next) = self
            .schema_names()
            .into_iter()
            .find(|s| !self.index_done.contains(s))
        else {
            return false;
        };
        let _ = self.tx_bg.send(Req::Index {
            gen: self.gen,
            schema: next.clone(),
            max: c.max,
        });
        self.index_inflight = Some(next);
        true
    }

    /// ★ 디스크 캐시 → L1(85 §9): 파일의 항목 가운데 지금 스키마 목록에 있는 것만 `Names`로 심고 스레드 사본에도 준다.
    fn seed_from_cache(&mut self) {
        let Some(path) = self.cache_path.clone() else {
            return;
        };
        let Some(d) = self.dialect else { return };
        let Some((_, _, entries)) = crate::metacache::load(&path) else {
            return;
        };
        let live: HashSet<String> = self.schema_names().into_iter().collect();
        let mut by_schema: HashMap<String, Vec<(ObjectKind, String)>> = HashMap::new();
        for (schema, kind, name) in entries {
            if live.contains(&schema) {
                by_schema.entry(schema).or_default().push((kind, name));
            }
        }
        let at = Self::now_secs();
        for (schema, names) in by_schema {
            self.meta
                .load_names(&schema, nsql_catalog::kinds_for(d), &names, at);
            let list: Vec<nsql_catalog::NameEntry> = names
                .into_iter()
                .map(|(kind, name)| nsql_catalog::NameEntry {
                    schema: schema.clone(),
                    kind,
                    name,
                })
                .collect();
            let _ = self.tx_bg.send(Req::NamesSeed {
                gen: self.gen,
                schema,
                list,
            });
        }
    }

    /// ★ L1 → 디스크 캐시(85 §9): 스키마 전부 들어갔을 때 한 번 — 파일 쓰기는 별 스레드(UI 프레임을 막지 않게).
    fn save_cache(&mut self) {
        if self.cache_saved {
            return;
        }
        let Some(path) = self.cache_path.clone() else {
            return;
        };
        let Some(d) = self.dialect else { return };
        let snap = self.meta.snapshot();
        let schemas = self.schema_names();
        let mut entries: Vec<crate::metacache::Entry> = Vec::new();
        for sname in &schemas {
            let Some(sc) = self.meta.names.find(sname) else {
                continue;
            };
            for &kind in nsql_catalog::kinds_for(d) {
                if !snap.coverage(sc, kind).has_list() {
                    continue;
                }
                for h in snap.prefix(&self.meta.names, sc, kind, "", usize::MAX) {
                    entries.push((sname.clone(), kind, self.meta.names.get(h.name).to_string()));
                }
            }
        }
        self.cache_saved = true;
        let stamp = Self::now_secs();
        let _ = std::thread::Builder::new()
            .name("nsql-metacache".into())
            .spawn(move || crate::metacache::store(&path, stamp, &schemas, &entries));
    }

    fn index_reset(&mut self) {
        self.index_done.clear();
        self.index_q.clear();
        self.index_inflight = None;
        self.index_truncated = false;
    }

    /// 인덱스 무효화(스키마 목록·메타 갱신·설정 변경 뒤) — 검색 중이면 바로 다시 채우기 시작하고 **검색 요청도 다시 보낸다**
    /// (09-25 결함: 검색어가 있는 채로 새 서버에 접속하면 `Prepare`가 스레드의 검색어를 지워 인덱스만 돌고 일치가 오지 않았다).
    fn index_invalidate(&mut self) {
        self.index_reset();
        if self.filter.is_some() {
            self.ensure_index();
            self.send_search();
        }
    }

    /// 지금 검색어를 스레드에(세대·검색어 세대 포함) — 응답은 `Resp::Hits{rev}`로 짝을 맞춘다.
    fn send_search(&mut self) {
        let Some(m) = self.filter.clone() else { return };
        let _ = self.tx_bg.send(Req::Search {
            gen: self.gen,
            rev: self.filter_rev,
            matcher: m,
            limit: self.index_cfg.hits_max,
        });
    }

    /// 트리의 스키마 이름(현재 스키마 먼저 · 그다음 트리 순서).
    fn schema_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.nodes[0]
            .children
            .iter()
            .filter_map(|&c| match &self.nodes[c].kind {
                NodeKind::Schema(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        if let Some(cur) = &self.server_schema {
            if let Some(p) = v.iter().position(|s| s.eq_ignore_ascii_case(cur)) {
                let c = v.remove(p);
                v.insert(0, c);
            }
        }
        v
    }

    /// ① 선별 준비: 아직 안 읽은 스키마를 큐에 넣고(현재 스키마 먼저) 하나를 보낸다.
    fn ensure_index(&mut self) {
        if !self.index_cfg.on
            || self.offline
            || self.dialect.is_none()
            || self.nodes[0].state != LoadState::Loaded
        {
            return;
        }
        if self.index_q.is_empty() {
            let names = self.schema_names();
            self.index_q = names
                .into_iter()
                .filter(|s| {
                    !self.index_done.contains(s) && self.index_inflight.as_deref() != Some(s)
                })
                .collect();
        }
        self.pump_index();
    }

    fn pump_index(&mut self) {
        if self.index_inflight.is_some() || self.offline {
            return;
        }
        while let Some(s) = self.index_q.pop_front() {
            if self.index_done.contains(&s) {
                continue;
            }
            self.last_used = Instant::now();
            self.suspended = false;
            // 인덱스는 늘 백그라운드 세션(이름 사본이 한 스레드에 모이게 · 사용자 클릭 세션을 막지 않게).
            let _ = self.tx_bg.send(Req::Index {
                gen: self.gen,
                schema: s.clone(),
                max: self.index_cfg.max,
            });
            self.index_inflight = Some(s);
            return;
        }
    }

    /// 스키마 노드의 종류 폴더를 만든다(서버 왕복 0 · 펼침 상태는 그대로).
    fn make_folders(&mut self, i: usize) {
        let NodeKind::Schema(schema) = self.nodes[i].kind.clone() else {
            return;
        };
        let Some(d) = self.dialect else { return };
        let was = self.nodes[i].expanded;
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
        self.nodes[i].expanded = was;
    }

    /// ② 일치를 트리로: 인덱스에서 필터에 맞는 항목을 그 (스키마, 종류) 폴더의 **부분** 자식으로 올린다(이미 전체가 읽힌/읽는 중인 폴더는 건너뜀 ·
    /// 상한 `hits_max`) · 올라간 폴더는 완성 큐에.
    fn materialize_hits(&mut self, hits: &[(String, ObjectKind, String)]) -> usize {
        if self.filter.is_none() {
            return 0;
        }
        let Some(d) = self.dialect else { return 0 };
        if hits.is_empty() {
            return 0;
        }
        let mut added = 0usize;
        let schema_node: HashMap<String, usize> = self.nodes[0]
            .children
            .iter()
            .filter_map(|&c| match &self.nodes[c].kind {
                NodeKind::Schema(s) => Some((s.clone(), c)),
                _ => None,
            })
            .collect();
        let picked: Vec<(String, ObjectKind, String)> = hits.to_vec();
        if picked.is_empty() {
            return 0;
        }
        let mut names: HashMap<usize, HashSet<String>> = HashMap::new();
        self.batching = true;
        for (schema, kind, name) in picked {
            let Some(&sn) = schema_node.get(&schema) else {
                continue;
            };
            if self.nodes[sn].children.is_empty() {
                self.make_folders(sn);
            }
            let Some(fi) = self.nodes[sn].children.iter().copied().find(
                |&c| matches!(&self.nodes[c].kind, NodeKind::Folder { kind: k, .. } if *k == kind),
            ) else {
                continue;
            };
            if !matches!(self.nodes[fi].state, LoadState::Idle | LoadState::Partial) {
                continue;
            }
            let set = names.entry(fi).or_insert_with(|| {
                self.nodes[fi]
                    .children
                    .iter()
                    .filter_map(|&c| match &self.nodes[c].kind {
                        NodeKind::Object(o) => Some(o.name.clone()),
                        _ => None,
                    })
                    .collect()
            });
            if !set.insert(name.clone()) {
                continue;
            }
            let depth = self.nodes[fi].depth + 1;
            let o = ObjectInfo {
                schema: schema.clone(),
                name,
                kind,
                status: String::new(),
                modified: String::new(),
                extra: String::new(),
            };
            self.nodes.push(Node {
                expandable: !nsql_catalog::sub_kinds(d, kind).is_empty(),
                kind: NodeKind::Object(o),
                depth,
                children: Vec::new(),
                expanded: false,
                state: LoadState::Idle,
            });
            let id = self.nodes.len() - 1;
            self.nodes[fi].children.push(id);
            added += 1;
            if self.nodes[fi].state == LoadState::Idle {
                self.nodes[fi].state = LoadState::Partial;
            }
            if !self.complete_q.contains(&fi) && self.completing != Some(fi) {
                self.complete_q.push_back(fi);
            }
        }
        self.batching = false;
        // 부분 폴더의 자식은 이름순(전체 목록이 오면 그 순서로 바뀐다).
        for &fi in names.keys() {
            let mut kids = std::mem::take(&mut self.nodes[fi].children);
            kids.sort_by(|&a, &b| {
                let na = match &self.nodes[a].kind {
                    NodeKind::Object(o) => o.name.as_str(),
                    _ => "",
                };
                let nb = match &self.nodes[b].kind {
                    NodeKind::Object(o) => o.name.as_str(),
                    _ => "",
                };
                na.cmp(nb)
            });
            self.nodes[fi].children = kids;
        }
        self.clamp_scroll();
        added
    }

    /// ③ 완성: 큐의 부분 폴더를 하나씩(보이는 순서 = 넣은 순서) 전체 목록으로 — 검색 중일 때만.
    fn pump_complete(&mut self) {
        if self.completing.is_some() || self.filter.is_none() || self.offline {
            return;
        }
        while let Some(f) = self.complete_q.pop_front() {
            if self
                .nodes
                .get(f)
                .is_none_or(|n| n.state != LoadState::Partial)
            {
                continue;
            }
            self.request_complete(f);
            return;
        }
    }

    /// 부분 폴더 하나의 전체 목록을 조용히(있는 노드 유지 · 디프) 요청한다.
    fn request_complete(&mut self, f: usize) {
        let NodeKind::Folder { schema, kind } = self.nodes[f].kind.clone() else {
            return;
        };
        if self.soft.contains(&f) {
            return;
        }
        self.last_used = Instant::now();
        self.suspended = false;
        self.soft.insert(f);
        let _ = self.tx.send(Req::Objects {
            gen: self.gen,
            node: f,
            schema,
            kind,
        });
        if self.completing.is_none() {
            self.completing = Some(f);
        }
    }

    /// 보관한 판정기로 다시 거른다 — 자식이 생기거나 빠질 때(`set_children`·`diff_children`·`detach`)마다.
    fn refilter(&mut self) {
        let Some(m) = self.filter.clone() else {
            // 필터 해제: 필터가 대신 펼쳤던 노드는 다시 접는다(사용자가 그 사이 직접 누른 것은 `toggle`에서 빠졌다).
            for i in std::mem::take(&mut self.filter_expanded) {
                if let Some(n) = self.nodes.get_mut(i) {
                    n.expanded = false;
                }
            }
            if self.filter_keep.take().is_some() {
                self.rows_cache = self.screen_rows();
                self.clamp_scroll();
            }
            self.filter_hits = 0;
            self.filter_tokens.clear();
            return;
        };
        self.filter_tokens = m
            .text()
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .collect();
        let n = self.nodes.len();
        let mut strong = vec![false; n];
        let mut hit = vec![false; n];
        // 부모 표를 한 번에(종전 `parent_of` = 노드마다 전체 스캔 = O(n²) · 09-25 실측 2,230 노드 67 ms → ms).
        let mut parents: Vec<Option<usize>> = vec![None; n];
        for (p, node) in self.nodes.iter().enumerate() {
            for &c in &node.children {
                if let Some(slot) = parents.get_mut(c) {
                    *slot = Some(p);
                }
            }
        }
        // 자식이 부모보다 뒤에 만들어진다(인덱스 증가) → 뒤에서 앞으로 한 번에.
        for i in (0..n).rev() {
            let node = &self.nodes[i];
            let attached = i == 0 || parents[i].is_some();
            if !attached {
                continue;
            }
            // 검색 대상 = 객체·컬럼·잎·스키마 이름(폴더·하위 폴더 라벨은 대상이 아니다 — "Tables"가 "b"에 걸리지 않게).
            let searchable = !matches!(
                node.kind,
                NodeKind::Root | NodeKind::Folder { .. } | NodeKind::Sub { .. }
            );
            let direct = searchable && {
                if self.lower_labels.len() <= i {
                    self.lower_labels.resize(i + 1, None);
                }
                if self.lower_labels[i].is_none() {
                    // 이름은 종류에서 바로(라벨 조립 = 할당 여럿 · 09-25 실측 39 µs/노드) · 그 밖은 라벨.
                    let name: &str = match &self.nodes[i].kind {
                        NodeKind::Schema(s) => s.as_str(),
                        NodeKind::Object(o) => o.name.as_str(),
                        NodeKind::Column(c) => c.name.as_str(),
                        NodeKind::Item(it) => it.name.as_str(),
                        _ => "",
                    };
                    let lower = if name.is_empty() {
                        self.label(i).0.to_lowercase()
                    } else {
                        name.to_lowercase()
                    };
                    self.lower_labels[i] = Some(lower.into_boxed_str());
                }
                let lower = self.lower_labels[i].clone().unwrap_or_default();
                m.matches_cached(&lower, || self.label(i).0)
            };
            let child_strong = node.children.iter().any(|&c| strong[c]);
            hit[i] = direct;
            strong[i] = direct || child_strong;
        }
        // 앞에서 뒤로: 일치 노드의 자손은 전부(`under`) · 루트는 늘(최소 표시 레벨).
        let mut under = vec![false; n];
        let mut keep = vec![false; n];
        for i in 0..n {
            let parent = if i == 0 { None } else { parents[i] };
            if i != 0 && parent.is_none() {
                continue;
            }
            under[i] = parent.is_some_and(|p| hit[p] || under[p]);
            keep[i] = i == 0 || strong[i] || under[i];
        }
        for i in 0..n {
            if strong[i]
                && matches!(self.nodes[i].state, LoadState::Loaded | LoadState::Partial)
                && self.nodes[i].children.iter().any(|&c| strong[c])
                && !self.nodes[i].expanded
            {
                self.nodes[i].expanded = true;
                self.filter_expanded.insert(i);
            }
        }
        self.filter_hits = hit.iter().filter(|h| **h).count();
        self.filter_keep = Some((0..n).filter(|&i| keep[i]).collect());
        self.rows_cache = self.screen_rows();
        self.clamp_scroll();
    }

    pub(crate) fn filter_hits(&self) -> usize {
        self.filter_hits
    }

    /// 다음(또는 이전) 직접 일치 행으로 선택을 옮긴다(보이는 순서 · 끝이면 처음부터) — 검색창 Enter.
    pub(crate) fn select_next_hit(&mut self, forward: bool) -> bool {
        if self.filter_tokens.is_empty() {
            return false;
        }
        let rows = self.visible_rows();
        let hits: Vec<usize> = rows
            .iter()
            .copied()
            .filter(|&i| {
                !matches!(
                    self.nodes[i].kind,
                    NodeKind::Root | NodeKind::Folder { .. } | NodeKind::Sub { .. }
                ) && self.label_hit(i).is_some()
            })
            .collect();
        if hits.is_empty() {
            return false;
        }
        let cur = self
            .selected
            .and_then(|s| hits.iter().position(|&h| h == s));
        let next = match (cur, forward) {
            (Some(c), true) => (c + 1) % hits.len(),
            (Some(c), false) => (c + hits.len() - 1) % hits.len(),
            (None, true) => 0,
            (None, false) => hits.len() - 1,
        };
        let i = hits[next];
        self.selected = Some(i);
        self.caret = Some(i);
        self.ensure_visible(i);
        true
    }

    /// 라벨 안 첫 질의 낱말의 (바이트 시작, 바이트 끝) — 강조 그리기·일치 판정(대소문자 무시 · 한글 자모는 부품 판정과 달라 안 그린다).
    fn label_hit(&self, i: usize) -> Option<(usize, usize)> {
        let label = self.label(i).0;
        let low = label.to_lowercase();
        if low.len() != label.len() {
            return self
                .filter_tokens
                .iter()
                .any(|t| low.contains(t))
                .then_some((0, 0));
        }
        for t in &self.filter_tokens {
            if let Some(b) = low.find(t.as_str()) {
                return Some((b, b + t.len()));
            }
        }
        None
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
        if !self.visible_rect().contains(p) {
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
        self.now_hint = now_ms;
        let a = self.bars.tick(now_ms) | self.typeahead.tick(now_ms);
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
        // 새 객체 강조 — 시간이 지나면 걷는다(남아 있는 동안은 서서히 옅어지므로 계속 그린다).
        let had = !self.fresh.is_empty();
        self.fresh.retain(|(_, until)| *until > now_ms);
        a || b || c || had
    }

    fn is_loading(&self) -> bool {
        self.nodes.iter().any(|n| n.state == LoadState::Loading)
    }

    /// 호스트의 빠른 타이머(≈30ms)를 유지해야 하는가 — 스크롤바 · 호버 페이드 · 로딩 애니메이션.
    pub(crate) fn bars_visible(&self) -> bool {
        self.visible
            && (self.bars.is_visible()
                || self.hover_fade.is_animating()
                || self.is_loading()
                || !self.fresh.is_empty())
    }

    /// 이벤트 — 다시 그려야 하면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        // 열린 우클릭 메뉴가 먼저(바깥 클릭은 닫고 통과 · CLAUDE.md §3).
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
                    // 빈 곳(행 아래) 클릭 = 선택 해제 · 캐럿(테두리)만(사용자 09-23).
                    if self.bounds.contains(p) && self.selected.is_some() {
                        self.caret = self.selected;
                        self.selected = None;
                        return true;
                    }
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
                let glyph_x = self.bounds.x - self.scroll_x
                    + (((self.nodes[i].depth + self.base_depth()) as f32 * INDENT + 4.0)
                        * self.scale)
                        .round() as i32;
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
                let gen_menu = |whats: Vec<nsql_catalog::GenWhat>| {
                    // DDL 앞 구분자(사용자 09-25) — DML/CALL 무리와 DDL을 나눈다(DDL만 있으면 구분자 없음).
                    let mut kids = Vec::new();
                    for w in &whats {
                        if *w == nsql_catalog::GenWhat::Ddl && !kids.is_empty() {
                            kids.push(CtxItem::Separator);
                        }
                        kids.push(CtxItem::item(format!("gen:{}", w.code()), w.label()));
                    }
                    CtxItem::submenu("gen", t(Msg::MnGenSql), kids)
                };
                match &self.nodes[i].kind {
                    NodeKind::Object(o) => {
                        if o.kind.is_relation() {
                            items.push(CtxItem::item("select", t(Msg::ExpSelectRows)));
                        }
                        // ★ 파일 → 표 적재(89 §3-3): 표만(뷰·MV는 제외).
                        if o.kind == ObjectKind::Table {
                            items.push(CtxItem::item("import", t(Msg::MnImportData)));
                        }
                        if o.kind.has_source() {
                            items.push(CtxItem::item("source", t(Msg::ExpOpenSource)));
                        }
                        // 패키지 = 스펙과 본문이 따로(D-202 · Package Bodies 폴더 없음) → 본문 열기.
                        if o.kind == ObjectKind::Package {
                            items.push(CtxItem::item("body", t(Msg::ExpOpenBody)));
                        }
                        // ★ Generate SQL(83 §3): 종류별 항목(테이블 = DML 유형별 · 루틴 = CALL · 공통 = DDL).
                        if let Some(d) = self.dialect {
                            let whats = nsql_catalog::gen_whats(d, o.kind, None);
                            if !whats.is_empty() {
                                items.push(gen_menu(whats));
                            }
                        }
                        items.push(CtxItem::item("copy", t(Msg::ExpCopyName)));
                        // ★ 새로 고침은 **계층형**(사용자 09-19): 서버 = 그 서버 전부 · 스키마 = 그 스키마 · 종류 폴더 = 그 종류의
                        //   객체 전부 · 테이블/뷰 = 그 객체의 정보(컬럼)만 · 그 밖의 객체·컬럼 = 자기가 속한 목록.
                        items.push(CtxItem::Separator);
                        items.push(CtxItem::item("refresh", t(Msg::ExpRefresh)));
                    }
                    NodeKind::Column(_) | NodeKind::Item(_) => {
                        // 잎(제약·인덱스·트리거)의 DDL.
                        if let (Some(d), Some((owner_kind, sub))) =
                            (self.dialect, self.item_owner(i))
                        {
                            let whats = nsql_catalog::gen_whats(d, owner_kind, Some(sub));
                            if !whats.is_empty() {
                                items.push(gen_menu(whats));
                            }
                        }
                        items.push(CtxItem::item("copy", t(Msg::ExpCopyName)));
                        items.push(CtxItem::Separator);
                        items.push(CtxItem::item("refresh", t(Msg::ExpRefresh)));
                    }
                    NodeKind::Schema(_) => {
                        items.push(CtxItem::item("copy", t(Msg::ExpCopyName)));
                        items.push(CtxItem::Separator);
                        items.push(CtxItem::item("refresh", t(Msg::ExpRefresh)));
                        items.push(CtxItem::item("refresh_meta", t(Msg::ExpRefreshMeta)));
                    }
                    // 루트 = 연결 항목(DBeaver 항해자와 같은 자리 · docs/54): 연결됨 → 새 탭 · 새로 고침 · 해제 / 오프라인 → 연결 · 제거.
                    NodeKind::Root if self.offline => {
                        items.push(CtxItem::item("connect", t(Msg::ExpConnectServer)));
                        items.push(CtxItem::item("remove", t(Msg::ExpRemoveServer)));
                    }
                    NodeKind::Root => {
                        items.push(CtxItem::item("newtab", t(Msg::ExpNewTabHere)));
                        items.push(CtxItem::item("refresh", t(Msg::ExpRefresh)));
                        items.push(CtxItem::item("refresh_meta", t(Msg::ExpRefreshMeta)));
                        items.push(CtxItem::Separator);
                        items.push(CtxItem::item(
                            "disconnect",
                            t(if self.grouped {
                                Msg::ExpDisconnectConn
                            } else {
                                Msg::ExpDisconnectServer
                            }),
                        ));
                    }
                    _ => items.push(CtxItem::item("refresh", t(Msg::ExpRefresh))),
                }
                let text_w = (160.0 * self.scale).round() as i32;
                self.menu.set_scale(self.scale);
                let host = if self.menu_host.h > 0 {
                    self.menu_host
                } else {
                    self.bounds
                };
                // ★ 누른 자리에 가깝게 · **대상 이름은 가리지 않게**(사용자 09-21): 그 행 바로 아래(자리가 없으면 위)에 연다.
                let row_h = self.row_h();
                let idx = (y - self.bounds.y + self.scroll) / row_h;
                let row = Rect::new(
                    self.bounds.x,
                    self.bounds.y - self.scroll + idx * row_h,
                    self.bounds.w,
                    row_h,
                );
                self.menu.open_beside(x, y, row, items, host, text_w);
                true
            }
            InputEvent::Key { key, .. } if self.focused => {
                let rows: Vec<usize> = self.visible_rows();
                let pos = self
                    .selected
                    .or(self.caret)
                    .and_then(|s| rows.iter().position(|&r| r == s));
                // 타입어헤드 활성이면 ↑/↓ = **접두 매치 안에서만 순환**(역방향 포함) · 순환 중엔 타임아웃 기준을 되돌린다(nexa-beep 규칙).
                let ta = self.typeahead.composing();
                match key {
                    CtlKey::Down if !ta.is_empty() => {
                        self.typeahead.touch(self.now_hint);
                        let n = rows.len().max(1);
                        let from = pos.map_or(0, |p| (p + 1) % n);
                        if let Some(k) =
                            nexa_ctl::typeahead::find_prefix(rows.len(), from, &ta, |k| {
                                self.label(rows[k]).0
                            })
                        {
                            self.selected = Some(rows[k]);
                            self.ensure_visible(rows[k]);
                        }
                    }
                    CtlKey::Up if !ta.is_empty() => {
                        self.typeahead.touch(self.now_hint);
                        let n = rows.len().max(1);
                        let from = pos.map_or(0, |p| (p + n - 1) % n);
                        if let Some(k) =
                            nexa_ctl::typeahead::find_prefix_rev(rows.len(), from, &ta, |k| {
                                self.label(rows[k]).0
                            })
                        {
                            self.selected = Some(rows[k]);
                            self.ensure_visible(rows[k]);
                        }
                    }
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
                    // → = 접힌 노드면 펼치고 · 이미 펼쳐져 있으면 **첫 자식으로**(DBeaver/SWT · Windows 탐색기 · 09-19).
                    CtlKey::Right => {
                        if let Some(i) = self.selected {
                            if self.nodes[i].expandable && !self.nodes[i].expanded {
                                self.toggle(i);
                            } else if let Some(&c) = self.nodes[i].children.first() {
                                if self.nodes[i].expanded {
                                    self.selected = Some(c);
                                    self.ensure_visible(c);
                                }
                            }
                        }
                    }
                    // Home/End = 첫/마지막 행 · PageUp/PageDown = 보이는 행 수만큼(DBeaver 내비게이터 · 09-19).
                    CtlKey::Home => {
                        if let Some(&n) = rows.first() {
                            self.selected = Some(n);
                            self.ensure_visible(n);
                        }
                    }
                    CtlKey::End => {
                        if let Some(&n) = rows.last() {
                            self.selected = Some(n);
                            self.ensure_visible(n);
                        }
                    }
                    CtlKey::PageUp | CtlKey::PageDown => {
                        let page = (self.bounds.h / self.row_h().max(1)).max(1) as usize;
                        let np = match key {
                            CtlKey::PageUp => pos.map_or(0, |p| p.saturating_sub(page)),
                            _ => pos.map_or(0, |p| (p + page).min(rows.len().saturating_sub(1))),
                        };
                        if let Some(&n) = rows.get(np) {
                            self.selected = Some(n);
                            self.ensure_visible(n);
                        }
                    }
                    // ← = 펼쳐진 노드면 접고 · 아니면(잎·접힘) **상위 노드로**(파일 탐색기 관례 · 사용자 09-19).
                    CtlKey::Left => {
                        if let Some(i) = self.selected {
                            if self.nodes[i].expanded && self.nodes[i].expandable {
                                self.toggle(i);
                            } else if let Some(p) = pos.and_then(|p| {
                                let d = self.nodes[i].depth;
                                rows[..p]
                                    .iter()
                                    .rev()
                                    .copied()
                                    .find(|&r| self.nodes[r].depth < d)
                            }) {
                                self.selected = Some(p);
                                self.ensure_visible(p);
                            }
                        }
                    }
                    CtlKey::Enter => {
                        if let Some(i) = self.selected {
                            self.activate(i);
                        }
                    }
                    CtlKey::Escape => {
                        self.typeahead.clear();
                        self.menu.close();
                    }
                    _ => return false,
                }
                true
            }
            // 글자 키(DBeaver/SWT 트리 관례 · 09-19): 타입어헤드가 비어 있을 때 Backspace = 상위 · `*` = 하위 전부 펼침(읽어 둔 것만) ·
            //   `+`/`-` = 펼침/접힘. 그 밖의 글자(또는 접두 입력 중의 모든 글자) = **타입어헤드**(nexa-ctl 부품 · 설정 `explorer.typeahead*`).
            InputEvent::Char { c, .. } if self.focused => self.on_char(c),
            _ => false,
        }
    }

    fn on_char(&mut self, c: char) -> bool {
        let rows = self.visible_rows();
        let Some(i) = self.selected else {
            return false;
        };
        let pos = rows.iter().position(|&r| r == i);
        match c {
            '\u{8}' if self.typeahead.is_active() => {
                if let Some(q) = self.typeahead.backspace(self.now_hint) {
                    if let Some(k) = nexa_ctl::typeahead::find_prefix(
                        rows.len(),
                        pos.unwrap_or(0),
                        &q.prefix,
                        |k| self.label(rows[k]).0,
                    ) {
                        self.selected = Some(rows[k]);
                        self.ensure_visible(rows[k]);
                    }
                }
                true
            }
            '\u{8}' => {
                let d = self.nodes[i].depth;
                if let Some(p) = pos.and_then(|p| {
                    rows[..p]
                        .iter()
                        .rev()
                        .copied()
                        .find(|&r| self.nodes[r].depth < d)
                }) {
                    self.selected = Some(p);
                    self.ensure_visible(p);
                }
                true
            }
            '+' if !self.typeahead.is_active() => {
                if self.nodes[i].expandable && !self.nodes[i].expanded {
                    self.toggle(i);
                }
                true
            }
            '-' if !self.typeahead.is_active() => {
                if self.nodes[i].expanded {
                    self.toggle(i);
                }
                true
            }
            '*' if !self.typeahead.is_active() => {
                self.expand_loaded_subtree(i);
                self.ensure_visible(i);
                true
            }
            c => self.typeahead_char(c, pos, &rows),
        }
    }

    /// 타입어헤드 글자 처리 — 설정 필터를 지난 글자를 버퍼에 넣고(한글은 조합) 접두 매치로 점프.
    /// `include_caret`(접두 확장)이면 지금 행부터, 새 접두면 다음 행부터 순환(nexa-beep 규칙).
    fn typeahead_char(&mut self, c: char, pos: Option<usize>, rows: &[usize]) -> bool {
        if !self.ta_cfg.enabled || !self.ta_cfg.filter.accepts(c) {
            return false;
        }
        let q = self.typeahead.push(c, self.now_hint);
        let n = rows.len();
        if n == 0 {
            return true;
        }
        let from = pos.map_or(0, |p| if q.include_caret { p } else { (p + 1) % n });
        if let Some(k) =
            nexa_ctl::typeahead::find_prefix(n, from, &q.prefix, |k| self.label(rows[k]).0)
        {
            self.selected = Some(rows[k]);
            self.ensure_visible(rows[k]);
        }
        true
    }

    /// 타입어헤드 설정(호스트 · 바뀔 때).
    pub(crate) fn set_typeahead(&mut self, cfg: TypeAheadCfg) {
        self.ta_cfg = cfg;
        self.typeahead.set_timeout(cfg.timeout_ms);
        if !cfg.enabled {
            self.typeahead.clear();
        }
    }

    /// 입력 중인 접두(HUD 표시용 · 비면 없음).
    pub(crate) fn typeahead_text(&self) -> String {
        self.typeahead.composing()
    }

    /// 타입어헤드 HUD 위치(설정).
    pub(crate) fn typeahead_pos(&self) -> nexa_ctl::HudPos {
        self.ta_cfg.pos
    }

    /// 선택 노드와 그 아래 **읽어 둔** 하위를 전부 펼친다(`*` · 아직 안 읽은 폴더는 요청만 나가고 그 아래는 다음 `*`로).
    fn expand_loaded_subtree(&mut self, root: usize) {
        let mut stack = vec![root];
        while let Some(i) = stack.pop() {
            if self.nodes[i].expandable && !self.nodes[i].expanded {
                self.toggle(i);
            }
            if self.nodes[i].state == LoadState::Loaded {
                stack.extend(self.nodes[i].children.iter().copied());
            }
        }
    }

    /// 자체 캡처용(기동 명령 `explorer.menu[:<n번째 보이는 줄>]`): 그 줄에서 **우클릭한 것과 같은 사건**을 컨트롤에 직접 준다
    /// (OS 입력 주입 없음) — 메뉴가 뜨는지 · 항목을 골랐을 때 동작하는지 확인하려고.
    pub(crate) fn capture_menu(&mut self, row: usize) -> bool {
        let step = (4.0 * self.scale).max(1.0) as i32;
        let mut seen: Vec<usize> = Vec::new();
        let mut y = self.bounds.y + step;
        while y < self.bounds.bottom() {
            if let Some((Some(i), _)) = self.row_at(Point {
                x: self.bounds.x + self.bounds.w / 2,
                y,
            }) {
                if !seen.contains(&i) {
                    seen.push(i);
                    if seen.len() == row + 1 {
                        return self.on_event(&InputEvent::RightDown {
                            x: self.bounds.x + self.bounds.w / 3,
                            y,
                        });
                    }
                }
            }
            y += step;
        }
        false
    }

    /// 자체 시험용(기동 명령 `explorer.expand<row>`): `row`번째 보이는 줄을 **펼친다**(이미 펼쳐졌으면 그대로 · 접지 않는다).
    pub(crate) fn capture_expand(&mut self, row: usize) -> bool {
        let rows = self.visible_rows();
        let Some(&i) = rows.get(row) else {
            return false;
        };
        if !self.nodes[i].expandable {
            return false;
        }
        self.selected = Some(i);
        if !self.nodes[i].expanded {
            self.toggle(i);
        }
        true
    }

    /// 자체 시험용(기동 명령 `explorer.dump:<파일>`): 보이는 줄을 `깊이|종류|라벨|부가|상태` 한 줄씩(트리 구조 자동 점검 · 83 §1).
    pub(crate) fn dump_rows(&self) -> String {
        let mut out = String::new();
        for i in self.visible_rows() {
            let n = &self.nodes[i];
            let tag = match &n.kind {
                NodeKind::Root => "root".to_string(),
                NodeKind::Schema(_) => "schema".into(),
                NodeKind::Folder { .. } => "folder".into(),
                NodeKind::Object(o) => {
                    format!("object{}", if o.status == "INVALID" { "!" } else { "" })
                }
                NodeKind::Column(_) => "column".into(),
                NodeKind::Sub { .. } => "sub".into(),
                NodeKind::Item(it) => format!("item:{:?}", it.icon),
            };
            let (label, sub) = self.label(i);
            out.push_str(&format!("{}|{tag}|{label}|{sub}|{:?}\n", n.depth, n.state));
        }
        out
    }

    /// 자체 캡처용(기동 명령 `explorer.pick:<id>`): 열린 메뉴에서 그 항목을 **클릭한 것과 같은 경로**로 고른다.
    pub(crate) fn capture_pick(&mut self, id: &str) -> bool {
        if !self.menu.is_open() {
            return false;
        }
        self.menu.close();
        self.menu_pick(id);
        true
    }

    fn menu_pick(&mut self, id: &str) {
        let Some(i) = self.selected else { return };
        if let Some(code) = id.strip_prefix("gen:") {
            if let Some(what) = nsql_catalog::GenWhat::parse(code) {
                self.gen_pick(i, what);
            }
            return;
        }
        match id {
            "select" | "source" => self.activate(i),
            "body" => {
                if let NodeKind::Object(o) = self.nodes[i].kind.clone() {
                    self.open_body(&o);
                }
            }
            "import" => {
                if let NodeKind::Object(o) = self.nodes[i].kind.clone() {
                    self.actions.push(ExplorerAction::Import {
                        owner: o,
                        server: None,
                    });
                }
            }
            // 읽어 둔 노드 = 디프로 조용히(펼침·선택 보존 · docs/57 T3) · 오류/미로딩 = 새로 읽기.
            "refresh" => {
                self.selected = Some(i);
                self.refresh_selected(false);
            }
            // ★ 메타(완성 캐시)만 새로 고침(79 §4): 루트 = 이 서버 전부 · 스키마 = 그 스키마.
            "refresh_meta" => {
                let scope = match &self.nodes[i].kind {
                    NodeKind::Root => None,
                    _ => self.schema_of(i),
                };
                let (b, o) = self.refresh_meta(scope.as_deref());
                self.actions.push(ExplorerAction::Status(tf(
                    Msg::StIntelRefreshed,
                    &[&b.to_string(), &o.to_string()],
                )));
            }
            "remove" => self.actions.push(ExplorerAction::RemoveServer),
            "disconnect" => self.actions.push(ExplorerAction::DisconnectServer(None)),
            "connect" => self.actions.push(ExplorerAction::ConnectServer(None)),
            "newtab" => self.actions.push(ExplorerAction::NewTabHere(None)),
            "copy" => {
                let name = match &self.nodes[i].kind {
                    NodeKind::Schema(s) => s.clone(),
                    NodeKind::Object(o) => o.name.clone(),
                    NodeKind::Column(c) => c.name.clone(),
                    NodeKind::Item(it) => it.name.clone(),
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
            NodeKind::Root if self.grouped && !self.conn_desc.is_empty() => {
                // 묶음 모드(docs/54 §9): 연결 행 = DB(서비스) · 흐리게 = 계정 · 프로필(· 오프라인).
                let main = if self.conn_db.is_empty() {
                    self.endpoint.clone()
                } else {
                    self.conn_db.clone()
                };
                let mut dim: Vec<String> = Vec::new();
                if !self.users.is_empty() {
                    dim.push(self.users.join(", "));
                } else if !self.conn_user.is_empty() {
                    dim.push(self.conn_user.clone());
                }
                if !self.profile_name.is_empty() && self.profile_name != main {
                    dim.push(self.profile_name.clone());
                }
                if self.offline {
                    dim.push(t(Msg::ExpOffline).to_string());
                }
                (main, dim.join(" · "))
            }
            NodeKind::Root => {
                if self.conn_desc.is_empty() {
                    (t(Msg::ExpNotConnected).to_string(), String::new())
                } else if self.profile_name.is_empty() {
                    // 접속 문자열로 붙은 서버(프로필 이름 없음)도 오프라인이면 그렇게 보인다(종전에는 표시가 없었다).
                    let dim = if self.offline {
                        t(Msg::ExpOffline).to_string()
                    } else {
                        String::new()
                    };
                    (self.endpoint.clone(), dim)
                } else if self.offline {
                    // 오프라인 = 읽어 둔 메타만(이 서버에 붙은 세션 없음).
                    (
                        self.profile_name.clone(),
                        format!("{} · {}", self.endpoint, t(Msg::ExpOffline)),
                    )
                } else {
                    (self.profile_name.clone(), self.endpoint.clone())
                }
            }
            NodeKind::Schema(s) => (s.clone(), String::new()),
            NodeKind::Folder { kind, .. } => {
                let base = folder_label(self.dialect, *kind);
                if matches!(n.state, LoadState::Loaded | LoadState::Partial) {
                    (format!("{base} ({})", self.count_label(n)), String::new())
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
                // 유효성은 아이콘 배지로(83 §2) — `VALID`/`INVALID` 글자는 뗀다 · 그 밖 상태(`DISABLED` …)는 흐린 글자.
                let status = if o.status == "VALID" || o.status == "INVALID" {
                    String::new()
                } else {
                    o.status.clone()
                };
                (format!("{}{extra}", o.name), status)
            }
            NodeKind::Sub { sub, .. } => {
                let base = t(sub_msg(*sub)).to_string();
                if matches!(n.state, LoadState::Loaded | LoadState::Partial) {
                    (format!("{base} ({})", self.count_label(n)), String::new())
                } else {
                    (base, String::new())
                }
            }
            NodeKind::Item(it) => (
                it.name.clone(),
                match (it.detail.is_empty(), it.status.is_empty()) {
                    (true, true) => String::new(),
                    (false, true) => it.detail.clone(),
                    (true, false) => it.status.clone(),
                    (false, false) => format!("{} · {}", it.detail, it.status),
                },
            ),
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
        // 보이는 부분만(이어 붙인 트리에서는 bounds가 영역 밖으로 나간다).
        let vis = self.visible_rect();
        if vis.h <= 0 || vis.w <= 0 {
            self.rows_cache = self.screen_rows();
            return;
        }
        dc.fill_rect(vis, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, vis.y, 1, vis.h), th.border);
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
        let first = ((vis.y - b.y + self.scroll) / row_h.max(1)).max(0) as usize;
        let sx = self.scroll_x;
        let mut content_w = 0;
        for (pos, (node, parent)) in rows.iter().enumerate().skip(first) {
            let y = b.y + pos as i32 * row_h - self.scroll;
            if y >= vis.bottom() {
                break;
            }
            let rr = Rect::new(b.x, y, b.w, row_h).intersection(&vis);
            if rr.h <= 0 {
                continue;
            }
            let ty = dc.text_center_y(y, row_h);
            // 글리프 시각 중심(대문자·한글 몸통) — 셰브론·칩을 여기에 맞춘다(사용자 09-15 "폰트 기준 세로 중앙").
            let vcy = ty + (asc as f32 * 0.62).round() as i32;
            match node {
                None => {
                    // 상태 행: 로딩 중 / 오류(클릭 = 재시도).
                    let depth = self.nodes[*parent].depth + 1 + self.base_depth();
                    let x = b.x - sx + ((depth as f32 * INDENT + 8.0) * s).round() as i32;
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
                    } else if self.selected.is_none() && self.caret == Some(*i) {
                        dc.stroke_round_rect(rr, 0, th.text_dim, 1.0);
                    } else {
                        // 방금 생긴 객체 = 옅은 강조가 서서히 사라진다(선택은 옮기지 않는다 · docs/57 D-110).
                        if let Some((_, until)) = self.fresh.iter().find(|(f, _)| f == i) {
                            let left = until.saturating_sub(self.now_hint) as f32
                                / self.highlight_ms.max(1) as f32;
                            dc.fill_rect_alpha(rr, th.accent, 0.22 * left.clamp(0.0, 1.0));
                        }
                        // 호버 = 전경색을 알파로 덮어 서서히(진행도 × 토큰 알파).
                        let ha = hover_alpha(false, self.hover_fade.value(*i));
                        if ha > 0.0 {
                            dc.fill_rect_alpha(rr, th.text, ha);
                        }
                    }
                    let gx = b.x - sx
                        + (((n.depth + self.base_depth()) as f32 * INDENT + 4.0) * s).round()
                            as i32;
                    // 셰브론(nexa-dir2 파일 그리드와 같은 부품 · 사용자 09-15) — 읽어서 자식이 없으면 그리지 않는다.
                    let empty_loaded = n.state == LoadState::Loaded && n.children.is_empty();
                    // ★ 부분적으로 잘린 행(위로 반쯤 스크롤된 첫 행 · 상태줄에 걸린 마지막 행)도 셰브론·아이콘을 **클립해서** 그린다
                    //   — 종전(09-15)엔 셰브론에 클립이 없어 아예 건너뛰었고, 그래서 반 줄만 올려도 ANONYMOUS의 셰브론·아이콘이
                    //   사라졌다(사용자 09-19 캡처). 이제 `draw_chevron_90_in(.., Some(rr))` · 아이콘은 원래 `rr`로 클립.
                    if n.expandable && !empty_loaded {
                        let cw = th_txt.max(10); // 사용자 09-15: 글꼴 높이의 1.0배
                        let chev = Rect::new(gx, vcy - cw / 2, cw, cw);
                        // 색: 접힘 = 진한 회색 · 마우스 오버 또는 펼침 = 본문색(검정) (사용자 09-15).
                        let color = if n.expanded || self.hover == Some(*i) {
                            th.text
                        } else {
                            th.text_dim
                        };
                        nexa_ctl::controls::draw_chevron_90_in(
                            dc,
                            chev,
                            color,
                            n.expanded,
                            Some(rr),
                        );
                    }
                    let mut x = gx + (16.0 * s).round() as i32;
                    // 아이콘(설정 켬 · DBMS/스키마/폴더/종류별 · 글꼴 높이 크기) 또는 색 칩(끔).
                    if self.icons_on {
                        if let Some((k, rgb)) = self.icon_for(n) {
                            let sz = (ICON_BASE_PX * self.font_px / ICON_REF_FONT_PX * s)
                                .round()
                                .max(8.0) as i32;
                            let dst = Rect::new(x, vcy - sz / 2, sz, sz);
                            let mut adv = sz;
                            if matches!(n.kind, NodeKind::Root) && !self.grouped {
                                // ★ 서버 루트 = DBMS 파일 아이콘(dbms_icons · 사용자 09-22): 틀 색 = 파일의 대표색(fill) ·
                                //   `<text>` 줄들(≤3자 1줄 세로 중앙 · 4~6자 2줄)을 Status 굵게 틀 안에 · 로고 파일(경로만)이면 마스크만.
                                //   루트만 조금 크게(1.35배) 그려 두 줄이 들어간다.
                                adv = self.draw_brand(dc, th, x, vcy, sz, rgb, rr);
                            } else {
                                // ★ 유효성 배지(83 §2 · DBeaver 관례): INVALID = 아이콘 오른쪽 아래 빨간 점(흰 테).
                                let invalid =
                                    matches!(&n.kind, NodeKind::Object(o) if o.status == "INVALID");
                                let img = self.icon_image(k, rgb, sz);
                                dc.image_scaled(dst, &img, rr);
                                {
                                    if invalid {
                                        let r = ((sz as f32) * 0.22).round().max(2.0) as i32;
                                        let cx = dst.x + dst.w - r;
                                        let cy = dst.y + dst.h - r;
                                        let ring =
                                            Rect::new(cx - r - 1, cy - r - 1, 2 * r + 2, 2 * r + 2)
                                                .intersection(&rr);
                                        dc.fill_round_rect(ring, r + 1, th.panel_bg);
                                        let dot = Rect::new(cx - r, cy - r, 2 * r, 2 * r)
                                            .intersection(&rr);
                                        dc.fill_round_rect(dot, r, th.danger);
                                    }
                                }
                            }
                            x += adv + (6.0 * s).round() as i32;
                        }
                    } else if let NodeKind::Object(o) = &n.kind {
                        let cr = Rect::new(x, vcy - chip / 2, chip, chip);
                        let color = if o.status == "INVALID" {
                            th.danger
                        } else {
                            kind_color(o.kind, th)
                        };
                        dc.fill_round_rect(cr, 2, color);
                        x += chip + (6.0 * s).round() as i32;
                    } else if let NodeKind::Column(_) | NodeKind::Item(_) = &n.kind {
                        let cr = Rect::new(x + 2, vcy - chip / 4, chip / 2, chip / 2);
                        dc.fill_round_rect(cr, 2, th.text_dim);
                        x += chip + (6.0 * s).round() as i32;
                    }
                    let (label, sub) = self.label(*i);
                    // 메뉴와 같은 글꼴·크기·굵기(DBeaver 캡처 기준 · 사용자 09-15) — 굵게 없음.
                    dc.select_font(FontSlot::Base, false);
                    // ★ 검색창 일치 강조(docs/28 §7): 첫 질의 낱말 자리에 옅은 강조색 띠.
                    if !self.filter_tokens.is_empty() {
                        if let Some((b0, b1)) = self.label_hit(*i) {
                            if b1 > b0 {
                                let w0 = dc.text_width(&label[..b0]);
                                let w1 = dc.text_width(&label[b0..b1]);
                                let hr = Rect::new(x + w0, ty, w1, th_txt).intersection(&rr);
                                dc.fill_rect_alpha(hr, th.accent, 0.28);
                            }
                        }
                    }
                    dc.text(x, ty, rr, &label, th.text);
                    let lw = dc.text_width(&label);
                    dc.select_font(FontSlot::Base, false);
                    let mut right = x + lw;
                    if !sub.is_empty() {
                        let color = if sub == "INVALID" {
                            th.danger
                        } else {
                            th.text_dim
                        };
                        let sx0 = x + lw + (8.0 * s).round() as i32;
                        dc.text(sx0, ty, rr, &sub, color);
                        right = sx0 + dc.text_width(&sub);
                    }
                    // 내용 폭 = 가장 긴 행의 오른쪽 끝(스크롤 되돌린 값) + 여백.
                    content_w = content_w.max(right + sx - b.x + (12.0 * s).round() as i32);
                    let _ = indent;
                }
            }
        }
        // 보이는 행만 쟀으므로 줄어들 때는 천천히(스크롤 중 폭이 요동치지 않게) · 늘 때는 즉시.
        self.content_w = if content_w >= self.content_w {
            content_w
        } else {
            self.content_w.max(content_w)
        };
        // 이어 붙인 트리(창이 주어짐)에서는 공용 스크롤바를 호스트가 그린다.
        if self.clip.is_none() {
            self.bars
                .paint(dc, th, b, b.w, self.content_h().max(b.h), 0, self.scroll, s);
        }
        // (우클릭 메뉴는 `paint_menu` — 호스트가 모든 서버의 트리를 그린 뒤에 그린다.)
        self.rows_cache = rows;
    }
}

#[cfg(test)]
mod blockers_tests {
    #![allow(clippy::unwrap_used)]
    use super::blockers_sql;
    use nsql_core::Dialect;

    /// docs/56 L3 — 방언별 막힘 질의: 숫자 id만 · 지원 방언만 · id가 문장에 그대로 들어간다.
    #[test]
    fn blockers_sql_per_dialect_and_id_validation() {
        let o = blockers_sql(Dialect::Oracle, "123").unwrap();
        assert!(o.contains("v$session") && o.contains("blocking_session = 123"));
        let p = blockers_sql(Dialect::Postgres, " 4567 ").unwrap();
        assert!(p.contains("pg_blocking_pids") && p.contains("4567 = ANY"));
        assert!(blockers_sql(Dialect::Mssql, "55")
            .unwrap()
            .contains("blocking_session_id = 55"));
        assert!(blockers_sql(Dialect::Mysql, "9")
            .unwrap()
            .contains("blocking_pid = 9"));
        assert!(
            blockers_sql(Dialect::Sqlite, "1").is_none(),
            "SQLite = 해당 없음"
        );
        assert!(blockers_sql(Dialect::Oracle, "").is_none());
        assert!(
            blockers_sql(Dialect::Oracle, "1; DROP TABLE t").is_none(),
            "숫자가 아니면 거부"
        );
    }
}

#[cfg(test)]
mod refresh_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use nsql_core::{DdlKind, DdlTarget, DdlVerb};

    fn node(kind: NodeKind, depth: usize) -> Node {
        Node {
            expandable: !matches!(kind, NodeKind::Column(_) | NodeKind::Item(_)),
            kind,
            depth,
            children: Vec::new(),
            expanded: false,
            state: LoadState::Idle,
        }
    }

    fn obj(name: &str) -> NodeKind {
        NodeKind::Object(ObjectInfo {
            schema: "HR".into(),
            name: name.into(),
            kind: ObjectKind::Table,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        })
    }

    /// 루트 → HR 스키마 → Tables(읽음: A·B·C) + Views(안 읽음) 한 벌.
    /// 객체 노드 아래 하위 폴더 노드(83 §1 · 시험용).
    fn sub_of(ex: &Explorer, obj: usize, sub: SubKind) -> Node {
        let NodeKind::Object(o) = &ex.nodes[obj].kind else {
            panic!("not an object");
        };
        node(
            NodeKind::Sub {
                owner: Box::new(o.clone()),
                sub,
            },
            ex.nodes[obj].depth + 1,
        )
    }

    fn sample() -> (Explorer, usize, usize) {
        let mut ex = Explorer::new(Box::new(|| {}), true);
        ex.dialect = Some(Dialect::Oracle);
        ex.conn_desc = "oracle://hr@host/db".into();
        ex.nodes[0].state = LoadState::Loaded;
        ex.nodes[0].expanded = true;
        ex.set_children(0, vec![node(NodeKind::Schema("HR".into()), 1)]);
        let schema = ex.nodes[0].children[0];
        ex.set_children(
            schema,
            vec![
                node(
                    NodeKind::Folder {
                        schema: "HR".into(),
                        kind: ObjectKind::Table,
                    },
                    2,
                ),
                node(
                    NodeKind::Folder {
                        schema: "HR".into(),
                        kind: ObjectKind::View,
                    },
                    2,
                ),
            ],
        );
        let tables = ex.nodes[schema].children[0];
        ex.set_children(
            tables,
            vec![node(obj("A"), 3), node(obj("B"), 3), node(obj("C"), 3)],
        );
        (ex, schema, tables)
    }

    /// 디프: 남는 노드는 인덱스·펼침·자식 그대로 · 새 노드는 새 목록 자리 + 강조 · 사라진 노드의 선택은 부모로.
    #[test]
    fn diff_keeps_state_adds_and_removes() {
        let (mut ex, _, tables) = sample();
        let (a, b, c) = (
            ex.nodes[tables].children[0],
            ex.nodes[tables].children[1],
            ex.nodes[tables].children[2],
        );
        // B를 펼쳐 컬럼 하나를 읽어 둔 상태 · C를 선택.
        ex.set_children(
            b,
            vec![node(
                NodeKind::Column(ColumnInfo {
                    name: "ID".into(),
                    data_type: "int".into(),
                    nullable: true,
                    position: 1,
                    default: String::new(),
                }),
                4,
            )],
        );
        ex.selected = Some(c);
        ex.now_hint = 1000;
        ex.diff_children(
            tables,
            vec![node(obj("A"), 3), node(obj("AB"), 3), node(obj("B"), 3)],
        );
        let kids = ex.nodes[tables].children.clone();
        assert_eq!(kids.len(), 3);
        assert_eq!((kids[0], kids[2]), (a, b), "남는 노드 = 같은 인덱스");
        assert!(
            ex.nodes[b].expanded && ex.nodes[b].children.len() == 1,
            "펼침·자식 보존"
        );
        assert!(matches!(&ex.nodes[kids[1]].kind, NodeKind::Object(o) if o.name == "AB"));
        assert_eq!(ex.fresh, vec![(kids[1], 3000)], "새 노드만 강조");
        assert_eq!(ex.selected, Some(tables), "사라진 선택 = 부모로");
        // 강조는 시간이 지나면 걷힌다.
        ex.tick(3001);
        assert!(ex.fresh.is_empty());
    }

    /// 검색창 필터(docs/28 §7): 일치 노드 + 조상 + 안 읽은 폴더만 남고, 일치를 품은 읽어 둔 조상은 펼쳐진다 · 빈 글 = 해제 ·
    /// Enter = 다음 일치로 선택 이동(끝이면 처음).
    #[test]
    fn filter_keeps_matches_ancestors_and_unloaded_folders() {
        let (mut ex, schema, tables) = sample();
        let b = ex.nodes[tables].children[1];
        ex.nodes[tables].expanded = false;
        ex.apply_filter(Some(crate::filterbar::Matcher::plain("b")));
        let rows = ex.visible_rows();
        assert!(rows.contains(&b), "일치 노드");
        assert_eq!(ex.label(tables).0, "Tables (1/3)", "폴더 개수 = 일치/전체");
        assert!(rows.contains(&tables) && rows.contains(&schema), "조상");
        assert!(ex.nodes[tables].expanded, "일치를 품은 폴더는 펼쳐진다");
        let a = ex.nodes[tables].children[0];
        assert!(!rows.contains(&a), "불일치 노드는 숨는다");
        let views = ex.nodes[schema].children[1];
        assert!(
            !rows.contains(&views),
            "일치가 있으면 안 읽은 다른 폴더는 숨는다(09-25 규칙)"
        );
        // 일치 노드 아래 자식은 전부 보인다(펼침) · 자식이 생기면 스스로 다시 거른다(⑩ 결함).
        ex.set_children(b, vec![sub_of(&ex, b, SubKind::Columns)]);
        let sub_b = ex.nodes[b].children[0];
        assert!(
            ex.visible_rows().contains(&sub_b),
            "일치 노드의 자손은 필터와 무관하게 보인다"
        );
        // 일치가 없는 트리 = 루트(연결 행)만 남는다(최소 표시 레벨 · 사용자 09-25).
        ex.apply_filter(Some(crate::filterbar::Matcher::plain("zzz")));
        assert_eq!(ex.visible_rows(), vec![0]);
        assert_eq!(ex.filter_hits(), 0);
        ex.apply_filter(Some(crate::filterbar::Matcher::plain("b")));
        assert_eq!(ex.filter_hits(), 1);
        assert!(ex.select_next_hit(true));
        assert_eq!(ex.selected, Some(b));
        ex.apply_filter(None);
        assert!(
            !ex.nodes[tables].expanded,
            "필터가 대신 펼친 폴더는 해제 때 다시 접힌다"
        );
        assert_eq!(ex.label(tables).0, "Tables (3)");
        ex.nodes[tables].expanded = true;
        assert!(ex.visible_rows().contains(&a), "빈 글 = 해제");
        assert_eq!(ex.filter_hits(), 0);
    }

    /// ★ 검색 인덱스(84 §3~4): 인덱스에 있는 **안 읽은 폴더**의 객체가 필터에 맞으면 부분 폴더("n/?")로 즉시 올라오고 완성 큐에 들어간다 ·
    /// 완성 응답(디프)은 있던 노드를 유지하며 전체 수로 · 이미 읽은 폴더의 항목은 건너뛴다 · 필터 해제 = 접힘 + 큐 비움.
    #[test]
    fn index_materializes_hits_into_unloaded_folders_and_completes() {
        let (mut ex, schema, tables) = sample();
        let views = ex.nodes[schema].children[1];
        assert_eq!(ex.nodes[views].state, LoadState::Idle);
        // L1 = MetaStore(85 §2): 이름 층으로 뷰 둘 · 테이블 하나(테이블 폴더는 트리에 이미 읽혔다).
        ex.meta.load_names(
            "HR",
            &[ObjectKind::Table, ObjectKind::View],
            &[
                (ObjectKind::View, "V_B1".into()),
                (ObjectKind::View, "V_A".into()),
                (ObjectKind::Table, "B".into()),
            ],
            1,
        );
        ex.index_done.insert("HR".into());
        ex.apply_filter(Some(crate::filterbar::Matcher::plain("b")));
        // 판정은 스레드(85 §6) — 시험은 그 응답(일치)을 직접 넣는다.
        let _ = ex.materialize_hits(&[
            ("HR".into(), ObjectKind::View, "V_B1".into()),
            ("HR".into(), ObjectKind::Table, "B".into()),
        ]);
        ex.refilter();
        ex.pump_complete();
        assert_eq!(
            ex.nodes[views].state,
            LoadState::Partial,
            "안 읽은 폴더 = 부분"
        );
        assert_eq!(ex.label(views).0, "Views (1/?)", "전체 수는 아직 모른다");
        assert_eq!(
            ex.nodes[tables].children.len(),
            3,
            "이미 읽은 폴더에는 인덱스가 끼어들지 않는다"
        );
        let rows = ex.visible_rows();
        assert!(rows.contains(&views), "부분 폴더가 보인다");
        let vb = ex.nodes[views].children[0];
        assert!(rows.contains(&vb), "일치 객체가 펼쳐져 보인다");
        assert_eq!(ex.filter_hits(), 2, "B(테이블) + V_B1");
        assert!(ex.soft.contains(&views), "완성 요청은 조용히(디프)");
        assert_eq!(ex.completing, Some(views));
        // 완성 응답: 있던 노드 유지 · Loaded · 일치/전체.
        ex.soft.remove(&views);
        ex.completing = None;
        let v = |name: &str| {
            let mut n = node(obj(name), 3);
            if let NodeKind::Object(o) = &mut n.kind {
                o.kind = ObjectKind::View;
            }
            n
        };
        ex.diff_children(views, vec![v("V_A"), v("V_B1"), v("V_B2")]);
        assert_eq!(ex.nodes[views].state, LoadState::Loaded);
        assert_eq!(ex.label(views).0, "Views (2/3)");
        assert!(
            ex.nodes[views].children.contains(&vb),
            "부분 노드는 그대로 남는다"
        );
        // 해제 = 필터가 펼친 폴더 접힘 · 큐 비움.
        ex.apply_filter(None);
        assert!(!ex.nodes[views].expanded);
        assert!(ex.complete_q.is_empty());
        assert_eq!(ex.label(views).0, "Views (3)");
    }

    /// 실행한 DDL → 그 폴더만: 읽어 둔 폴더 = 요청 1 · 안 읽은 폴더 = 0(펼칠 때 새로 읽는다) · ALTER = 읽어 둔 객체의 컬럼만 ·
    /// 같은 노드에 겹친 요청은 하나 · 오프라인 = 0.
    #[test]
    fn apply_ddl_targets_only_loaded_nodes() {
        let (mut ex, _, tables) = sample();
        let t = |verb, kind, schema: Option<&str>, name: &str| DdlTarget {
            verb,
            kind,
            schema: schema.map(String::from),
            name: name.into(),
        };
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Create, DdlKind::Table, None, "X"), None),
            1
        );
        assert!(ex.soft.contains(&tables));
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Create, DdlKind::Table, Some("hr"), "Y"), None),
            0,
            "진행 중인 조용한 갱신과 겹치면 다시 보내지 않는다"
        );
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Create, DdlKind::View, None, "V"), None),
            0
        );
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Create, DdlKind::Table, Some("NOPE"), "X"), None),
            0
        );
        // ALTER TABLE b: 컬럼을 읽어 둔 적 없으면 0 · 읽어 두었으면 1.
        let b = ex.nodes[tables].children[1];
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Alter, DdlKind::Table, None, "b"), None),
            0
        );
        ex.set_children(b, vec![sub_of(&ex, b, SubKind::Columns)]);
        let sub_b = ex.nodes[b].children[0];
        ex.set_children(sub_b, Vec::new());
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Alter, DdlKind::Table, None, "b"), None),
            1,
            "읽어 둔 하위 폴더(컬럼)만 다시"
        );
        // 스키마 생성 = 루트의 스키마 목록.
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Create, DdlKind::Schema, None, "APP"), None),
            1
        );
        ex.soft.clear();
        ex.offline = true;
        assert_eq!(
            ex.apply_ddl(&t(DdlVerb::Drop, DdlKind::Table, None, "A"), None),
            0
        );
    }

    /// 계층형 새로 고침(우클릭 · F5): 서버 = 읽어 둔 전부 · 종류 폴더 = 목록 + 그 아래 읽어 둔 객체 · 테이블 = 그 컬럼만 ·
    /// 컬럼 = 속한 테이블만 · 안 읽은 폴더 = 새로 읽기(Loading).
    #[test]
    fn manual_refresh_is_hierarchical() {
        let (mut ex, schema, tables) = sample();
        let (a, b) = (ex.nodes[tables].children[0], ex.nodes[tables].children[1]);
        let col = |n: &str| {
            node(
                NodeKind::Column(ColumnInfo {
                    name: n.into(),
                    data_type: "int".into(),
                    nullable: true,
                    position: 1,
                    default: String::new(),
                }),
                4,
            )
        };
        // 객체 → 하위 폴더(Columns) → 컬럼(83 §1).
        ex.set_children(a, vec![sub_of(&ex, a, SubKind::Columns)]);
        ex.set_children(b, vec![sub_of(&ex, b, SubKind::Columns)]);
        let (sub_a, sub_b) = (ex.nodes[a].children[0], ex.nodes[b].children[0]);
        ex.set_children(sub_a, vec![col("ID")]);
        ex.set_children(sub_b, vec![col("ID")]);
        let col_b = ex.nodes[sub_b].children[0];
        let views = ex.nodes[schema].children[1];
        let pick = |ex: &mut Explorer, i: usize| {
            ex.soft.clear();
            ex.selected = Some(i);
            ex.refresh_selected(false);
            let mut v: Vec<usize> = ex.soft.iter().copied().collect();
            v.sort_unstable();
            v
        };
        let mut all = vec![0, tables, sub_a, sub_b];
        all.sort_unstable();
        assert_eq!(
            pick(&mut ex, 0),
            all,
            "서버 = 스키마 목록 + 읽어 둔 폴더·하위 폴더 전부(객체 노드 자체는 요청 없음)"
        );
        let mut under = vec![tables, sub_a, sub_b];
        under.sort_unstable();
        assert_eq!(pick(&mut ex, schema), under, "스키마 = 그 아래 읽어 둔 것");
        assert_eq!(
            pick(&mut ex, tables),
            under,
            "종류 폴더 = 목록 + 읽어 둔 하위 폴더"
        );
        assert_eq!(
            pick(&mut ex, a),
            vec![sub_a],
            "테이블 = 그 테이블의 읽어 둔 하위 폴더만"
        );
        assert_eq!(
            pick(&mut ex, col_b),
            vec![sub_b],
            "컬럼 = 속한 폴더(Columns)만"
        );
        // 아직 안 읽은 **접힌** 폴더 = 펼치지 않는다(사용자 09-21) — 읽을 것이 없고, 다음에 펼칠 때 새로 읽는다.
        assert!(pick(&mut ex, views).is_empty());
        assert_eq!(ex.nodes[views].state, LoadState::Idle);
        assert!(
            !ex.nodes[views].expanded,
            "새로 고침이 접힌 노드를 펼치지 않는다"
        );
        // 펼쳐져 있는데 아직 못 읽은(오류) 폴더 = 새로 읽는다 · 펼침은 그대로.
        ex.nodes[views].expanded = true;
        ex.nodes[views].state = LoadState::Error("x".into());
        assert!(pick(&mut ex, views).is_empty());
        assert_eq!(ex.nodes[views].state, LoadState::Loading);
        assert!(ex.nodes[views].expanded);
        // 강제(Shift+F5)도 접힌 노드는 펼치지 않는다 — 읽어 둔 것을 버리기만.
        ex.nodes[a].expanded = false;
        ex.selected = Some(a);
        ex.refresh_selected(true);
        assert!(!ex.nodes[a].expanded);
        assert!(ex.nodes[a].children.is_empty());
        assert_eq!(ex.nodes[a].state, LoadState::Idle);
        assert!(ex
            .take_actions()
            .iter()
            .any(|x| matches!(x, ExplorerAction::Status(_))));
    }

    /// 연결이 해제된 서버에서 새로 고침 = 읽어 둔 목록을 "접속 안 됨" 안내로 바꾸고 알린다 · 접힌 노드는 펼치지 않는다 · 요청은 0.
    #[test]
    fn refresh_on_offline_server_shows_the_disconnected_state() {
        let (mut ex, schema, tables) = sample();
        ex.offline = true;
        ex.nodes[tables].expanded = true;
        ex.selected = Some(tables);
        ex.soft.clear();
        ex.refresh_selected(false);
        assert!(ex.soft.is_empty(), "서버로 가는 요청 없음");
        assert!(ex.nodes[tables].children.is_empty());
        assert!(matches!(ex.nodes[tables].state, LoadState::Error(_)));
        assert!(ex
            .take_actions()
            .iter()
            .any(|a| matches!(a, ExplorerAction::Notice(_))));
        // 접힌 폴더 = 펼치지 않는다(읽어 둔 것만 버린다) · 스키마를 고르면 그 아래 폴더들이 대상.
        let views = ex.nodes[schema].children[1];
        ex.nodes[views].state = LoadState::Loaded;
        ex.nodes[views].expanded = false;
        ex.selected = Some(schema);
        ex.refresh_selected(false);
        assert_eq!(ex.nodes[views].state, LoadState::Idle);
        assert!(!ex.nodes[views].expanded);
        assert!(
            !ex.nodes[schema].children.is_empty(),
            "스키마의 폴더 줄은 남는다"
        );
        // 오프라인 표시는 프로필 이름이 없는 접속에도 나온다.
        ex.conn_desc = "oracle://u@h:1521/s".into();
        ex.profile_name.clear();
        ex.endpoint = "h:1521".into();
        assert_eq!(
            ex.label(0),
            ("h:1521".to_string(), t(Msg::ExpOffline).to_string())
        );
    }

    /// 못 찾음 신호: 이름이 트리에 있을 때만 그 폴더 · 폴더당 60초에 1회 · 이름을 모르면 현재 스키마의 읽어 둔 테이블·뷰 폴더.
    #[test]
    fn missing_signal_is_rate_limited() {
        let (mut ex, _, tables) = sample();
        assert_eq!(
            ex.note_missing(Some("hr.zzz"), None),
            0,
            "트리에 없는 이름 = 트리는 낡지 않았다"
        );
        assert_eq!(ex.note_missing(Some("HR.b"), None), 1);
        ex.soft.clear();
        assert_eq!(
            ex.note_missing(Some("b"), None),
            0,
            "60초 안 = 다시 읽지 않는다"
        );
        ex.missing_at.clear();
        assert_eq!(ex.note_missing(None, None), 1);
        assert!(ex.soft.contains(&tables));
        assert_eq!(
            missing_name("relation \"emp\" does not exist").as_deref(),
            Some("emp")
        );
        assert_eq!(
            missing_name("Invalid object name 'dbo.emp'.").as_deref(),
            Some("dbo.emp")
        );
        assert_eq!(missing_name("no such table: emp").as_deref(), Some("emp"));
        assert_eq!(
            missing_name("ORA-00942: table or view \"HR\".\"EMP\" does not exist").as_deref(),
            Some("EMP")
        );
        assert_eq!(
            missing_name("ORA-00942: table or view does not exist"),
            None
        );
    }

    fn pump(ex: &mut Explorer, mut done: impl FnMut(&Explorer) -> bool) {
        for _ in 0..200 {
            ex.drain();
            if done(ex) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("meta thread did not answer in 5s");
    }

    /// 실제 SQLite 파일로 전 경로: 접속 → Tables 펼침 → 다른 세션이 CREATE/DROP → `apply_ddl` → 디프(남는 노드 인덱스 유지 ·
    /// 새 노드 강조) → 워터마크(`PRAGMA schema_version`)가 바뀌면 같은 폴더를 조용히 다시 읽는다.
    #[test]
    fn sqlite_end_to_end_ddl_refresh() {
        let path = std::env::temp_dir().join(format!("nsql_t138_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let spec = ConnectSpec::parse(&format!("sqlite:{}", path.display())).unwrap();
        let mut other = nsql_drivers::open(&spec, Dialect::Sqlite).unwrap();
        let mut run = |sql: &str| {
            other
                .execute(&nsql_core::ExecRequest {
                    sql: sql.into(),
                    params: vec![],
                })
                .unwrap();
        };
        run("CREATE TABLE a (id INTEGER)");
        let mut ex = Explorer::new(Box::new(|| {}), true);
        ex.connect(&spec, "t138", false);
        pump(&mut ex, |e| !e.nodes[0].children.is_empty());
        let schema = ex.nodes[0].children[0];
        if !ex.nodes[schema].expanded {
            ex.toggle(schema);
        }
        let tables = ex.folder_node(schema, ObjectKind::Table).unwrap();
        ex.toggle(tables);
        pump(&mut ex, |e| e.nodes[tables].state == LoadState::Loaded);
        let a = ex.nodes[tables].children[0];
        // 다른 세션이 테이블을 만든다 → 실행한 DDL 반영.
        run("CREATE TABLE b (id INTEGER)");
        let t = nsql_core::ddl_target("CREATE TABLE b (id INTEGER)", Dialect::Sqlite).unwrap();
        ex.now_hint = 10;
        assert_eq!(ex.apply_ddl(&t, None), 1);
        pump(&mut ex, |e| e.soft.is_empty());
        let kids = ex.nodes[tables].children.clone();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0], a, "남는 노드는 그대로");
        assert_eq!(ex.fresh.len(), 1);
        // 워터마크: 첫 값 = 기준 · DROP 뒤 값이 달라지면 폴더를 다시 읽어 b가 사라진다.
        assert!(ex.watermark_poll(false));
        pump(&mut ex, |e| !e.wm_inflight);
        assert!(ex.soft.is_empty(), "첫 지문은 기준일 뿐");
        run("DROP TABLE b");
        assert!(ex.watermark_poll(false));
        pump(&mut ex, |e| !e.wm_inflight && e.soft.is_empty());
        assert_eq!(ex.nodes[tables].children, vec![a]);
        ex.disconnect();
        drop(other);
        let _ = std::fs::remove_file(&path);
    }

    /// 워터마크: 읽어 둔 폴더가 있는 스키마만 묻는다 · 첫 값은 기준 · 달라지면 그 스키마의 읽어 둔 노드를 조용히 다시 · 유휴로 닫힌
    /// 메타 세션은 깨우지 않는다 · 방언별 질의(따옴표 이스케이프).
    #[test]
    fn watermark_rules() {
        let (mut ex, schema, tables) = sample();
        assert!(ex.watermark_poll(false));
        assert!(!ex.watermark_poll(false), "진행 중이면 다시 묻지 않는다");
        ex.wm_inflight = false;
        ex.suspended = true;
        assert!(
            !ex.watermark_poll(false),
            "유휴로 닫힌 메타 세션은 깨우지 않는다"
        );
        ex.suspended = false;
        assert!(ex.watermark_poll(true), "all = 지문 없이 읽어 둔 것 전부");
        assert!(ex.soft.contains(&tables) && ex.soft.contains(&0));
        let _ = schema;
        let q = watermark_sql(Dialect::Oracle, "O'X").unwrap();
        assert!(q.contains("OWNER = 'O''X'") && q.contains("LAST_DDL_TIME"));
        assert!(watermark_sql(Dialect::Mssql, "dbo")
            .unwrap()
            .contains("SCHEMA_ID('dbo')"));
        assert!(watermark_sql(Dialect::Postgres, "public")
            .unwrap()
            .contains("relnatts"));
        assert_eq!(
            watermark_sql(Dialect::Sqlite, "main").as_deref(),
            Some("PRAGMA schema_version")
        );
        assert!(watermark_sql(Dialect::Odbc, "x").is_none());
    }

    /// 현재 스키마 판정(09-23 사용자 "FROM 뒤 M4S_ 테이블이 안 보인다" — SQL Server는 계정 ≠ 스키마): 서버 값 → 계정 → 기본 → 단일.
    #[test]
    fn current_schema_prefers_server_then_user_then_default() {
        let l = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let mssql = l(&["dbo", "sales", "hr"]);
        assert_eq!(
            pick_current_schema(&mssql, Some("dbo"), "m4plan"),
            Some("dbo".into())
        );
        assert_eq!(
            pick_current_schema(&mssql, None, "m4plan"),
            Some("dbo".into()),
            "계정이 스키마가 아니면 dbo"
        );
        assert_eq!(
            pick_current_schema(&mssql, Some("nope"), "SALES"),
            Some("sales".into()),
            "서버 값이 목록에 없으면 계정"
        );
        let ora = l(&["BISCM", "SYS", "HR"]);
        assert_eq!(
            pick_current_schema(&ora, Some("BISCM"), "biscm"),
            Some("BISCM".into())
        );
        assert_eq!(
            pick_current_schema(&ora, None, "hr"),
            Some("HR".into()),
            "목록 표기로 돌려준다"
        );
        let pg = l(&["public", "app"]);
        assert_eq!(
            pick_current_schema(&pg, Some("public"), "postgres"),
            Some("public".into())
        );
        assert_eq!(
            pick_current_schema(&l(&["only"]), None, "x"),
            Some("only".into()),
            "하나뿐이면 그것"
        );
        assert_eq!(pick_current_schema(&l(&["a", "b"]), None, "x"), None);
        assert_eq!(
            pick_current_schema(&l(&["a", "b"]), Some(""), ""),
            None,
            "빈 이름은 무시"
        );
    }
}

#[cfg(test)]
mod search_thread_tests {
    use super::*;
    use std::collections::HashMap;

    /// 스레드 판정 = 순수 함수: 스키마 정렬 순 · 상한 · 대소문자 무시(plain 매처).
    #[test]
    fn search_hits_matches_across_schemas_with_limit() {
        let e = |s: &str, k: ObjectKind, n: &str| nsql_catalog::NameEntry {
            schema: s.into(),
            kind: k,
            name: n.into(),
        };
        let names: HashMap<String, Vec<nsql_catalog::NameEntry>> = HashMap::from([
            (
                "HR".to_string(),
                vec![
                    e("HR", ObjectKind::Table, "EMP"),
                    e("HR", ObjectKind::View, "V_EMP"),
                ],
            ),
            ("B".to_string(), vec![e("B", ObjectKind::Table, "employee")]),
        ]);
        let m = crate::filterbar::Matcher::plain("emp");
        let all = search_hits(&names, &m, 10);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].0, "B", "스키마 이름순");
        assert_eq!(search_hits(&names, &m, 2).len(), 2, "상한");
        assert!(search_hits(&names, &crate::filterbar::Matcher::plain("zzz"), 10).is_empty());
    }
}

#[cfg(test)]
mod detail_cache_tests {
    use super::detail_key_hit;

    /// 상세 캐시 무효화 범위(09-26): 전부 · 스키마 · 객체(하위/컬럼 포함) — 이름이 접두사인 다른 객체는 건드리지 않는다.
    #[test]
    fn detail_cache_invalidation_scope() {
        let o = "o:BISCM.EMP:Table";
        let c = "c:BISCM.EMP.NAME";
        let i = "i:BISCM.EMP:Indexes:EMP_PK";
        let other = "o:BISCM.EMP2:Table";
        let other_schema = "o:HR.EMP:Table";
        for k in [o, c, i, other, other_schema] {
            assert!(detail_key_hit(k, None, None));
        }
        assert!(detail_key_hit(o, Some("BISCM"), None));
        assert!(!detail_key_hit(other_schema, Some("BISCM"), None));
        assert!(detail_key_hit(o, Some("BISCM"), Some("EMP")));
        assert!(detail_key_hit(c, Some("BISCM"), Some("EMP")));
        assert!(detail_key_hit(i, Some("BISCM"), Some("EMP")));
        assert!(
            !detail_key_hit(other, Some("BISCM"), Some("EMP")),
            "EMP2는 EMP가 아니다"
        );
        assert!(!detail_key_hit(other_schema, Some("BISCM"), Some("EMP")));
        assert!(!detail_key_hit("x", Some("A"), None));
    }
}
