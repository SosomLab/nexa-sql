//! App — ★ **Ctrl 객체 하이퍼링크 + 설명 툴팁**(docs/77 §1-3 객체 척추 · docs/96 · T-256 · 사용자 09-29).
//!
//! Ctrl(맥 ⌘)을 누르는 동안: 활성 편집기의 SQL을 **위치·별칭 기준으로 분석**해 테이블/뷰 · 컬럼 · 프로시저/함수/패키지 참조를
//! 골라 링크로 삼고, 커서 아래 링크는 **설명(코멘트) 툴팁**(대상 우상단 · 설정 `objlink.tooltip_pos`) · 좌클릭 = 설명
//! 클립보드 복사(뒤에 다른 동작으로 바뀔 수 있음) · 우클릭 = 메뉴("설명 복사").
//!
//! **표시**(`objlink.display` · 사용자 09-29 2차): `all` = Ctrl 동안 전부 밑줄 · `hover`(기본) = 마우스 아래 링크 하나만 · `none` =
//! 그리지 않고 동작만. 링크는 **정상**(현재 연결 메타에서 확인 · `objlink.line_*`)과 **미확인**(객체 자리인데 메타에 없음 → 설명을
//! 가져올 수 없음 · `objlink.bad_*` 기본 = 밝은 벽돌색 물결)으로 나눠 색·두께·모양을 따로 그린다. 향상 모드(`perf::BOOST`)와 큰 파일
//! 모드는 표시를 `none`으로 고정한다(다시 그리기 0 · 기능은 유지). 큰 파일 모드 또는 `objlink.max_kb` 초과 문서는 **보이는 구간**
//! (문장 경계까지 넓힘)만 분석한다. 메타는 탐색기 스냅숏(단일 원천)만 읽고, 없는 컬럼은 요청해 둔다(지연 · 61 §1-8).

use crate::*;
use nexa_ctl::controls::ctxmenu::CtxItem;
use nexa_ctl::{LinkLine, LinkStyle};
use nsql_run::meta::{ColState, DetailState, Interner, Snapshot};
use nsql_script::intel::{context_at, resolve_alias, Alias};

/// 링크 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkKind {
    Table,
    Column,
    Routine,
}

/// 링크 하나(문자 인덱스 구간).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub range: (usize, usize),
    pub kind: LinkKind,
    pub schema: Option<String>,
    /// 컬럼 = 소속 테이블(별칭 풀이) · 루틴 = 패키지.
    pub owner: Option<String>,
    pub name: String,
    /// 현재 연결의 메타(카탈로그)에서 확인됐는가 — 아니면 미확인 스타일(설명 없음).
    pub known: bool,
}

impl Link {
    /// 표시 이름(`schema.owner.name` 중 있는 것 · `with_schema` = false면 스키마 접두 생략 — 설정 `objlink.show_schema` 기본 끔).
    pub(crate) fn qualified(&self, with_schema: bool) -> String {
        let mut s = String::new();
        if let (true, Some(sc)) = (with_schema, &self.schema) {
            s.push_str(sc);
            s.push('.');
        }
        if let Some(o) = &self.owner {
            s.push_str(o);
            s.push('.');
        }
        s.push_str(&self.name);
        s
    }
}

/// 객체 분류(메타 해석기가 돌려주는 것).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjClass {
    Relation,
    Routine,
    Package,
    Other,
}

/// 메타 해석 포트(순수 스캔이 호스트 메타를 묻는 창 · 시험 = 가짜 구현).
pub(crate) trait Resolver {
    /// 이름의 분류(메타에 없으면 None).
    fn object_class(&self, schema: Option<&str>, name: &str) -> Option<ObjClass>;
    /// 그 테이블에 이 컬럼이 있는가 — `Some(있다/없다)` · `None` = 컬럼 목록을 아직 모른다(호스트가 뒤에서 요청한다).
    fn has_column(&self, schema: Option<&str>, table: &str, col: &str) -> Option<bool>;
}

/// 표시 방식(`objlink.display`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Display {
    All,
    #[default]
    Hover,
    None,
}

impl Display {
    fn parse(s: &str) -> Display {
        match s {
            "all" => Display::All,
            "none" => Display::None,
            _ => Display::Hover,
        }
    }
}

const RELATION_AFTER: &[&str] = &[
    "FROM", "JOIN", "INTO", "UPDATE", "TABLE", "DESC", "DESCRIBE", "TRUNCATE", "USING",
];

/// ★ 문서 → 링크 목록(순수 · 토큰 = nsql-format 렉서 · 별칭 = nsql-script `context_at` 문장별 1회). 범위 = `text` 안의 문자 인덱스.
pub(crate) fn scan(
    text: &str,
    dialect: Option<nsql_core::Dialect>,
    res: &dyn Resolver,
) -> Vec<Link> {
    use nsql_format::{is_keyword, Kind};
    let toks = nsql_format::lex(text);
    let mut out = Vec::new();
    // 바이트 → 문자 인덱스(증분 커서).
    let mut cb = 0usize;
    let mut cc = 0usize;
    let mut char_at = |b: usize| -> usize {
        if b < cb {
            cb = 0;
            cc = 0;
        }
        cc += text[cb..b].chars().count();
        cb = b;
        cc
    };
    // 문장 문맥 캐시(별칭 표 · 문장 구간).
    let mut stmt: Option<(std::ops::Range<usize>, Vec<Alias>)> = None;
    // 절 추적(괄호 깊이별 마지막 절).
    let mut clause: Vec<Option<String>> = vec![None];
    let sig = |k: usize| -> Option<&nsql_format::Token> {
        // k 앞의 주석 아닌 토큰.
        let mut j = k;
        while j > 0 {
            j -= 1;
            if !toks[j].is_comment() {
                return Some(&toks[j]);
            }
        }
        None
    };
    let next_sig = |k: usize| -> Option<&nsql_format::Token> {
        let mut j = k + 1;
        while j < toks.len() {
            if !toks[j].is_comment() {
                return Some(&toks[j]);
            }
            j += 1;
        }
        None
    };
    let is_relation = |schema: Option<&str>, name: &str| {
        res.object_class(schema, name) == Some(ObjClass::Relation)
    };
    for (i, t) in toks.iter().enumerate() {
        match t.kind {
            Kind::Punct if t.text == "(" => {
                clause.push(None);
                continue;
            }
            Kind::Punct if t.text == ")" => {
                if clause.len() > 1 {
                    clause.pop();
                }
                continue;
            }
            Kind::Punct if t.text == ";" => {
                clause.clear();
                clause.push(None);
                continue;
            }
            Kind::Word => {}
            _ => continue,
        }
        let up = t.up();
        if is_keyword(&up) {
            if matches!(
                up.as_str(),
                "SELECT"
                    | "FROM"
                    | "WHERE"
                    | "GROUP"
                    | "HAVING"
                    | "ORDER"
                    | "SET"
                    | "VALUES"
                    | "INTO"
                    | "UPDATE"
                    | "DELETE"
                    | "MERGE"
                    | "USING"
                    | "ON"
                    | "JOIN"
                    | "WITH"
            ) {
                if let Some(c) = clause.last_mut() {
                    *c = Some(up.clone());
                }
            }
            continue;
        }
        // 다음이 `.`+이름이면 이 토큰은 한정자 — 마지막 이름에서 다룬다.
        if next_sig(i).is_some_and(|n| n.is_punct("."))
            && toks.get(i + 2).is_some_and(|n| n.kind == Kind::Word)
        {
            continue;
        }
        // 한정자 사슬(최대 둘).
        let mut quals: Vec<String> = Vec::new();
        let mut k = i;
        while k >= 2 && toks[k - 1].is_punct(".") && toks[k - 2].kind == Kind::Word {
            quals.insert(0, toks[k - 2].text.clone());
            k -= 2;
            if quals.len() == 2 {
                break;
            }
        }
        let head = k; // 사슬의 첫 토큰 index
        let prev = sig(head);
        let prev_up = prev.map(|p| p.up()).unwrap_or_default();
        // 별칭 정의 자리(`AS x` · `table x`)는 링크가 아니다 — 아래 규칙에 안 걸리면 자연히 빠진다.
        if prev_up == "AS" {
            continue;
        }
        // 문장 별칭 표.
        let b = t.span.0;
        let aliases: &[Alias] = {
            let fresh = match &stmt {
                Some((r, _)) => !r.contains(&b),
                None => true,
            };
            if fresh {
                let c = context_at(text, b, dialect);
                let r = if c.statement.is_empty() {
                    b..text.len()
                } else {
                    c.statement.clone()
                };
                stmt = Some((r, c.aliases));
            }
            &stmt.as_ref().expect("stmt").1
        };
        let cur_clause = clause.last().cloned().flatten();
        let relation_pos = (prev.is_some_and(|p| p.kind == Kind::Word)
            && RELATION_AFTER.contains(&prev_up.as_str()))
            || (prev.is_some_and(|p| p.is_punct(",")) && cur_clause.as_deref() == Some("FROM"));
        let a0 = char_at(toks[head].span.0);
        let a1 = char_at(t.span.1);
        let name = t.text.clone();
        let is_local = |n: &str| {
            aliases
                .iter()
                .any(|a| a.local && a.table.eq_ignore_ascii_case(n))
        };
        if relation_pos {
            if is_local(&name)
                || (quals.is_empty()
                    && aliases
                        .iter()
                        .any(|a| a.alias.eq_ignore_ascii_case(&name) && a.local))
            {
                continue;
            }
            // USING (서브쿼리) · 함수 테이블 등은 제외.
            if next_sig(i).is_some_and(|n| n.is_punct("(")) && prev_up == "USING" {
                continue;
            }
            let (schema, owner) = match quals.len() {
                0 => (None, None),
                1 => (Some(quals[0].clone()), None),
                _ => (Some(quals[0].clone()), Some(quals[1].clone())),
            };
            if owner.is_some() {
                // `schema.pkg.fn` 꼴은 루틴.
                let known = res.object_class(schema.as_deref(), owner.as_deref().unwrap_or(""))
                    == Some(ObjClass::Package);
                out.push(Link {
                    range: (a0, a1),
                    kind: LinkKind::Routine,
                    schema,
                    owner,
                    name,
                    known,
                });
            } else {
                // 관계 자리 = 구조적으로 테이블/뷰 — 메타에 있으면 정상, 없으면 미확인(설명 없음 · 벽돌색).
                let known = is_relation(schema.as_deref(), &name);
                out.push(Link {
                    range: (a0, a1),
                    kind: LinkKind::Table,
                    schema,
                    owner: None,
                    name,
                    known,
                });
            }
            continue;
        }
        let followed_by_paren = next_sig(i).is_some_and(|n| n.is_punct("("));
        match quals.len() {
            2 => {
                // schema.table.col 또는 schema.pkg.proc.
                let (sc, mid) = (&quals[0], &quals[1]);
                match res.object_class(Some(sc), mid) {
                    Some(ObjClass::Relation) => {
                        let known = res.has_column(Some(sc), mid, &name) != Some(false);
                        out.push(Link {
                            range: (a0, a1),
                            kind: LinkKind::Column,
                            schema: Some(sc.clone()),
                            owner: Some(mid.clone()),
                            name,
                            known,
                        })
                    }
                    Some(ObjClass::Package) => out.push(Link {
                        range: (a0, a1),
                        kind: LinkKind::Routine,
                        schema: Some(sc.clone()),
                        owner: Some(mid.clone()),
                        name,
                        known: true,
                    }),
                    _ => {}
                }
            }
            1 => {
                let q = &quals[0];
                if let Some(a) = resolve_alias(aliases, q) {
                    if !a.local {
                        // 별칭의 테이블 컬럼: 테이블이 메타에 없거나 컬럼이 없으면 미확인 · 컬럼 목록을 아직 모르면 일단 정상.
                        let known = is_relation(a.schema.as_deref(), &a.table)
                            && res.has_column(a.schema.as_deref(), &a.table, &name) != Some(false);
                        out.push(Link {
                            range: (a0, a1),
                            kind: LinkKind::Column,
                            schema: a.schema.clone(),
                            owner: Some(a.table.clone()),
                            name,
                            known,
                        });
                    }
                    continue;
                }
                match res.object_class(None, q) {
                    Some(ObjClass::Package) => out.push(Link {
                        range: (a0, a1),
                        kind: LinkKind::Routine,
                        schema: None,
                        owner: Some(q.clone()),
                        name,
                        known: true,
                    }),
                    Some(ObjClass::Relation) => {
                        let known = res.has_column(None, q, &name) != Some(false);
                        out.push(Link {
                            range: (a0, a1),
                            kind: LinkKind::Column,
                            schema: None,
                            owner: Some(q.clone()),
                            name,
                            known,
                        })
                    }
                    _ => match res.object_class(Some(q), &name) {
                        Some(ObjClass::Relation) => out.push(Link {
                            range: (a0, a1),
                            kind: LinkKind::Table,
                            schema: Some(q.clone()),
                            owner: None,
                            name,
                            known: true,
                        }),
                        Some(ObjClass::Routine) => out.push(Link {
                            range: (a0, a1),
                            kind: LinkKind::Routine,
                            schema: Some(q.clone()),
                            owner: None,
                            name,
                            known: true,
                        }),
                        _ => {}
                    },
                }
            }
            _ => {
                if followed_by_paren {
                    if matches!(
                        res.object_class(None, &name),
                        Some(ObjClass::Routine | ObjClass::Package)
                    ) {
                        out.push(Link {
                            range: (a0, a1),
                            kind: LinkKind::Routine,
                            schema: None,
                            owner: None,
                            name,
                            known: true,
                        });
                    }
                    continue;
                }
                if aliases.iter().any(|a| a.alias.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                // 접두 없는 컬럼 = 문장의 별칭 테이블 중 그 컬럼을 가진 첫 테이블(컬럼 목록을 모르면 요청만).
                if let Some(a) = aliases
                    .iter()
                    .filter(|a| !a.local)
                    .find(|a| res.has_column(a.schema.as_deref(), &a.table, &name) == Some(true))
                {
                    out.push(Link {
                        range: (a0, a1),
                        kind: LinkKind::Column,
                        schema: a.schema.clone(),
                        owner: Some(a.table.clone()),
                        name,
                        known: true,
                    });
                }
            }
        }
    }
    out
}

/// 상태(App 필드).
#[derive(Default)]
pub(crate) struct ObjLinks {
    pub active: bool,
    pub tab: u64,
    pub rev: u64,
    /// 분석에 쓴 메타 스냅숏 stamp — 메타가 채워지면(컬럼·상세 도착) 다시 판정한다.
    pub stamp: u64,
    /// 분석한 문자 구간 — 전체면 `(0, len)` · 부분(큰 파일)이면 보이는 구간을 문장 경계까지 넓힌 것.
    pub region: (usize, usize),
    pub partial: bool,
    pub display: Display,
    pub links: Vec<Link>,
    pub hot: Option<usize>,
    /// 우클릭 메뉴가 가리키는 링크.
    pub menu_link: Option<usize>,
}

/// 부분 분석 때 보이는 구간 밖으로 문장 경계(`;`)를 찾는 최대 글자 수.
const REGION_REACH: usize = 64 * 1024;

/// 탐색기 스냅숏 위의 해석기 — 없는 컬럼은 `needs`에 모아 호스트가 요청한다(지연 로딩 · 61 §1-8).
struct SnapResolver<'a> {
    names: &'a Interner,
    snap: &'a Snapshot,
    needs: std::cell::RefCell<Vec<(Option<String>, String)>>,
}

impl Resolver for SnapResolver<'_> {
    fn object_class(&self, schema: Option<&str>, name: &str) -> Option<ObjClass> {
        let id = self.snap.lookup(self.names, schema, name)?;
        let o = self.snap.object(id)?;
        use nsql_catalog::ObjectKind as K;
        Some(match o.kind {
            K::Table
            | K::View
            | K::MaterializedView
            | K::Synonym
            | K::ExternalTable
            | K::ForeignTable => ObjClass::Relation,
            K::Procedure | K::Function | K::Aggregate => ObjClass::Routine,
            K::Package | K::PackageBody => ObjClass::Package,
            _ => ObjClass::Other,
        })
    }
    fn has_column(&self, schema: Option<&str>, table: &str, col: &str) -> Option<bool> {
        let id = self.snap.lookup(self.names, schema, table)?;
        match self.snap.columns(id) {
            ColState::Loaded { cols, .. } => Some(
                cols.iter()
                    .any(|c| self.names.get(c.name).eq_ignore_ascii_case(col)),
            ),
            ColState::Unknown => {
                let key = (schema.map(String::from), table.to_string());
                let mut n = self.needs.borrow_mut();
                if !n.contains(&key) {
                    n.push(key);
                }
                None
            }
            _ => None,
        }
    }
}

/// 설정의 색(`#RRGGBB[AA]` · 빈 값/형식 오류 = None).
fn color_opt(settings: &Settings, key: &str) -> Option<nexa_ctl::Color> {
    let v = settings.get(key).unwrap_or("").trim();
    nexa_ctl::rgba_from_hex(v).map(|rgba| nexa_ctl::Color(rgba >> 8))
}

fn line_opt(settings: &Settings, key: &str) -> LinkLine {
    match settings.get(key).unwrap_or("solid") {
        "dashed" => LinkLine::Dashed,
        "dotted" => LinkLine::Dotted,
        "wavy" => LinkLine::Wavy,
        "wavy_dashed" => LinkLine::WavyDashed,
        _ => LinkLine::Solid,
    }
}

impl App {
    /// 기능이 지금 켜질 수 있는가(설정).
    fn objlink_enabled(&self) -> bool {
        self.settings.flag("objlink.enabled")
    }

    /// 이 탭에 써도 되는가: SQL 구문만(큰 파일·크기 상한은 끄지 않고 **부분 분석**으로 간다).
    fn objlink_suitable(&self) -> bool {
        self.editors.syntax_name() == "SQL"
    }

    /// 실제 표시 방식 — 향상 모드(`perf::BOOST` = none 강제)와 큰 파일 모드는 `none`(다시 그리기 0 · 동작은 유지).
    fn objlink_display(&self) -> Display {
        if self.editors.is_large(self.editors.active()) {
            return Display::None;
        }
        Display::parse(
            self.settings
                .effective("objlink.display")
                .unwrap_or("hover"),
        )
    }

    /// 부분 분석인가: 큰 파일 모드 · 문서가 `objlink.max_kb`(0 = 무제한)보다 클 때.
    fn objlink_partial(&self) -> bool {
        if self.editors.is_large(self.editors.active()) {
            return true;
        }
        let max_kb = self.settings.int("objlink.max_kb").max(0) as usize;
        max_kb > 0 && self.editors.cur().buf().len_bytes() > max_kb * 1024
    }

    /// ★ 밑줄 스타일(`objlink.line_*` 정상 · `objlink.bad_*` 미확인)을 전 편집기에 적용(설정 변경 · 기동).
    pub(crate) fn apply_objlink_style(&mut self) {
        let s = &self.settings;
        let ok = LinkStyle {
            color: color_opt(s, "objlink.line_color"),
            width: s.int("objlink.line_width").clamp(0, 4) as i32,
            line: line_opt(s, "objlink.line_style"),
        };
        let bad = LinkStyle {
            color: color_opt(s, "objlink.bad_color"),
            width: s.int("objlink.bad_width").clamp(0, 4) as i32,
            line: line_opt(s, "objlink.bad_style"),
        };
        if self.editors.set_link_styles(ok, bad) && self.objlinks.active {
            self.redraw();
        }
    }

    /// 표시 방식에 따라 편집기에 넣을 밑줄 구간 — 바뀌었으면 true.
    fn objlink_apply_marks(&mut self) -> bool {
        let marks: Vec<(usize, usize, bool)> = match self.objlinks.display {
            Display::All => self
                .objlinks
                .links
                .iter()
                .map(|l| (l.range.0, l.range.1, l.known))
                .collect(),
            Display::Hover => self
                .objlinks
                .hot
                .and_then(|k| self.objlinks.links.get(k))
                .map(|l| vec![(l.range.0, l.range.1, l.known)])
                .unwrap_or_default(),
            Display::None => Vec::new(),
        };
        let hot = match self.objlinks.display {
            Display::None => None,
            _ => self
                .objlinks
                .hot
                .and_then(|k| self.objlinks.links.get(k))
                .map(|l| l.range),
        };
        let tb = self.editors.cur_mut();
        let a = tb.set_link_marks(marks);
        let b = tb.set_link_hot(hot);
        a || b
    }

    /// 분석 구간: 전체 또는(부분) 보이는 구간을 앞뒤 `;`까지 넓힌 것(없으면 `REGION_REACH`까지).
    fn objlink_region(&self, partial: bool) -> Option<(usize, usize)> {
        let buf = self.editors.cur().buf();
        let len = buf.len();
        if !partial {
            return Some((0, len));
        }
        let (va, vb) = self.editors.cur().visible_range()?;
        let (va, vb) = (va.min(len), vb.min(len));
        let mut a = va;
        for (n, c) in buf.iter_rev_from(va).enumerate() {
            if c == ';' || n >= REGION_REACH {
                break;
            }
            a -= 1;
        }
        let mut b = vb;
        for (n, c) in buf.iter_from(vb).enumerate() {
            b += 1;
            if c == ';' || n >= REGION_REACH {
                break;
            }
        }
        Some((a, b.min(len)))
    }

    /// 지금 분석 결과를 그대로 써도 되는가(탭·본문·메타·표시 방식 같고 부분이면 보이는 구간이 안에).
    fn objlink_fresh(&self) -> bool {
        let o = &self.objlinks;
        if !o.active
            || o.tab != self.editors.active_id()
            || o.rev != self.editors.cur().rev()
            || o.display != self.objlink_display()
        {
            return false;
        }
        let stamp = self.explorer.meta_view(self.sess.spec.as_ref()).1.stamp;
        if stamp != o.stamp {
            return false;
        }
        if o.partial != self.objlink_partial() {
            return false;
        }
        if o.partial {
            match self.editors.cur().visible_range() {
                Some((va, vb)) => va >= o.region.0 && vb <= o.region.1,
                None => true,
            }
        } else {
            true
        }
    }

    /// ★ 수식키·탭·본문·설정이 바뀔 때 — Ctrl을 누르고 있고 쓸 수 있으면 링크 목록을 만들고, 아니면 걷는다.
    pub(crate) fn objlink_sync(&mut self) {
        let want = self.primary && self.objlink_enabled() && self.objlink_suitable();
        let tab = self.editors.active_id();
        // 다른 탭에 남긴 표시부터 걷는다(탭이 바뀌었을 때).
        if self.objlinks.active && self.objlinks.tab != tab {
            if let Some(tb) = self.editors.buf_mut_by_id(self.objlinks.tab) {
                tb.set_link_marks(Vec::new());
                tb.set_link_hot(None);
            }
            self.objlinks.active = false;
            self.objlinks.links.clear();
            self.objlinks.hot = None;
            self.redraw();
        }
        if !want {
            if self.objlinks.active {
                self.objlinks.active = false;
                self.objlinks.hot = None;
                self.objlinks.links.clear();
                let tb = self.editors.cur_mut();
                let a = tb.set_link_marks(Vec::new());
                let b = tb.set_link_hot(None);
                if a || b {
                    self.redraw();
                }
            }
            return;
        }
        if self.objlink_fresh() {
            return;
        }
        let rev = self.editors.cur().rev();
        let partial = self.objlink_partial();
        let display = self.objlink_display();
        // 부분 분석인데 아직 그린 적이 없으면(보이는 구간 모름) 다음 그리기 뒤 hover에서 다시 온다.
        let region = self.objlink_region(partial).unwrap_or((0, 0));
        let text: String = if partial {
            self.editors
                .cur()
                .buf()
                .slice(region.0, region.1)
                .into_owned()
        } else {
            self.editors.cur().text()
        };
        let spec = self.sess.spec.clone();
        let dialect = Some(self.sess.dialect);
        let (mut links, needs, stamp) = {
            let (names, snap) = self.explorer.meta_view(spec.as_ref());
            let res = SnapResolver {
                names,
                snap: &snap,
                needs: std::cell::RefCell::new(Vec::new()),
            };
            let links = scan(&text, dialect, &res);
            (links, res.needs.into_inner(), snap.stamp)
        };
        if region.0 > 0 {
            for l in &mut links {
                l.range.0 += region.0;
                l.range.1 += region.0;
            }
        }
        for (schema, table) in needs {
            self.explorer
                .request_columns(spec.as_ref(), schema.as_deref(), &table, false);
        }
        // hot은 자리로 이어 받는다(메타 도착으로 다시 판정할 때 깜빡이지 않게).
        let old_hot = self
            .objlinks
            .hot
            .and_then(|k| self.objlinks.links.get(k))
            .map(|l| l.range);
        let hot = old_hot.and_then(|r| links.iter().position(|l| l.range == r));
        self.objlinks = ObjLinks {
            active: true,
            tab,
            rev,
            stamp,
            region,
            partial,
            display,
            links,
            hot,
            menu_link: None,
        };
        if self.objlink_apply_marks() || display != Display::None {
            self.redraw();
        }
    }

    /// 커서 아래 링크 index.
    fn objlink_at(&self, p: Point) -> Option<usize> {
        if !self.objlinks.active {
            return None;
        }
        let tb = self.editors.cur();
        let idx = tb.index_at_point(p)?;
        let k = self
            .objlinks
            .links
            .iter()
            .position(|l| l.range.0 <= idx && idx < l.range.1)?;
        // 줄 끝 너머(같은 줄의 빈 자리)는 링크가 아니다.
        let end = tb.point_at(self.objlinks.links[k].range.1)?;
        if p.x > end.x {
            return None;
        }
        Some(k)
    }

    /// 마우스 이동 — 링크 위면 강조·툴팁(표시 방식에 따라 다시 그리기).
    pub(crate) fn objlink_hover(&mut self, p: Point) {
        if !self.objlinks.active {
            return;
        }
        // 본문·탭·메타·보이는 구간이 바뀌었으면 다시 분석.
        if !self.objlink_fresh() {
            self.objlink_sync();
            if !self.objlinks.active {
                return;
            }
        }
        let hot = self.objlink_at(p);
        if hot != self.objlinks.hot {
            self.objlinks.hot = hot;
            let changed = self.objlink_apply_marks();
            // 툴팁의 설명은 메타에서 — 없으면 상세(코멘트)를 요청해 둔다(지연).
            if let Some(k) = hot {
                self.objlink_prefetch(k);
            }
            // `none`이면 밑줄이 없으니 툴팁이 켜져 있을 때만 다시 그린다.
            if changed || self.settings.flag("objlink.tooltip") {
                self.redraw();
            }
        }
    }

    /// 링크의 설명을 위해 필요한 메타(테이블 상세 = 코멘트 · 컬럼)를 요청한다.
    fn objlink_prefetch(&mut self, k: usize) {
        let Some(link) = self.objlinks.links.get(k).cloned() else {
            return;
        };
        let spec = self.sess.spec.clone();
        let (table, schema) = match link.kind {
            LinkKind::Column => (link.owner.clone(), link.schema.clone()),
            _ => (Some(link.name.clone()), link.schema.clone()),
        };
        let Some(table) = table else { return };
        let id = {
            let (names, snap) = self.explorer.meta_view(spec.as_ref());
            snap.lookup(names, schema.as_deref(), &table)
        };
        if let Some(id) = id {
            let need_detail = {
                let (_, snap) = self.explorer.meta_view(spec.as_ref());
                matches!(snap.detail(id), DetailState::Unknown)
                    && !snap.col_comments.contains_key(&id)
            };
            if need_detail {
                self.explorer.request_detail(spec.as_ref(), id);
            }
        } else if link.kind == LinkKind::Column {
            self.explorer
                .request_columns(spec.as_ref(), schema.as_deref(), &table, true);
        }
    }

    /// 링크의 설명(코멘트) — (종류 라벨, 표시 이름, 설명 · 미확인 = None).
    fn objlink_describe(&self, k: usize) -> Option<(String, String, Option<String>)> {
        let link = self.objlinks.links.get(k)?;
        let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
        let kind_label = t(match link.kind {
            LinkKind::Table => Msg::ObjKindTable,
            LinkKind::Column => Msg::ObjKindColumn,
            LinkKind::Routine => Msg::ObjKindRoutine,
        })
        .to_string();
        let desc = match link.kind {
            LinkKind::Column => {
                let table = link.owner.as_deref();
                let id = table.and_then(|tb| snap.lookup(names, link.schema.as_deref(), tb));
                let col = &link.name;
                id.and_then(|id| {
                    let from_cols = match snap.columns(id) {
                        ColState::Loaded { cols, .. } => cols
                            .iter()
                            .find(|c| names.get(c.name).eq_ignore_ascii_case(col))
                            .and_then(|c| c.comment)
                            .map(|s| names.get(s).to_string()),
                        _ => None,
                    };
                    from_cols
                        .or_else(|| {
                            snap.col_comments.get(&id).and_then(|cc| {
                                cc.iter()
                                    .find(|(n, _)| names.get(*n).eq_ignore_ascii_case(col))
                                    .and_then(|(_, c)| *c)
                                    .map(|s| names.get(s).to_string())
                            })
                        })
                        .or_else(|| match snap.detail(id) {
                            DetailState::Loaded { detail, .. } => detail
                                .col_comments
                                .iter()
                                .find(|(n, _)| names.get(*n).eq_ignore_ascii_case(col))
                                .map(|(_, c)| names.get(*c).to_string()),
                            _ => None,
                        })
                })
            }
            _ => snap
                .lookup(names, link.schema.as_deref(), &link.name)
                .and_then(|id| {
                    snap.object(id)
                        .and_then(|o| o.comment)
                        .map(|s| names.get(s).to_string())
                        .or_else(|| match snap.detail(id) {
                            DetailState::Loaded { detail, .. } => {
                                detail.comment.map(|s| names.get(s).to_string())
                            }
                            _ => None,
                        })
                }),
        };
        Some((
            kind_label,
            link.qualified(self.settings.flag("objlink.show_schema")),
            desc.filter(|d| !d.trim().is_empty()),
        ))
    }

    /// 클릭(좌 = 설명 복사 · **Shift+좌** = `이름 - 설명` 복사 · 우 = 메뉴) — 링크 위였으면 true(편집기로 가지 않는다).
    pub(crate) fn objlink_click(&mut self, p: Point, right: bool) -> bool {
        if !self.objlinks.active {
            return false;
        }
        let Some(k) = self.objlink_at(p) else {
            return false;
        };
        if right {
            self.objlinks.menu_link = Some(k);
            let items = vec![
                CtxItem::item("objlink.copy_desc", t(Msg::MnObjLinkCopyDesc)),
                CtxItem::item("objlink.copy_name_desc", t(Msg::MnObjLinkCopyNameDesc)),
            ];
            let host = self.window_rect();
            self.objlink_menu.set_scale(self.scale);
            self.objlink_menu
                .open_at(p.x, p.y, items, host, px(100.0, self.scale));
            self.redraw();
        } else if self.shift {
            self.objlink_copy_name_desc(k);
        } else {
            self.objlink_copy_desc(k);
        }
        true
    }

    /// `이름 - 설명` 복사(사용자 09-29): 테이블 = `테이블 - 설명` · 컬럼 = `테이블.컬럼 - 설명` · 스키마 접두는 `objlink.show_schema` ·
    /// 설명이 없으면 이름만 복사하고 상태줄에 알린다.
    fn objlink_copy_name_desc(&mut self, k: usize) {
        let Some((_, name, desc)) = self.objlink_describe(k) else {
            return;
        };
        let text = match &desc {
            Some(d) => format!("{name} - {d}"),
            None => name.clone(),
        };
        self.sess.status = if !clipboard::write_text(&text) {
            t(Msg::StCopyFailed).into()
        } else if desc.is_some() {
            tf(Msg::StObjLinkCopiedNameDesc, &[&text])
        } else {
            tf(Msg::StObjLinkCopiedNameOnly, &[&name])
        };
        self.redraw();
    }

    fn window_rect(&self) -> Rect {
        self.window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(Rect::new(0, 0, i32::MAX / 2, i32::MAX / 2))
    }

    fn objlink_copy_desc(&mut self, k: usize) {
        let Some((_, name, desc)) = self.objlink_describe(k) else {
            return;
        };
        match desc {
            Some(d) => {
                self.sess.status = if clipboard::write_text(&d) {
                    tf(Msg::StObjLinkCopied, &[&name, &d])
                } else {
                    t(Msg::StCopyFailed).into()
                };
            }
            None => self.sess.status = tf(Msg::StObjLinkNoDesc, &[&name]),
        }
        self.redraw();
    }

    /// 우클릭 메뉴 라우팅(열려 있으면 모달 · 바깥 클릭은 닫고 통과).
    pub(crate) fn route_objlink_menu(&mut self, ev: &InputEvent) -> bool {
        if !self.objlink_menu.is_open() {
            return false;
        }
        let outside = self.objlink_menu.is_outside_click(ev);
        let consumed = self.objlink_menu.on_event(ev) && !outside;
        if let Some(id) = self.objlink_menu.take_picked() {
            if let Some(k) = self.objlinks.menu_link.take() {
                match id.as_str() {
                    "objlink.copy_desc" => self.objlink_copy_desc(k),
                    "objlink.copy_name_desc" => self.objlink_copy_name_desc(k),
                    _ => {}
                }
            }
            self.redraw();
            return true;
        }
        if consumed || self.objlink_menu.is_open() {
            self.redraw();
            return true;
        }
        false
    }

    /// 그릴 툴팁(커서 아래 링크의 설명 · 표시 설정 켜짐) — (본문, 링크 글자 사각형). 페인트가 표면을 빌리기 **전에** 셈한다.
    /// 미확인 객체(메타에 없음)는 "(현재 연결에 없는 객체)".
    pub(crate) fn objlink_tip(&self) -> Option<(String, Rect)> {
        if !self.objlinks.active || !self.settings.flag("objlink.tooltip") {
            return None;
        }
        let k = self.objlinks.hot?;
        let (kind, name, desc) = self.objlink_describe(k)?;
        let link = &self.objlinks.links[k];
        let second = match desc {
            Some(d) => d,
            None if !link.known => t(Msg::ObjLinkNotFound).to_string(),
            None => t(Msg::ObjLinkNoDesc).to_string(),
        };
        let text = format!("{kind} {name}\n{second}");
        let tb = self.editors.cur();
        let (a, e) = link.range;
        let (p0, p1) = (tb.point_at(a)?, tb.point_at(e)?);
        Some((
            text,
            Rect::new(p0.x, p0.y, (p1.x - p0.x).max(1), tb.line_h()),
        ))
    }
}

/// 팝업 층: 설명 툴팁을 링크 글자 기준 `pos`(top_right 기본 · 61 §2-2 = nexa-ctl `draw_tooltip_in`이 표면 안으로 맞춘다)에.
pub(crate) fn paint_tip(
    dc: &mut dyn nexa_ctl::draw::DrawCtx,
    th: &nexa_ctl::Theme,
    tip: Option<&(String, Rect)>,
    pos: &str,
    scale: f32,
    clamp: Rect,
) {
    let Some((text, link)) = tip else { return };
    let s = |v: f32| (v * scale).round() as i32;
    // 툴팁 크기(draw_tooltip_in과 같은 셈법: Status 글꼴 · 폭 +12 · 높이 = 줄 수 × 글자 높이 + 8).
    dc.select_font(nexa_ctl::FontSlot::Status, false);
    let lines: Vec<&str> = text.split('\n').collect();
    let w = lines.iter().map(|l| dc.text_width(l)).max().unwrap_or(0) + s(12.0);
    let h = dc.text_height() * lines.len() as i32 + s(8.0);
    let (x, y) = match pos {
        "top_left" => (link.x - w - s(4.0), link.y - s(6.0) - h),
        "bottom_right" => (link.right() + s(4.0), link.bottom() + s(6.0)),
        "bottom_left" => (link.x - w - s(4.0), link.bottom() + s(6.0)),
        _ => (link.right() + s(4.0), link.y - s(6.0) - h),
    };
    // draw_tooltip_in = 기준 rect 아래 6px · 가로 중앙 → 원하는 (x, y)에 오도록 0×0 기준을 역산한다.
    let anchor = Rect::new(x + w / 2, y - s(6.0), 0, 0);
    nexa_ctl::draw::draw_tooltip_in(dc, th, anchor, (clamp.x, clamp.w), text, scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl Resolver for Fake {
        fn object_class(&self, schema: Option<&str>, name: &str) -> Option<ObjClass> {
            match (schema, name.to_ascii_uppercase().as_str()) {
                (_, "TB_ORDER" | "TB_ITEM" | "TB_LAZY") => Some(ObjClass::Relation),
                (_, "SP_RUN" | "FN_CALC") => Some(ObjClass::Routine),
                (_, "PKG_X") => Some(ObjClass::Package),
                _ => None,
            }
        }
        fn has_column(&self, _schema: Option<&str>, table: &str, col: &str) -> Option<bool> {
            let table = table.to_ascii_uppercase();
            if table == "TB_LAZY" {
                return None; // 컬럼 목록을 아직 모른다.
            }
            if !matches!(table.as_str(), "TB_ORDER" | "TB_ITEM") {
                return None;
            }
            Some(matches!(
                (table.as_str(), col.to_ascii_uppercase().as_str()),
                ("TB_ORDER", "ORD_NO" | "ITEM_CD") | ("TB_ITEM", "ITEM_CD" | "ITEM_NM")
            ))
        }
    }

    fn kinds(text: &str) -> Vec<(String, LinkKind, Option<String>)> {
        scan(text, Some(nsql_core::Dialect::Oracle), &Fake)
            .into_iter()
            .map(|l| (l.name, l.kind, l.owner))
            .collect()
    }

    #[test]
    fn links_tables_columns_and_routines() {
        let v = kinds("select a.ord_no, item_nm, nvl(a.qty, 0), fn_calc(1) from biscm.tb_order a inner join tb_item b on b.item_cd = a.item_cd where pkg_x.is_ok(a.ord_no) = 'Y'");
        let names: Vec<&str> = v.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(
            names.contains(&"tb_order") && names.contains(&"tb_item"),
            "{v:?}"
        );
        assert!(
            v.iter().any(|(n, k, o)| n == "ord_no"
                && *k == LinkKind::Column
                && o.as_deref() == Some("tb_order")),
            "{v:?}"
        );
        // 접두 없는 컬럼 = 별칭 테이블에서 찾는다(item_nm → tb_item).
        assert!(
            v.iter().any(|(n, k, o)| n == "item_nm"
                && *k == LinkKind::Column
                && o.as_deref() == Some("tb_item")),
            "{v:?}"
        );
        // 함수: 카탈로그에 있는 것만(NVL은 아님).
        assert!(
            v.iter()
                .any(|(n, k, _)| n == "fn_calc" && *k == LinkKind::Routine),
            "{v:?}"
        );
        assert!(!names.contains(&"nvl"), "{v:?}");
        assert!(
            v.iter().any(|(n, k, o)| n == "is_ok"
                && *k == LinkKind::Routine
                && o.as_deref() == Some("pkg_x")),
            "{v:?}"
        );
        // 별칭 자체(a · b)와 키워드는 링크가 아니다.
        assert!(
            !names.contains(&"a") && !names.contains(&"b") && !names.contains(&"select"),
            "{v:?}"
        );
        // 스키마 한정 테이블.
        let t = scan(
            "select 1 from biscm.tb_order x",
            Some(nsql_core::Dialect::Oracle),
            &Fake,
        );
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].schema.as_deref(), Some("biscm"));
        assert_eq!(t[0].qualified(true), "biscm.tb_order");
        assert_eq!(t[0].qualified(false), "tb_order");
        assert!(t[0].known);
    }

    #[test]
    fn ranges_are_char_indices_and_ctes_are_skipped() {
        let text = "with c as (select 1 k from tb_order) select 한글, c.k from c";
        let v = scan(text, Some(nsql_core::Dialect::Oracle), &Fake);
        // tb_order만(CTE c와 c.k는 로컬).
        assert_eq!(v.len(), 1, "{v:?}");
        let (a, e) = v[0].range;
        let s: String = text.chars().skip(a).take(e - a).collect();
        assert_eq!(s, "tb_order");
    }

    /// 미확인(사용자 09-29 2차): 관계 자리의 이름이 메타에 없으면 링크이되 `known = false` · 별칭 컬럼도 테이블/컬럼이 없으면 미확인 ·
    /// 컬럼 목록을 아직 모르는 테이블의 컬럼은 일단 정상(요청 뒤 다시 판정).
    #[test]
    fn unknown_objects_are_links_but_not_known() {
        let v = scan(
            "select a.eng_time_index, a.ord_no, z.col1 from demo_bsy a, tb_order b, tb_lazy z",
            Some(nsql_core::Dialect::Oracle),
            &Fake,
        );
        let find = |n: &str| {
            v.iter()
                .find(|l| l.name == n)
                .unwrap_or_else(|| panic!("{n} {v:?}"))
        };
        assert!(!find("demo_bsy").known);
        assert!(find("tb_order").known);
        assert!(find("tb_lazy").known);
        // demo_bsy는 메타에 없다 → 그 컬럼도 미확인.
        assert_eq!(find("eng_time_index").kind, LinkKind::Column);
        assert!(!find("eng_time_index").known);
        // b가 아니라 a.ord_no — a = demo_bsy(없음) → 미확인.
        assert!(!find("ord_no").known);
        // 컬럼 목록 미확정 테이블의 컬럼 = 정상 취급.
        assert!(find("col1").known);
        // 확인된 테이블의 없는 컬럼 = 미확인.
        let w = scan(
            "select b.nope from tb_order b",
            Some(nsql_core::Dialect::Oracle),
            &Fake,
        );
        assert!(
            !w.iter()
                .find(|l| l.name == "nope")
                .expect("nope link")
                .known,
            "{w:?}"
        );
    }
}
