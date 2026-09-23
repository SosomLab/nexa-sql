//! **코드 완성 호스트**(docs/76 · docs/47 §6 · 사용자 09-23) — 팝업 상태 · 트리거 · 후보 조립 · 확정 · MRU · 문서 아웃라인 캐시.
//!
//! 계산은 `nsql_script::intel`(문맥·랭킹 · 순수)과 `nsql_script::outline`(문서 심볼)에 있고, 메타는 탐색기가 채운 `MetaStore`
//! 스냅샷([`MetaView`])만 읽는다(서버 접속 0 · 26 §8). 팝업 = nexa-ctl `ContextMenu` 재사용(D-201 · 창 안 배치 규칙).
//! 호스트(main.rs)가 하는 것: 글자 입력 뒤 [`Intel::after_char`] → 필요하면 [`Intel::request`] · 키(↑↓ Enter Tab Esc)를 팝업에 먼저 ·
//! [`Intel::take_accept`]로 확정 글자를 편집기에 · 틱에서 [`Intel::due`] 디바운스.

use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::geom::{Point, Rect};
use nsql_catalog::ObjectKind;
use nsql_core::Dialect;
use nsql_run::meta::{ColState, Interner, Snapshot};
use nsql_script::builtins;
use nsql_script::intel::{self, Cand, CandKind, Context, CtxKind, MatchMode};
use nsql_script::outline::{self, Outline, SymKind};
use nsql_settings::Settings;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 설정 묶음(`intel.*` · 47 §8-1).
#[derive(Clone, Debug)]
pub(crate) struct IntelCfg {
    pub enabled: bool,
    pub auto: bool,
    pub delay_ms: u64,
    pub triggers: Vec<char>,
    pub min_chars: usize,
    pub mode: MatchMode,
    pub recent: bool,
    pub max_items: usize,
    pub rows: usize,
    pub keywords: bool,
    pub doc_words: bool,
    pub insert_case: String,
    pub show_types: bool,
    /// T-178: 내장 함수·시스템 패키지·사전 객체(정적 표) · 삽입 옵션 · 시그니처 도움 · 예산.
    pub functions: bool,
    pub insert_parens: bool,
    pub insert_alias: bool,
    pub insert_space: bool,
    pub insert_columns: bool,
    pub signature_help: bool,
    pub budget_ms: u64,
}

impl IntelCfg {
    pub(crate) fn from_settings(s: &Settings) -> IntelCfg {
        IntelCfg {
            enabled: s.flag("intel.enabled"),
            auto: s.flag("intel.auto_activation"),
            delay_ms: s.int("intel.delay_ms").clamp(0, 2000) as u64,
            triggers: s
                .get("intel.trigger_chars")
                .unwrap_or(".")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect(),
            min_chars: s.int("intel.min_chars").clamp(1, 5) as usize,
            mode: MatchMode::parse(s.get("intel.match").unwrap_or("fuzzy")),
            recent: s.flag("intel.recent_boost"),
            max_items: s.int("intel.max_items").clamp(50, 2000) as usize,
            rows: s.int("intel.popup_rows").clamp(6, 30) as usize,
            keywords: s.flag("intel.keywords"),
            doc_words: s.flag("intel.document_words"),
            insert_case: s.get("intel.insert_case").unwrap_or("default").to_string(),
            show_types: s.flag("intel.show_types"),
            functions: s.flag("intel.functions"),
            insert_parens: s.flag("intel.insert_parens"),
            insert_alias: s.flag("intel.insert_alias"),
            insert_space: s.flag("intel.insert_space"),
            insert_columns: s.flag("intel.insert_columns"),
            signature_help: s.flag("intel.signature_help"),
            budget_ms: s.int("intel.budget_ms").clamp(5, 500) as u64,
        }
    }
}

/// 메타 읽기 창(탐색기 스냅샷 · 접속 없으면 None).
pub(crate) struct MetaView<'a> {
    pub names: &'a Interner,
    pub snap: Arc<Snapshot>,
}

/// 호스트에 되돌리는 요청 — 컬럼이 없어서 즉시 채워야 할 테이블(47 §4 "즉시 채움").
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NeedColumns {
    pub schema: Option<String>,
    pub table: String,
}

/// 확정 결과(편집기에 적용할 것).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Accept {
    /// 바이트 구간(접두 자리).
    pub replace: Range<usize>,
    pub text: String,
    /// 넣은 뒤 캐럿을 끝에서 이만큼 앞으로(`NAME()` = 1 · 괄호 안).
    pub caret_back: usize,
}

/// 문서 캐시 항목(탭별 · 본문 세대 열쇠 · D-203).
struct DocCache {
    rev: u64,
    outline: Outline,
    words: Vec<String>,
}

pub(crate) struct Intel {
    pub(crate) menu: ContextMenu,
    cfg: IntelCfg,
    cands: Vec<Cand>,
    ctx: Option<Context>,
    /// 팝업이 열린 탭 id(다른 탭이면 닫는다).
    tab: Option<u64>,
    mru: Vec<String>,
    /// 자동 활성 마감(디바운스).
    due: Option<Instant>,
    docs: HashMap<u64, DocCache>,
    accept: Option<Accept>,
    /// 마지막 요청이 남긴 즉시 채움 요청.
    needs: Vec<NeedColumns>,
    /// "불러오는 중" 표시 여부(마지막 요청).
    pub(crate) loading: bool,
    /// 마지막 요청이 예산(`intel.budget_ms`)을 넘겼으면 (후보 수, ms) — 호스트가 로그 한 줄.
    over_budget: Option<(usize, u128)>,
    /// 마지막 요청의 방언(확정 때 시그니처 조회).
    dialect: Option<Dialect>,
}

const MRU_MAX: usize = 20;
/// 문서 단어 상한(큰 문서 보호).
const DOC_WORDS_MAX: usize = 5000;

impl Intel {
    pub(crate) fn new(cfg: IntelCfg) -> Self {
        Intel {
            menu: ContextMenu::new(),
            cfg,
            cands: Vec::new(),
            ctx: None,
            tab: None,
            mru: Vec::new(),
            due: None,
            docs: HashMap::new(),
            accept: None,
            needs: Vec::new(),
            loading: false,
            over_budget: None,
            dialect: None,
        }
    }

    pub(crate) fn set_cfg(&mut self, cfg: IntelCfg) {
        if !cfg.enabled {
            self.close();
            self.docs.clear();
        }
        self.cfg = cfg;
    }

    pub(crate) fn cfg(&self) -> &IntelCfg {
        &self.cfg
    }

    pub(crate) fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    pub(crate) fn close(&mut self) {
        self.menu.close();
        self.cands.clear();
        self.ctx = None;
        self.tab = None;
        self.due = None;
        self.loading = false;
    }

    /// 탭 하나의 아웃라인(캐시 · 세대가 같으면 재계산 0 · 큰 파일 단계는 호출자가 거른다).
    pub(crate) fn outline_for(
        &mut self,
        tab: u64,
        rev: u64,
        text: &dyn Fn() -> String,
        dialect: Option<Dialect>,
    ) -> &Outline {
        let stale = self.docs.get(&tab).is_none_or(|d| d.rev != rev);
        if stale {
            let src = text();
            let ol = outline::outline(&src, dialect);
            let classes = nsql_script::lexer::classify(&src);
            let mut words: Vec<String> = Vec::new();
            for w in outline::words(&src, &classes) {
                if words.len() >= DOC_WORDS_MAX {
                    break;
                }
                let t = w.text;
                if t.len() < 2
                    || !t.as_bytes()[0].is_ascii_alphabetic()
                        && !matches!(t.as_bytes()[0], b'_' | b':' | b'&' | b'@')
                {
                    continue;
                }
                if !words.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                    words.push(t.to_string());
                }
            }
            self.docs.insert(
                tab,
                DocCache {
                    rev,
                    outline: ol,
                    words,
                },
            );
            // 캐시 상한 — 닫힌 탭의 항목이 남지 않게(탭 32개 넘으면 지금 탭만 남긴다).
            if self.docs.len() > 32 {
                self.docs.retain(|k, _| *k == tab);
            }
        }
        &self.docs.get(&tab).expect("just inserted").outline
    }

    /// 글자 하나가 편집기에 들어간 뒤 — 트리거 문자면 즉시(`true` = 지금 요청), 식별자 글자면 디바운스, 그 밖은 닫는다.
    pub(crate) fn after_char(&mut self, c: char, prefix_len: usize) -> bool {
        if !self.cfg.enabled {
            return false;
        }
        if self.cfg.triggers.contains(&c) {
            self.due = None;
            return self.cfg.auto;
        }
        let ident = c.is_alphanumeric() || c == '_' || c == '$' || c == '#';
        if !ident {
            self.close();
            return false;
        }
        if self.is_open() {
            // 열린 채 타이핑 = 바로 다시 거른다.
            return true;
        }
        if self.cfg.auto && prefix_len >= self.cfg.min_chars {
            if self.cfg.delay_ms == 0 {
                return true;
            }
            self.due = Some(Instant::now() + Duration::from_millis(self.cfg.delay_ms));
        }
        false
    }

    /// 디바운스 마감이 지났으면(틱) `true` — 호스트가 `request`를 부른다.
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        match self.due {
            Some(t) if now >= t => {
                self.due = None;
                true
            }
            _ => false,
        }
    }

    /// 다음 깨울 시각(틱 스케줄러).
    pub(crate) fn next_wake(&self) -> Option<Instant> {
        self.due
    }

    /// 후보 조립 + 팝업 열기. `tab` = 편집기 탭 id · `doc` = 본문 · `caret` = 바이트 오프셋 · `anchor` = 캐럿 아래 점 · `host` = 창.
    /// 반환 = 열렸는가. 즉시 채움 요청은 [`Intel::take_needs`]로.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request(
        &mut self,
        tab: u64,
        rev: u64,
        doc: &str,
        caret: usize,
        dialect: Option<Dialect>,
        meta: Option<&MetaView<'_>>,
        anchor: Option<Point>,
        host: Rect,
        scale: f32,
    ) -> bool {
        if !self.cfg.enabled {
            return false;
        }
        let t0 = Instant::now();
        self.needs.clear();
        self.loading = false;
        let ctx = intel::context_at(doc, caret, dialect);
        if ctx.kind == CtxKind::None {
            self.close();
            return false;
        }
        // 문서 캐시(심볼 · 단어).
        self.outline_for(tab, rev, &|| doc.to_string(), dialect);
        let (symbols, doc_words): (Vec<(String, SymKind)>, Vec<String>) = {
            let d = self.docs.get(&tab).expect("cache");
            (d.outline.names(), d.words.clone())
        };
        let mut cands: Vec<Cand> = Vec::new();
        let show_types = self.cfg.show_types;
        let push_col = |cands: &mut Vec<Cand>, c: &nsql_run::meta::ColEntry, names: &Interner| {
            cands.push(Cand {
                text: names.get(c.name).to_string(),
                kind: CandKind::Column,
                detail: if show_types {
                    col_detail(names.get(c.data_type), c.key, c.nullable)
                } else {
                    String::new()
                },
                source: 1,
            });
        };
        let functions = self.cfg.functions;
        match &ctx.kind {
            CtxKind::None => unreachable!(),
            CtxKind::Member { qualifier } => {
                // 1) alias/테이블 → 컬럼.
                let mut resolved = false;
                if let Some(a) = intel::resolve_alias(&ctx.aliases, qualifier) {
                    // CTE/서브쿼리 = 카탈로그 없음 → 아래 문서 단어로(열 이름이 문서 안에 있다).
                    resolved = !a.local;
                    if a.local {
                    } else if let Some(m) = meta {
                        match m.snap.lookup(m.names, a.schema.as_deref(), &a.table) {
                            Some(id) => match m.snap.columns(id) {
                                ColState::Loaded { cols, .. } => {
                                    for c in cols.iter() {
                                        push_col(&mut cands, c, m.names);
                                    }
                                }
                                ColState::Loading => self.loading = true,
                                _ => {
                                    self.loading = true;
                                    self.needs.push(NeedColumns {
                                        schema: a.schema.clone().or_else(|| {
                                            m.snap
                                                .current_schema
                                                .map(|s| m.names.get(s).to_string())
                                        }),
                                        table: a.table.clone(),
                                    });
                                }
                            },
                            None => {
                                // 카탈로그에 없는 이름(`sys.` · `DBMS_OUTPUT.` · 아직 안 읽은 스키마) — 채움은 요청하되 아래
                                // 정적 표·문서 단어로 이어지게 둔다.
                                resolved = false;
                                self.needs.push(NeedColumns {
                                    schema: a.schema.clone(),
                                    table: a.table.clone(),
                                });
                            }
                        }
                    }
                }
                // 2) 스키마 이름 → 그 스키마 객체.
                if let Some(m) = meta {
                    let q = qualifier.rsplit('.').next().unwrap_or(qualifier);
                    if let Some(sc) = m
                        .snap
                        .schemas
                        .iter()
                        .copied()
                        .find(|s| m.names.get(*s).eq_ignore_ascii_case(q))
                    {
                        for kind in [
                            ObjectKind::Table,
                            ObjectKind::View,
                            ObjectKind::Synonym,
                            ObjectKind::Procedure,
                            ObjectKind::Function,
                            ObjectKind::Package,
                            ObjectKind::Sequence,
                        ] {
                            for h in m.snap.prefix(m.names, sc, kind, "", self.cfg.max_items) {
                                cands.push(Cand {
                                    text: m.names.get(h.name).to_string(),
                                    kind: cand_kind(kind),
                                    detail: if show_types {
                                        kind_label(kind).to_string()
                                    } else {
                                        String::new()
                                    },
                                    source: 3,
                                });
                            }
                        }
                        resolved = true;
                    } else if !resolved {
                        // 테이블 이름을 직접 쓴 경우(alias 없이).
                        if let Some(id) = m.snap.lookup(m.names, None, q) {
                            resolved = true;
                            match m.snap.columns(id) {
                                ColState::Loaded { cols, .. } => {
                                    for c in cols.iter() {
                                        push_col(&mut cands, c, m.names);
                                    }
                                }
                                ColState::Loading => self.loading = true,
                                _ => {
                                    self.loading = true;
                                    self.needs.push(NeedColumns {
                                        schema: None,
                                        table: q.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
                // 3) 시스템 패키지(`DBMS_OUTPUT.`) · 사전 객체의 앞부분(`sys.` · `INFORMATION_SCHEMA.` · `pg_catalog.`) — 정적 표(T-178).
                if !resolved && functions {
                    if let Some(p) = builtins::package(dialect, qualifier) {
                        resolved = true;
                        for m in p.members {
                            cands.push(Cand {
                                text: m.name.to_string(),
                                kind: CandKind::Function,
                                detail: if show_types {
                                    m.sig.to_string()
                                } else {
                                    String::new()
                                },
                                source: 4,
                            });
                        }
                    }
                    let sys = builtins::system_members(dialect, qualifier);
                    if !sys.is_empty() {
                        resolved = true;
                        for n in sys {
                            cands.push(Cand {
                                text: n.to_string(),
                                kind: CandKind::View,
                                detail: if show_types {
                                    "system".into()
                                } else {
                                    String::new()
                                },
                                source: 4,
                            });
                        }
                    }
                }
                if !resolved && self.cfg.doc_words {
                    for w in &doc_words {
                        cands.push(Cand {
                            text: w.clone(),
                            kind: CandKind::Word,
                            detail: String::new(),
                            source: 6,
                        });
                    }
                }
            }
            CtxKind::Relation => {
                if let Some(m) = meta {
                    if let Some(cur) = m.snap.current_schema {
                        for kind in [ObjectKind::Table, ObjectKind::View, ObjectKind::Synonym] {
                            for h in m.snap.prefix(m.names, cur, kind, "", self.cfg.max_items) {
                                cands.push(Cand {
                                    text: m.names.get(h.name).to_string(),
                                    kind: cand_kind(kind),
                                    detail: if show_types {
                                        kind_label(kind).to_string()
                                    } else {
                                        String::new()
                                    },
                                    source: 3,
                                });
                            }
                        }
                    }
                    for s in &m.snap.schemas {
                        cands.push(Cand {
                            text: m.names.get(*s).to_string(),
                            kind: CandKind::Schema,
                            detail: if show_types {
                                "schema".into()
                            } else {
                                String::new()
                            },
                            source: 4,
                        });
                    }
                }
                for a in ctx.aliases.iter().filter(|a| a.local && a.alias == a.table) {
                    cands.push(Cand {
                        text: a.alias.clone(),
                        kind: CandKind::Table,
                        detail: if show_types {
                            "cte".into()
                        } else {
                            String::new()
                        },
                        source: 2,
                    });
                }
                for (name, k) in &symbols {
                    if *k == SymKind::Object || *k == SymKind::Cte {
                        cands.push(Cand {
                            text: name.clone(),
                            kind: CandKind::Symbol,
                            detail: if show_types {
                                k.label().to_string()
                            } else {
                                String::new()
                            },
                            source: 3,
                        });
                    }
                }
                // 사전 객체(`ALL_TABLES` · `V$SESSION` · `sys.tables` · `pg_catalog.pg_class`) — 정적 표(T-178).
                if functions {
                    for o in builtins::system_objects(dialect) {
                        cands.push(Cand {
                            text: (*o).to_string(),
                            kind: CandKind::View,
                            detail: if show_types {
                                "system".into()
                            } else {
                                String::new()
                            },
                            source: 4,
                        });
                    }
                }
            }
            CtxKind::Expr | CtxKind::Start => {
                if ctx.kind == CtxKind::Expr
                    && self.cfg.insert_columns
                    && ctx.paren_into
                    && ctx.prefix.is_empty()
                {
                    // `INSERT INTO t (` 바로 뒤 = 그 테이블의 컬럼 전체를 한 조각으로(첫 후보 · T-178).
                    if let (Some(owner), Some(m)) = (ctx.paren_owner.as_deref(), meta) {
                        let (schema, table) = match intel::resolve_alias(&ctx.aliases, owner) {
                            Some(a) if !a.local => (a.schema.clone(), a.table.clone()),
                            _ => match owner.rsplit_once('.') {
                                Some((sc, t)) => (Some(sc.to_string()), t.to_string()),
                                None => (None, owner.to_string()),
                            },
                        };
                        match m.snap.lookup(m.names, schema.as_deref(), &table) {
                            Some(id) => match m.snap.columns(id) {
                                ColState::Loaded { cols, .. } if !cols.is_empty() => {
                                    let list: Vec<&str> =
                                        cols.iter().map(|c| m.names.get(c.name)).collect();
                                    cands.push(Cand {
                                        text: list.join(", "),
                                        kind: CandKind::Snippet,
                                        detail: nsql_i18n::t(nsql_i18n::Msg::IntelAllColumns)
                                            .to_string(),
                                        source: 0,
                                    });
                                }
                                ColState::Loading => self.loading = true,
                                ColState::Loaded { .. } | ColState::Error(_) => {}
                                ColState::Unknown => {
                                    self.loading = true;
                                    self.needs.push(NeedColumns { schema, table });
                                }
                            },
                            None => self.needs.push(NeedColumns { schema, table }),
                        }
                    }
                }
                if ctx.kind == CtxKind::Expr {
                    // 문장 alias 컬럼(로드된 것만) + alias 이름.
                    for a in &ctx.aliases {
                        cands.push(Cand {
                            text: a.alias.clone(),
                            kind: CandKind::Alias,
                            detail: if show_types {
                                if a.local {
                                    "cte/subquery".into()
                                } else {
                                    a.table.clone()
                                }
                            } else {
                                String::new()
                            },
                            source: 2,
                        });
                        if a.local {
                            continue;
                        }
                        if let Some(m) = meta {
                            if let Some(id) = m.snap.lookup(m.names, a.schema.as_deref(), &a.table)
                            {
                                if let ColState::Loaded { cols, .. } = m.snap.columns(id) {
                                    for c in cols.iter() {
                                        push_col(&mut cands, c, m.names);
                                    }
                                }
                            }
                        }
                    }
                    // 내장 함수(시그니처) + 시스템 패키지 이름 — 정적 표(T-178 · `intel.functions`).
                    if functions {
                        for b in builtins::functions(dialect) {
                            cands.push(Cand {
                                text: b.name.to_string(),
                                kind: CandKind::Function,
                                detail: if show_types {
                                    b.sig.to_string()
                                } else {
                                    String::new()
                                },
                                source: 4,
                            });
                        }
                        for p in builtins::packages(dialect) {
                            cands.push(Cand {
                                text: p.name.to_string(),
                                kind: CandKind::Package,
                                detail: if show_types {
                                    "package".into()
                                } else {
                                    String::new()
                                },
                                source: 4,
                            });
                        }
                    }
                }
                for (name, k) in &symbols {
                    cands.push(Cand {
                        text: name.clone(),
                        kind: match k {
                            SymKind::Define | SymKind::Variable | SymKind::Declared => {
                                CandKind::Variable
                            }
                            SymKind::Procedure | SymKind::Function => CandKind::Routine,
                            _ => CandKind::Symbol,
                        },
                        detail: if show_types {
                            k.label().to_string()
                        } else {
                            String::new()
                        },
                        source: 3,
                    });
                }
                if self.cfg.keywords {
                    for k in intel::KEYWORDS {
                        cands.push(Cand {
                            text: (*k).to_string(),
                            kind: CandKind::Keyword,
                            detail: String::new(),
                            source: 5,
                        });
                    }
                }
                if self.cfg.doc_words {
                    for w in &doc_words {
                        cands.push(Cand {
                            text: w.clone(),
                            kind: CandKind::Word,
                            detail: String::new(),
                            source: 6,
                        });
                    }
                }
            }
        }
        // 접두를 뺀 문서 단어(자기 자신)는 후보에서 뺀다.
        let prefix = ctx.prefix.clone();
        cands.retain(|c| !(c.kind == CandKind::Word && c.text.eq_ignore_ascii_case(&prefix)));
        let mru: &[String] = if self.cfg.recent { &self.mru } else { &[] };
        let mut ranked = intel::rank(cands, &prefix, self.cfg.mode, mru, self.cfg.max_items);
        // 조각(컬럼 목록)은 MRU와 무관하게 맨 위(안정 정렬).
        ranked.sort_by_key(|c| c.kind != CandKind::Snippet);
        if ranked.is_empty() && !self.loading {
            self.close();
            return false;
        }
        // 팝업 = 랭킹 전부(≤ `intel.max_items`) · 보이는 행은 `intel.popup_rows`까지 → 넘치면 스크롤(nexa-ui 71차 `set_max_rows` · T-178).
        let mut items: Vec<CtxItem> = ranked
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let label = if c.kind == CandKind::Snippet && c.text.len() > 48 {
                    let cut = c
                        .text
                        .char_indices()
                        .nth(45)
                        .map_or(c.text.len(), |(b, _)| b);
                    format!("{}…", &c.text[..cut])
                } else {
                    c.text.clone()
                };
                CtxItem::item(format!("intel:{i}"), label).with_shortcut(c.detail.clone())
            })
            .collect();
        if self.loading {
            items.push(CtxItem::maybe(
                "intel:loading",
                nsql_i18n::t(nsql_i18n::Msg::StIntelLoading),
                false,
            ));
        }
        self.cands = ranked;
        self.ctx = Some(ctx);
        self.tab = Some(tab);
        self.dialect = dialect;
        let p = anchor.unwrap_or(Point {
            x: host.x,
            y: host.y,
        });
        self.menu.set_scale(scale);
        self.menu.set_max_rows(Some(self.cfg.rows));
        self.menu
            .open_at(p.x, p.y, items, host, (280.0 * scale) as i32);
        self.due = None;
        let ms = t0.elapsed().as_millis();
        self.over_budget = (ms > u128::from(self.cfg.budget_ms)).then_some((self.cands.len(), ms));
        true
    }

    /// 마지막 요청이 예산을 넘겼으면 (후보 수, ms) — 한 번만 돌려준다(호스트가 개발자 로그에).
    pub(crate) fn take_over_budget(&mut self) -> Option<(usize, u128)> {
        self.over_budget.take()
    }

    /// 시그니처 도움(`intel.signature_help` · T-178): 캐럿을 감싸는 안 닫힌 `(`의 주인이 내장 함수·패키지 멤버면 그 시그니처.
    pub(crate) fn signature_at(
        &self,
        doc: &str,
        caret: usize,
        dialect: Option<Dialect>,
    ) -> Option<String> {
        if !self.cfg.enabled || !self.cfg.signature_help {
            return None;
        }
        let ctx = intel::context_at(doc, caret, dialect);
        let owner = ctx.paren_owner.as_deref()?;
        builtins::signature(dialect, owner).map(str::to_string)
    }

    /// 마지막 요청이 남긴 즉시 채움 요청(호스트가 탐색기 메타 큐에 넣는다).
    pub(crate) fn take_needs(&mut self) -> Vec<NeedColumns> {
        std::mem::take(&mut self.needs)
    }

    /// 팝업이 고른 항목 → 확정(편집기 적용은 호스트).
    pub(crate) fn pick(&mut self, id: &str) {
        let Some(i) = id
            .strip_prefix("intel:")
            .and_then(|n| n.parse::<usize>().ok())
        else {
            return;
        };
        let (Some(c), Some(ctx)) = (self.cands.get(i), self.ctx.as_ref()) else {
            return;
        };
        let mut text = intel::apply_case(&c.text, &ctx.prefix, &self.cfg.insert_case);
        let mut caret_back = 0;
        match c.kind {
            // 함수 = `NAME()` + 캐럿 안(인자가 없는 값 `SYSDATE`·`@@ROWCOUNT`는 그대로 · 시그니처의 괄호 유무로 판단).
            CandKind::Function if self.cfg.insert_parens => {
                let sig = builtins::signature(self.dialect, &c.text);
                let has_parens = sig.is_none_or(|s| s.contains('('));
                if has_parens && !text.ends_with('(') {
                    text.push_str("()");
                    if !sig.is_some_and(|s| s.contains("()")) {
                        caret_back = 1;
                    }
                }
            }
            CandKind::Keyword if self.cfg.insert_space => {
                if !text.ends_with('(') {
                    text.push(' ');
                }
            }
            CandKind::Table | CandKind::View | CandKind::Synonym
                if self.cfg.insert_alias && ctx.kind == CtxKind::Relation =>
            {
                // 치던 접두 자체가 alias 표에 테이블로 잡혀 있으면(`FROM sc|`) 그것은 제외.
                let taken: Vec<&str> = ctx
                    .aliases
                    .iter()
                    .filter(|a| {
                        !(a.alias.eq_ignore_ascii_case(&ctx.prefix)
                            && a.table.eq_ignore_ascii_case(&ctx.prefix))
                    })
                    .map(|a| a.alias.as_str())
                    .collect();
                let alias = gen_alias(&c.text, &taken);
                text.push(' ');
                text.push_str(&alias);
            }
            _ => {}
        }
        if c.kind != CandKind::Snippet {
            self.mru.retain(|m| !m.eq_ignore_ascii_case(&c.text));
            self.mru.insert(0, c.text.clone());
            self.mru.truncate(MRU_MAX);
        }
        self.accept = Some(Accept {
            replace: ctx.replace.clone(),
            text,
            caret_back,
        });
        self.close();
    }

    pub(crate) fn take_accept(&mut self) -> Option<Accept> {
        self.accept.take()
    }

    /// 팝업이 열린 탭.
    pub(crate) fn tab(&self) -> Option<u64> {
        self.tab
    }
}

/// 컬럼 오른쪽 열: `PK NUMBER(4)` · `FK VARCHAR2(10) NOT NULL` · `UQ …`.
fn col_detail(ty: &str, key: u8, nullable: Option<bool>) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if key & nsql_run::meta::KEY_PK != 0 {
        parts.push("PK");
    } else if key & nsql_run::meta::KEY_FK != 0 {
        parts.push("FK");
    } else if key & nsql_run::meta::KEY_UQ != 0 {
        parts.push("UQ");
    }
    if !ty.is_empty() {
        parts.push(ty);
    }
    if nullable == Some(false) && key & nsql_run::meta::KEY_PK == 0 {
        parts.push("NOT NULL");
    }
    parts.join(" ")
}

/// 테이블 이름 → 짧은 alias(`sales_customer` → `sc` · `emp` → `e` · `MyTable` → `mt`) · 문장 안 alias와 겹치면 숫자 붙임 ·
/// 키워드 모양(`in`·`as`·`or` …)이면 `1`을 붙인다(`intel.insert_alias` · T-178).
fn gen_alias(name: &str, taken: &[&str]) -> String {
    let base = name.rsplit('.').next().unwrap_or(name);
    let mut a = String::new();
    let mut prev = '_';
    for ch in base.chars() {
        if ch.is_alphanumeric()
            && (prev == '_'
                || prev == '$'
                || prev == '#'
                || (ch.is_uppercase() && prev.is_lowercase()))
        {
            a.push(ch.to_ascii_lowercase());
        }
        prev = ch;
    }
    if a.is_empty() {
        a = base.chars().take(1).collect::<String>().to_lowercase();
    }
    if a.is_empty() {
        a.push('t');
    }
    const KW: &[&str] = &[
        "in", "as", "or", "on", "by", "is", "if", "to", "at", "do", "go", "no",
    ];
    if KW.contains(&a.as_str()) {
        a.push('1');
    }
    let mut out = a.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&out)) {
        out = format!("{a}{n}");
        n += 1;
    }
    out
}

/// 객체 종류 라벨(팝업 오른쪽 열 · 소문자).
fn kind_label(k: ObjectKind) -> String {
    format!("{k:?}").to_lowercase()
}

fn cand_kind(k: ObjectKind) -> CandKind {
    match k {
        ObjectKind::Table => CandKind::Table,
        ObjectKind::View => CandKind::View,
        ObjectKind::Synonym => CandKind::Synonym,
        ObjectKind::Sequence => CandKind::Sequence,
        ObjectKind::Package => CandKind::Package,
        ObjectKind::Procedure | ObjectKind::Function => CandKind::Routine,
        _ => CandKind::Symbol,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_run::meta::{MetaStore, NewCol, NewObj};

    fn cfg() -> IntelCfg {
        IntelCfg {
            enabled: true,
            auto: true,
            delay_ms: 250,
            triggers: vec!['.'],
            min_chars: 2,
            mode: MatchMode::Fuzzy,
            recent: true,
            max_items: 200,
            rows: 12,
            keywords: true,
            doc_words: true,
            insert_case: "default".into(),
            show_types: true,
            functions: true,
            insert_parens: true,
            insert_alias: true,
            insert_space: true,
            insert_columns: true,
            signature_help: true,
            budget_ms: 30,
        }
    }

    fn store() -> MetaStore {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["SCOTT".into(), "HR".into()], Some("SCOTT"));
        m.load_bucket(
            "SCOTT",
            ObjectKind::Table,
            &[
                NewObj {
                    name: "EMP".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "DEPT".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "SALES_CUSTOMER".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        let id = m
            .snapshot()
            .lookup(&m.names, Some("SCOTT"), "EMP")
            .expect("emp");
        m.set_columns(
            id,
            &[
                NewCol {
                    name: "EMPNO".into(),
                    data_type: "NUMBER(4)".into(),
                    key: nsql_run::meta::KEY_PK,
                    ..Default::default()
                },
                NewCol {
                    name: "ENAME".into(),
                    data_type: "VARCHAR2(10)".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        m
    }

    #[test]
    fn member_columns_relation_tables_and_loading_request() {
        let m = store();
        let snap = m.snapshot();
        let view = MetaView {
            names: &m.names,
            snap,
        };
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let doc = "SELECT e. FROM emp e, dept d";
        assert!(it.request(
            1,
            1,
            doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            None,
            host,
            1.0
        ));
        let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["EMPNO", "ENAME"]);
        assert!(it.cands[0].detail.contains("PK"));
        // dept = 컬럼 없음 → 즉시 채움 요청 + 불러오는 중.
        let doc = "SELECT d.| FROM emp e, dept d".replace('|', "");
        assert!(it.request(
            1,
            1,
            &doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            None,
            host,
            1.0
        ));
        assert!(it.loading);
        assert_eq!(
            it.take_needs(),
            vec![NeedColumns {
                schema: Some("SCOTT".into()),
                table: "dept".into()
            }]
        );
        // FROM 뒤 = 테이블 + 스키마 · 접두 sc = 퍼지(sales_customer) + 스키마 SCOTT.
        let doc = "SELECT * FROM sc";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            Some(Dialect::Oracle),
            Some(&view),
            None,
            host,
            1.0
        ));
        let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert!(
            texts.contains(&"SCOTT") && texts.contains(&"SALES_CUSTOMER"),
            "{texts:?}"
        );
        // 확정 = 접두 구간 교체 · MRU.
        let i = texts
            .iter()
            .position(|t| *t == "SALES_CUSTOMER")
            .expect("sc");
        it.pick(&format!("intel:{i}"));
        let a = it.take_accept().expect("accept");
        assert_eq!(
            (a.replace.clone(), a.text.as_str()),
            (14..16, "SALES_CUSTOMER sc"),
            "insert_alias = 짧은 alias까지"
        );
        assert!(!it.is_open());
        assert_eq!(it.mru[0], "SALES_CUSTOMER");
    }

    /// T-178: 내장 함수 = `NAME()` + 캐럿 안 · 패키지 멤버 · 사전 객체 · 시그니처 도움 · INSERT 컬럼 목록 조각 · 컬럼 상세 · alias.
    #[test]
    fn builtins_snippet_and_signature() {
        let m = store();
        let snap = m.snapshot();
        let view = MetaView {
            names: &m.names,
            snap,
        };
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let ora = Some(Dialect::Oracle);
        // 식 자리 `nv` → NVL(시그니처) · 확정 = `NVL()` 캐럿 1 뒤로.
        let doc = "SELECT nv FROM emp";
        assert!(it.request(1, 1, doc, 9, ora, Some(&view), None, host, 1.0));
        let i = it.cands.iter().position(|c| c.text == "NVL").expect("nvl");
        assert_eq!(it.cands[i].detail, "NVL(expr1, expr2)");
        it.pick(&format!("intel:{i}"));
        let a = it.take_accept().expect("accept");
        assert_eq!((a.text.as_str(), a.caret_back), ("NVL()", 1));
        // 인자 없는 값 = 괄호 없이.
        let doc = "SELECT sysd";
        assert!(it.request(1, 1, doc, doc.len(), ora, Some(&view), None, host, 1.0));
        let i = it
            .cands
            .iter()
            .position(|c| c.text == "SYSDATE")
            .expect("sysdate");
        it.pick(&format!("intel:{i}"));
        assert_eq!(it.take_accept().expect("a").text, "SYSDATE");
        // 패키지 멤버 `DBMS_OUTPUT.` → PUT_LINE …(문서 단어로 떨어지지 않음).
        let doc = "BEGIN DBMS_OUTPUT.";
        assert!(it.request(1, 1, doc, doc.len(), ora, Some(&view), None, host, 1.0));
        assert!(it
            .cands
            .iter()
            .any(|c| c.text == "PUT_LINE" && c.kind == CandKind::Function));
        assert!(!it.cands.iter().any(|c| c.kind == CandKind::Word));
        // 사전 객체: FROM 뒤 `all_t` → ALL_TABLES · MSSQL `sys.` → tables.
        let doc = "SELECT * FROM all_t";
        assert!(it.request(1, 1, doc, doc.len(), ora, Some(&view), None, host, 1.0));
        assert!(it.cands.iter().any(|c| c.text == "ALL_TABLES"));
        let doc = "SELECT * FROM sys.";
        let ms = Some(Dialect::Mssql);
        assert!(it.request(1, 1, doc, doc.len(), ms, Some(&view), None, host, 1.0));
        assert!(it
            .cands
            .iter()
            .any(|c| c.text == "tables" && c.kind == CandKind::View));
        // 키워드 + 공백.
        assert!(it.request(1, 1, "sel", 3, ora, Some(&view), None, host, 1.0));
        it.pick("intel:0");
        assert_eq!(it.take_accept().expect("kw").text, "SELECT ");
        // 시그니처 도움.
        assert_eq!(
            it.signature_at("SELECT NVL(a, ", 14, ora).as_deref(),
            Some("NVL(expr1, expr2)")
        );
        assert_eq!(it.signature_at("SELECT foo(", 11, ora), None);
        // INSERT INTO emp ( → 첫 후보 = 컬럼 전체 조각 · MRU에 안 남음.
        let doc = "INSERT INTO emp (";
        assert!(it.request(1, 1, doc, doc.len(), ora, Some(&view), None, host, 1.0));
        assert_eq!(it.cands[0].kind, CandKind::Snippet);
        assert_eq!(it.cands[0].text, "EMPNO, ENAME");
        it.pick("intel:0");
        assert_eq!(it.take_accept().expect("snip").text, "EMPNO, ENAME");
        assert_ne!(it.mru[0], "EMPNO, ENAME");
        // 컬럼 상세 · alias 생성.
        assert_eq!(
            col_detail("NUMBER(4)", nsql_run::meta::KEY_PK, Some(false)),
            "PK NUMBER(4)"
        );
        assert_eq!(
            col_detail("VARCHAR2(10)", nsql_run::meta::KEY_FK, Some(false)),
            "FK VARCHAR2(10) NOT NULL"
        );
        assert_eq!(col_detail("DATE", 0, Some(true)), "DATE");
        assert_eq!(gen_alias("sales_customer", &[]), "sc");
        assert_eq!(gen_alias("EMP", &["e"]), "e2");
        assert_eq!(gen_alias("MyTable", &[]), "mt");
        assert_eq!(gen_alias("orders", &[]), "o");
        assert_eq!(gen_alias("sch.item_set", &[]), "is1", "키워드 모양이면 1");
        assert!(it.take_over_budget().is_none());
    }

    #[test]
    fn document_symbols_keywords_and_triggers() {
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let doc = "DEFINE v_user = 'x'\nVARIABLE rc REFCURSOR\nSELECT v_ FROM dual";
        let caret = doc.rfind("v_").expect("v_") + 2;
        assert!(it.request(
            7,
            3,
            doc,
            caret,
            Some(Dialect::Oracle),
            None,
            None,
            host,
            1.0
        ));
        let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts[0], "v_user");
        assert!(it.cands[0].kind == CandKind::Variable);
        // 문장 시작 = 키워드.
        assert!(it.request(7, 3, "sel", 3, Some(Dialect::Oracle), None, None, host, 1.0));
        assert_eq!(it.cands[0].text, "SELECT");
        // 트리거 규칙.
        assert!(it.after_char('.', 0), "트리거 문자 = 즉시");
        it.close();
        assert!(!it.after_char('a', 1), "min_chars 미만 = 아직");
        assert!(it.due.is_none());
        assert!(!it.after_char('b', 2), "지연 예약");
        assert!(it.due.is_some());
        assert!(it.due(Instant::now() + Duration::from_millis(300)));
        assert!(!it.after_char(' ', 0));
        // 문자열 안 = 안 뜬다.
        assert!(!it.request(
            7,
            3,
            "SELECT 'a",
            9,
            Some(Dialect::Oracle),
            None,
            None,
            host,
            1.0
        ));
    }
}
