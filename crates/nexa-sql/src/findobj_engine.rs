//! ★ 객체 찾기 **전용 엔진**(docs/113 §4 v2 · 사용자 10-15): 탐색기 메타 스레드와 **분리**된 세션으로 돈다(메인 창·탐색기에 영향 0) ·
//! **항상 최신** = 사본 없이 DB를 직접 조회(D-295) · 작업을 **잘게**(서버 × 스키마 × 단계) 쪼개 서버마다 워커 N이 병렬로 처리하고
//! 작업 하나가 끝날 때마다 창으로 보낸다(빨리 보이기 시작) · 취소 = rev 비교(작업 사이) · 서버 안 동시성 상한 = `findobj.workers`.
//!
//! 흐름: `start` → 서버마다 **계획 스레드**(세션 1 열기 → 스키마 목록(실시간) → 작업 큐 → `Planned`) → 같은 스레드가 워커 0 · 추가 워커(세션 각자)
//!       → 큐에서 작업을 꺼내 질의 1회 → `Task` → 큐가 비고 워커가 다 끝나면 `ServerDone`. 세션은 작업이 끝나면 닫는다(창 전용 · 상주 0).
use nsql_catalog::find::{self, FindPat};
use nsql_catalog::ObjectKind;
use nsql_core::Dialect;
use nsql_script::ConnectSpec;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 찾기 조건(검색어(대소문자·낱말) · 정규식(이름·설명에만) · 대상 4 · 종류 · 스키마(비면 전부) · 상한).
#[derive(Clone, Debug)]
pub(crate) struct FindQuery {
    pub pat: FindPat,
    pub regex: bool,
    pub names: bool,
    pub comments: bool,
    pub columns: bool,
    pub source: bool,
    pub kinds: Vec<ObjectKind>,
    pub kinds_all: bool,
    pub schemas: Vec<String>,
    pub max_hits: usize,
    pub lines_per_object: usize,
    /// ★ SQL Server = 접근 가능한 **모든 DB**(시스템 DB 제외 · 사용자 10-15 · 설정 `findobj.all_dbs` · 탐색기 검색 설정과 독립) · 끔 = 현재 DB.
    pub all_dbs: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FindWhat {
    Name,
    Comment(String),
    Column { column: String, data_type: String },
    Source { line: u32, text: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindHit {
    pub db: Option<String>,
    pub schema: String,
    pub name: String,
    pub kind: ObjectKind,
    pub what: FindWhat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FindStage {
    Names,
    Columns,
    Source,
}

/// 작업 하나 = (단계, 스키마, 소스면 종류 묶음).
#[derive(Clone, Debug)]
struct Task {
    stage: FindStage,
    schema: String,
    kinds: Vec<ObjectKind>,
    /// SSMS DB 단위(그 밖 None) — 워커는 작업의 DB가 바뀌면 `USE`.
    db: Option<String>,
}

/// 엔진 → 창.
#[derive(Clone, Debug)]
pub(crate) enum EngineMsg {
    /// 서버 계획 완료(스키마 수 · 작업 수 · 안내).
    Planned {
        rev: u64,
        server: usize,
        label: String,
        schemas: usize,
        tasks: usize,
        note: Option<String>,
    },
    /// 작업 하나 끝(일치 · 조사한 행 · 안내 · 소요 ms).
    Task {
        rev: u64,
        server: usize,
        label: String,
        stage: FindStage,
        schema: String,
        hits: Vec<FindHit>,
        scanned: usize,
        note: Option<String>,
        ms: u64,
    },
    /// 서버의 작업 전부 끝(오류면 그 글).
    ServerDone {
        rev: u64,
        server: usize,
        label: String,
        err: Option<String>,
    },
}

/// 검색 대상 서버(탐색기 칸의 키 = 비밀번호 없음 · 세션 자격 금고로 연다).
#[derive(Clone, Debug)]
pub(crate) struct ServerTarget {
    pub spec: ConnectSpec,
    pub label: String,
    /// SSMS 현재 DB(그 밖 None).
    pub db: Option<String>,
}

pub(crate) struct Engine {
    tx: Sender<EngineMsg>,
    rx: Receiver<EngineMsg>,
    /// 살아 있는 검색의 rev(워커는 작업 사이마다 비교 · 다르면 멈춤).
    live: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Engine {
    pub(crate) fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            live: Arc::new(AtomicU64::new(0)),
            wake,
        }
    }

    /// 지금 살아 있는 검색을 취소한다(워커는 다음 작업 경계에서 멈춤 · 진행 중 질의는 호출 상한까지).
    pub(crate) fn cancel(&self) {
        self.live.store(0, Ordering::SeqCst);
    }

    pub(crate) fn try_recv(&self) -> Option<EngineMsg> {
        self.rx.try_recv().ok()
    }

    /// 검색 시작 — 서버마다 계획 스레드 + 워커. 돌려주는 값 = 서버 수.
    pub(crate) fn start(
        &self,
        rev: u64,
        q: FindQuery,
        servers: Vec<ServerTarget>,
        workers_per_server: usize,
    ) -> usize {
        self.live.store(rev, Ordering::SeqCst);
        let n = servers.len();
        for (ix, srv) in servers.into_iter().enumerate() {
            let tx = self.tx.clone();
            let live = Arc::clone(&self.live);
            let wake = Arc::clone(&self.wake);
            let q = q.clone();
            let workers = workers_per_server.clamp(1, 8);
            let _ = std::thread::Builder::new()
                .name(format!("nsql-findobj-{ix}"))
                .spawn(move || plan_and_run(rev, ix, srv, q, workers, tx, live, wake));
        }
        n
    }
}

fn alive(live: &AtomicU64, rev: u64) -> bool {
    live.load(Ordering::SeqCst) == rev
}

/// 소스 단계의 종류 묶음(작업을 잘게 — 루틴·패키지 / 뷰·MV / 트리거) — 선택된 종류만.
fn source_groups(q: &FindQuery) -> Vec<Vec<ObjectKind>> {
    let groups: [&[ObjectKind]; 3] = [
        &[
            ObjectKind::Procedure,
            ObjectKind::Function,
            ObjectKind::Package,
            ObjectKind::PackageBody,
            ObjectKind::Type,
        ],
        &[ObjectKind::View, ObjectKind::MaterializedView],
        &[ObjectKind::Trigger],
    ];
    groups
        .iter()
        .map(|g| {
            g.iter()
                .copied()
                .filter(|k| q.kinds_all || q.kinds.contains(k))
                .collect::<Vec<_>>()
        })
        .filter(|g| !g.is_empty())
        .collect()
}

/// 정규식에 메타 문자가 있는가(없으면 컬럼·소스는 글자 그대로 LIKE · D-293).
fn regex_meta(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '^' | '$' | '\\'
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_and_run(
    rev: u64,
    ix: usize,
    srv: ServerTarget,
    q: FindQuery,
    workers: usize,
    tx: Sender<EngineMsg>,
    live: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let label = srv.label.clone();
    let default = srv.spec.dialect.unwrap_or(Dialect::Oracle);
    let done = |err: Option<String>| {
        let _ = tx.send(EngineMsg::ServerDone {
            rev,
            server: ix,
            label: label.clone(),
            err,
        });
        wake();
    };
    if !alive(&live, rev) {
        return;
    }
    // 세션 1(계획 + 워커 0).
    let mut s0 = match crate::explorer::open_meta(&srv.spec, default) {
        Ok(s) => s,
        Err(e) => {
            done(Some(e.message));
            return;
        }
    };
    let is_mssql = matches!(s0.dialect(), Dialect::Mssql);
    // ★ DB 단위 목록(10-15 · 사용자 "객체 검색 창에서는 전체를 대상으로 하는 스위치"): SQL Server + `all_dbs` = 접근 가능한 사용자 DB 전부 ·
    //   그 밖 = 현재 DB 하나(SSMS) 또는 DB 개념 없음(None 하나).
    let dbs: Vec<Option<String>> = if is_mssql && q.all_dbs {
        match find::databases(s0.as_mut()) {
            Ok(v) if !v.is_empty() => v.into_iter().map(Some).collect(),
            Ok(_) => vec![srv.db.clone()],
            Err(e) => {
                done(Some(e.message));
                return;
            }
        }
    } else {
        vec![srv.db.clone().filter(|d| !d.is_empty())]
    };
    let regex_skip = q.regex && regex_meta(&q.pat.text);
    let mut tasks: VecDeque<Task> = VecDeque::new();
    let mut n_schemas = 0usize;
    for db in &dbs {
        if !alive(&live, rev) {
            return;
        }
        if let Some(d) = db.as_deref() {
            let _ = s0.set_option("schema", d);
        }
        // 스키마 목록 = 실시간(사용자 필터가 있으면 그것).
        let schemas: Vec<String> = if !q.schemas.is_empty() {
            q.schemas.clone()
        } else if matches!(s0.dialect(), Dialect::Sqlite) {
            vec!["main".to_string()]
        } else {
            match nsql_catalog::schemas_opt(
                s0.as_mut(),
                nsql_catalog::SchemaOpts {
                    show_system: false,
                    hide_empty: true,
                },
            ) {
                Ok(v) => v,
                Err(e) => {
                    done(Some(e.message));
                    return;
                }
            }
        };
        n_schemas += schemas.len();
        for sc in &schemas {
            if q.names || q.comments {
                tasks.push_back(Task {
                    stage: FindStage::Names,
                    schema: sc.clone(),
                    kinds: Vec::new(),
                    db: db.clone(),
                });
            }
            if q.columns && !regex_skip {
                tasks.push_back(Task {
                    stage: FindStage::Columns,
                    schema: sc.clone(),
                    kinds: Vec::new(),
                    db: db.clone(),
                });
            }
            if q.source && !regex_skip {
                for g in source_groups(&q) {
                    tasks.push_back(Task {
                        stage: FindStage::Source,
                        schema: sc.clone(),
                        kinds: g,
                        db: db.clone(),
                    });
                }
            }
        }
    }
    let schemas_len = n_schemas;
    let note = (regex_skip && (q.columns || q.source))
        .then(|| nsql_i18n::t(nsql_i18n::Msg::FoRegexLocalOnly).to_string());
    let _ = tx.send(EngineMsg::Planned {
        rev,
        server: ix,
        label: label.clone(),
        schemas: schemas_len,
        tasks: tasks.len(),
        note,
    });
    wake();
    let queue = Arc::new(Mutex::new(tasks));
    let remaining = Arc::new(AtomicUsize::new(workers));
    let db = srv.db.clone().filter(|d| !d.is_empty());
    // 추가 워커(세션 각자).
    for w in 1..workers {
        let (tx, live, wake, q, queue, remaining, spec, label, db) = (
            tx.clone(),
            Arc::clone(&live),
            Arc::clone(&wake),
            q.clone(),
            Arc::clone(&queue),
            Arc::clone(&remaining),
            srv.spec.clone(),
            label.clone(),
            db.clone(),
        );
        let _ = std::thread::Builder::new()
            .name(format!("nsql-findobj-{ix}-{w}"))
            .spawn(move || {
                if alive(&live, rev) && !queue.lock().map(|g| g.is_empty()).unwrap_or(true) {
                    if let Ok(mut s) = crate::explorer::open_meta(&spec, default) {
                        if let Some(d) = db.as_deref() {
                            let _ = s.set_option("schema", d);
                        }
                        work(
                            rev,
                            ix,
                            &label,
                            &q,
                            db.clone(),
                            s.as_mut(),
                            &queue,
                            &tx,
                            &live,
                            &wake,
                        );
                    }
                }
                if remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
                    let _ = tx.send(EngineMsg::ServerDone {
                        rev,
                        server: ix,
                        label: label.clone(),
                        err: None,
                    });
                    wake();
                }
            });
    }
    // 계획 세션은 마지막 DB에 서 있다 → 워커 루프가 작업의 DB로 맞춘다(`cur_db` = 마지막 계획 DB).
    let last_db = dbs.last().cloned().flatten();
    work(
        rev,
        ix,
        &label,
        &q,
        last_db,
        s0.as_mut(),
        &queue,
        &tx,
        &live,
        &wake,
    );
    drop(s0);
    if remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
        done(None);
    }
}

/// 워커 루프: 큐에서 작업을 꺼내 질의 1회 → 결과 한 묶음.
#[allow(clippy::too_many_arguments)]
fn work(
    rev: u64,
    ix: usize,
    label: &str,
    q: &FindQuery,
    mut cur_db: Option<String>,
    s: &mut dyn nsql_core::Session,
    queue: &Mutex<VecDeque<Task>>,
    tx: &Sender<EngineMsg>,
    live: &AtomicU64,
    wake: &Arc<dyn Fn() + Send + Sync>,
) {
    loop {
        if !alive(live, rev) {
            return;
        }
        let task = match queue.lock() {
            Ok(mut g) => g.pop_front(),
            Err(_) => None,
        };
        let Some(task) = task else {
            return;
        };
        let t0 = Instant::now();
        // 작업의 DB가 세션의 현재 DB와 다르면 전환(SSMS `USE` · 전 DB 검색).
        if task.db != cur_db {
            if let Some(d) = task.db.as_deref() {
                let _ = s.set_option("schema", d);
            }
            cur_db = task.db.clone();
        }
        let (hits, scanned, note) = run_task(s, q, &task, task.db.as_deref());
        let _ = tx.send(EngineMsg::Task {
            rev,
            server: ix,
            label: label.to_string(),
            stage: task.stage,
            schema: task.schema,
            hits,
            scanned,
            note,
            ms: t0.elapsed().as_millis() as u64,
        });
        wake();
    }
}

/// 작업 하나 = 질의 1회(+ 클라이언트 판정). 반환 = (일치, 조사한 행, 안내).
fn run_task(
    s: &mut dyn nsql_core::Session,
    q: &FindQuery,
    task: &Task,
    db: Option<&str>,
) -> (Vec<FindHit>, usize, Option<String>) {
    let schemas = [task.schema.clone()];
    let trunc_note = || nsql_i18n::t(nsql_i18n::Msg::FoTruncated).to_string();
    match task.stage {
        FindStage::Names => {
            let pat = (!q.regex).then_some(&q.pat);
            match find::names(s, &schemas, pat, q.comments, q.max_hits) {
                Ok((rows, trunc)) => {
                    let scanned = rows.len();
                    let hits = filter_names(rows, q, db);
                    (hits, scanned, trunc.then(trunc_note))
                }
                Err(e) => (Vec::new(), 0, Some(e.message)),
            }
        }
        FindStage::Columns => match find::columns(s, &schemas, &q.pat, q.max_hits) {
            Ok((v, trunc)) => {
                let scanned = v.len();
                let hits = v
                    .into_iter()
                    .filter(|c| q.kinds_all || q.kinds.contains(&c.table_kind))
                    .map(|c| FindHit {
                        db: db.map(str::to_string),
                        schema: c.schema,
                        name: c.table,
                        kind: c.table_kind,
                        what: FindWhat::Column {
                            column: c.column,
                            data_type: c.data_type,
                        },
                    })
                    .collect();
                (hits, scanned, trunc.then(trunc_note))
            }
            Err(e) => (Vec::new(), 0, Some(e.message)),
        },
        FindStage::Source => {
            match find::source(
                s,
                &schemas,
                &task.kinds,
                &q.pat,
                q.max_hits,
                q.lines_per_object,
            ) {
                Ok((v, trunc)) => {
                    let scanned = v.len();
                    let hits = v
                        .into_iter()
                        .map(|h| FindHit {
                            db: db.map(str::to_string),
                            schema: h.schema,
                            name: h.name,
                            kind: h.kind,
                            what: FindWhat::Source {
                                line: h.line,
                                text: h.text,
                            },
                        })
                        .collect();
                    (hits, scanned, trunc.then(trunc_note))
                }
                Err(e) => (Vec::new(), 0, Some(e.message)),
            }
        }
    }
}

/// 이름·설명 행 → 일치(정규식은 여기서 · 그 밖은 서버 LIKE가 걸렀고 낱말 경계만 다시).
fn filter_names(rows: Vec<find::NameHit>, q: &FindQuery, db: Option<&str>) -> Vec<FindHit> {
    let rx = q.regex.then(|| {
        let src = if q.pat.case_sensitive {
            q.pat.text.clone()
        } else {
            format!("(?i){}", q.pat.text)
        };
        fancy_regex::Regex::new(&src).ok()
    });
    if q.regex && rx.as_ref().is_none_or(Option::is_none) {
        return Vec::new();
    }
    let hit = |hay: &str| -> bool {
        match rx.as_ref().and_then(|r| r.as_ref()) {
            Some(r) => r.is_match(hay).unwrap_or(false),
            None => find::line_matches(hay, &q.pat),
        }
    };
    let mut out = Vec::new();
    for e in rows {
        if !q.kinds_all && !q.kinds.contains(&e.kind) {
            continue;
        }
        let what = if q.names && hit(&e.name) {
            FindWhat::Name
        } else if let Some(c) = e.comment.as_deref().filter(|c| q.comments && hit(c)) {
            FindWhat::Comment(c.to_string())
        } else {
            continue;
        };
        out.push(FindHit {
            db: db.map(str::to_string),
            schema: e.schema,
            name: e.name,
            kind: e.kind,
            what,
        });
        if q.max_hits > 0 && out.len() >= q.max_hits {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str, regex: bool) -> FindQuery {
        FindQuery {
            pat: FindPat {
                text: text.into(),
                case_sensitive: false,
                whole_word: false,
            },
            regex,
            names: true,
            comments: true,
            columns: true,
            source: true,
            kinds: Vec::new(),
            kinds_all: true,
            schemas: Vec::new(),
            max_hits: 0,
            lines_per_object: 0,
            all_dbs: true,
        }
    }

    #[test]
    fn source_groups_follow_kinds() {
        assert_eq!(
            source_groups(&q("x", false)).len(),
            3,
            "전부 = 루틴 · 뷰 · 트리거"
        );
        let mut only_views = q("x", false);
        only_views.kinds_all = false;
        only_views.kinds = vec![ObjectKind::View];
        let g = source_groups(&only_views);
        assert_eq!(g, vec![vec![ObjectKind::View]]);
    }

    #[test]
    fn regex_meta_and_name_filter() {
        assert!(regex_meta("cust.*"));
        assert!(!regex_meta("customer"));
        let rows = vec![
            find::NameHit {
                schema: "HR".into(),
                name: "CUSTOMER".into(),
                kind: ObjectKind::Table,
                comment: None,
            },
            find::NameHit {
                schema: "HR".into(),
                name: "ORDERS".into(),
                kind: ObjectKind::Table,
                comment: Some("orders by customer".into()),
            },
            find::NameHit {
                schema: "HR".into(),
                name: "EMP".into(),
                kind: ObjectKind::Table,
                comment: None,
            },
        ];
        let hits = filter_names(rows.clone(), &q("cust", false), None);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].what, FindWhat::Name);
        assert!(matches!(hits[1].what, FindWhat::Comment(_)));
        let hits = filter_names(rows.clone(), &q("^cust", true), None);
        assert_eq!(hits.len(), 1, "정규식 = 이름 앞머리만");
        let mut no_comment = q("cust", false);
        no_comment.comments = false;
        assert_eq!(filter_names(rows, &no_comment, None).len(), 1);
    }
}
