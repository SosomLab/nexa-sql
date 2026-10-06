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
use nsql_run::meta::{ColState, DetailState, Interner, ObjId, Snapshot};
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
    /// ★ 분석에 쓴 **세션 키**(세션 id · 현재 스키마 · 계정) — 탭의 연결이 바뀌면 다시 판정한다(사용자 09-30 "BISCM/SQLEDU 전환
    ///   뒤 이전 세션의 판정이 남아 있었다").
    pub sess_key: String,
    /// 분석한 문자 구간 — 전체면 `(0, len)` · 부분(큰 파일)이면 보이는 구간을 문장 경계까지 넓힌 것.
    pub region: (usize, usize),
    pub partial: bool,
    pub display: Display,
    pub links: Vec<Link>,
    pub hot: Option<usize>,
    /// 우클릭 메뉴가 가리키는 링크.
    pub menu_link: Option<usize>,
    /// ★ 머무름 모드(T-179 ③ · `objlink.hover_ms`): Ctrl 없이 포인터가 객체 이름 위에 머물러 분석이 켜진 상태 — 툴팁만
    /// (밑줄 없음 · 클릭 동작 없음) · 포인터가 링크를 벗어나거나 키를 치면 끝난다.
    pub hover: bool,
    /// 머무름 판정 시각·자리(포인터가 멈춘 뒤 `hover_ms`).
    pub hover_due: Option<std::time::Instant>,
    pub hover_at: Option<Point>,
    /// ★ hover 카드(T-179 ③ · 77 §1-3): 마지막 그리기의 카드 자리와 버튼 — 포인터가 카드 안이면 링크를 떠나도 유지 ·
    /// 클릭은 버튼으로(편집기로 가지 않는다). 툴팁이 없으면 None.
    pub card: Option<CardLayout>,
    /// MouseDown으로 누른 카드 버튼 id — 같은 버튼 위에서 놓으면 동작(사용자 10-06 "마우스업에서 동작").
    pub card_pressed: Option<&'static str>,
}

/// hover 카드 배치(그리기가 채우고 사건 처리가 읽는다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardLayout {
    pub rect: Rect,
    /// (버튼 id, 사각형, 활성).
    pub buttons: Vec<(&'static str, Rect, bool)>,
    /// 카드가 가리키는 링크.
    pub link: usize,
    /// 링크 글자 사각형(카드의 기준) — 링크에서 카드로 건너가는 "다리" 영역 판정에.
    pub anchor: Rect,
}

/// 포인터가 카드 **영역**(카드 ∪ 링크 글자의 둘레 상자 + 여유 `pad`) 안인가 — 링크에서 카드로 마우스를 옮기는 동안
/// 둘 사이 빈 틈(4~6 px)과 비스듬한 경로에서 카드가 사라지지 않게(사용자 10-06 "마우스를 움직이면 사라져서 클릭 불가").
fn card_zone_contains(card: Rect, anchor: Rect, pad: i32, p: Point) -> bool {
    let u = card.union(&anchor);
    Rect::new(u.x - pad, u.y - pad, u.w + pad * 2, u.h + pad * 2).contains(p)
}

/// 카드·툴팁의 세로 자리 — 정방향(`pos`의 위/아래)이 호스트 세로 범위 밖이면 **반대쪽**(링크 아래 ↔ 위)으로 뒤집는다
/// (61 §2-2 "정방향 → 반대쪽 → 밀어 넣기"). 밀어 넣기만 하면 링크 줄과 그 오른쪽 글자를 덮었다(협업 V1 10-06 관찰 ①②).
/// `host_y`/`host_bottom` = 호스트 세로 범위 · 반대쪽도 안 들어가면 정방향 값 그대로(호출자가 밀어 넣는다).
fn flip_vertical(pos: &str, anchor: Rect, h: i32, gap: i32, host_y: i32, host_bottom: i32) -> i32 {
    let above = anchor.y - gap - h;
    let below = anchor.bottom() + gap;
    let wants_above = !matches!(pos, "bottom_right" | "bottom_left");
    if wants_above {
        if above >= host_y || below + h > host_bottom {
            above
        } else {
            below
        }
    } else if below + h <= host_bottom || above < host_y {
        below
    } else {
        above
    }
}

/// 부분 분석 때 보이는 구간 밖으로 문장 경계(`;`)를 찾는 최대 글자 수.
const REGION_REACH: usize = 64 * 1024;

/// ★ 시스템 객체 분류(10-01 ㉙ · 사용자 "`DBMS_XPLAN.DISPLAY_CURSOR` 같은 시스템 객체는 Ctrl 링크가 안 된다"): 메타(탐색기 스냅숏)에
///   없는 이름을 nsql-script 내장 표로 — 시스템 패키지(`builtins::package` · Oracle `DBMS_*`/`UTL_*`) = Package · 사전 객체
///   (`builtins::system_objects` · `ALL_TABLES` · `DUAL` · `sys.objects` …) = Relation. 스키마가 있으면 `SYS`/`PUBLIC`/`sys`만(그 밖은 사용자
///   스키마 = 메타가 답할 일). 순수 함수.
pub(crate) fn builtin_class(
    dialect: Option<nsql_core::Dialect>,
    schema: Option<&str>,
    name: &str,
) -> Option<ObjClass> {
    use nsql_script::builtins;
    if schema
        .is_some_and(|s| !matches!(s.to_ascii_uppercase().as_str(), "SYS" | "PUBLIC" | "SYSTEM"))
    {
        // `sys.objects`(SQL Server)처럼 사전 객체 자체가 접두를 가진 것은 아래 전체 이름 비교로.
        let full = format!("{}.{name}", schema.unwrap_or(""));
        return builtins::system_objects(dialect)
            .iter()
            .any(|o| o.eq_ignore_ascii_case(&full))
            .then_some(ObjClass::Relation);
    }
    if builtins::package(dialect, name).is_some() {
        return Some(ObjClass::Package);
    }
    builtins::system_objects(dialect)
        .iter()
        .any(|o| {
            o.eq_ignore_ascii_case(name)
                || o.rsplit_once('.')
                    .is_some_and(|(_, n)| n.eq_ignore_ascii_case(name))
        })
        .then_some(ObjClass::Relation)
}

/// 탐색기 스냅숏 위의 해석기 — 없는 컬럼은 `needs`에 모아 호스트가 요청한다(지연 로딩 · 61 §1-8).
struct SnapResolver<'a> {
    names: &'a Interner,
    snap: &'a Snapshot,
    /// 방언(시스템 객체 폴백 `builtin_class` · ㉙).
    dialect: Option<nsql_core::Dialect>,
    needs: std::cell::RefCell<Vec<(Option<String>, String)>>,
    /// 탭 세션의 현재 스키마(서버 답 → 접속 `?schema=` → 사용자) — 스냅숏의 현재 스키마 대신(같은 서버 다른 계정 · 사용자 09-30).
    cur: Option<String>,
    /// 접근성(96 §6): 세션 계정 · 이 서버 메타를 수집한 계정 — 다른 스키마의 객체는 둘이 같을 때만 "보인다".
    session_user: Option<String>,
    collector: Option<String>,
}

/// ★ 세션이 그 스키마의 객체에 닿을 수 있는가(96 §6 · 사용자 09-30 "권한 없는 행위·설명·접근이 허용되는 것처럼 보이면 안 된다"):
/// 자기 현재 스키마 · PUBLIC · 그 서버 메타를 **자기 계정이 수집**했을 때(권한 필터가 이미 자기 것). 다른 계정이 수집한 다른 스키마는
/// 메타에 있어도 없는 객체. 순수 함수.
pub(crate) fn schema_visible(
    obj_schema: &str,
    cur: Option<&str>,
    session_user: Option<&str>,
    collector: Option<&str>,
) -> bool {
    if obj_schema.eq_ignore_ascii_case("public") {
        return true;
    }
    if cur.is_some_and(|c| c.eq_ignore_ascii_case(obj_schema)) {
        return true;
    }
    match (session_user, collector) {
        (Some(u), Some(c)) => u.eq_ignore_ascii_case(c),
        _ => false,
    }
}

impl SnapResolver<'_> {
    fn resolve(&self, schema: Option<&str>, name: &str) -> Option<ObjId> {
        // ★ SQL Server(10-01 ㉗-i · 매트릭스 A9): 이름 해석은 현재 스키마(로그인 기본 · master에선 `guest`) → 없으면 **dbo**(서버 규칙).
        let id = self
            .snap
            .lookup_resolvable_from(self.names, schema, self.cur.as_deref(), name)
            .or_else(|| {
                (self.dialect == Some(nsql_core::Dialect::Mssql)
                    && schema.is_none()
                    && !self
                        .cur
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case("dbo")))
                .then(|| {
                    self.snap
                        .lookup_resolvable_from(self.names, None, Some("dbo"), name)
                })
                .flatten()
            })?;
        let o = self.snap.object(id)?;
        schema_visible(
            self.names.get(o.schema),
            self.cur.as_deref(),
            self.session_user.as_deref(),
            self.collector.as_deref(),
        )
        .then_some(id)
    }
}

impl Resolver for SnapResolver<'_> {
    fn object_class(&self, schema: Option<&str>, name: &str) -> Option<ObjClass> {
        // 서버가 풀 수 있고 이 세션이 닿을 수 있는 이름만 정상(96 §6) · 메타에 없으면 시스템 객체 표(㉙).
        let Some(id) = self.resolve(schema, name) else {
            return builtin_class(self.dialect, schema, name);
        };
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
        let id = self.resolve(schema, table)?;
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
        // 머무름 모드(Ctrl 없음) = 툴팁만 — 밑줄은 Ctrl을 눌렀을 때의 것.
        if !self.primary && self.objlinks.hover {
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
        if self.objlink_sess_key() != o.sess_key {
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
        let want = (self.primary || self.objlinks.hover)
            && self.objlink_enabled()
            && self.objlink_suitable();
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
            // ★ 우클릭 메뉴가 열려 있는 동안은 링크를 비우지 않는다(10-01 ㉗-e · 사용자 "첫 번째는 실패, 두 번째는 성공" = 메뉴를 띄운 뒤
            //   Ctrl을 떼면 목록이 비어 메뉴 항목이 가리키던 링크 번호가 무효가 됐다) — 메뉴가 닫힌 뒤 다음 동기화에서 걷는다.
            if self.objlink_menu.is_open() {
                return;
            }
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
        // ★ 우클릭 메뉴가 열려 있는 동안은 재분석을 미룬다(㉗-k · 메뉴가 닫힌 뒤 다음 동기화에서) — 항목 번호가 흔들리지 않게.
        if self.objlink_menu.is_open() && !self.objlink_menu_force_closed_for_resync {
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
        // ★ 자기 칸 메타가 비었으면 루트 읽기부터 · 현재 스키마 이름 버킷이 비었으면 그것부터(㉗-g·h) — 응답이 오면 stamp가 바뀌어 다시 분석된다.
        let cur0 = self.objlink_cur_schema();
        self.explorer.ensure_meta(spec.as_ref(), cur0.as_deref());
        let mut trace_lines: Vec<String> = Vec::new();
        let (mut links, needs, stamp) = {
            let (names, snap) = self.explorer.meta_view(spec.as_ref());
            // 탭 세션의 스키마 = 접속 `?schema=` → 사용자 이름(Oracle·PG·MSSQL 모두 기본 스키마의 근사 · 없으면 스냅숏 값).
            let cur = self.objlink_cur_schema();
            let res = SnapResolver {
                names,
                snap: &snap,
                dialect,
                needs: std::cell::RefCell::new(Vec::new()),
                cur,
                session_user: spec.as_ref().and_then(|s| s.user.clone()),
                collector: self.explorer.meta_account(spec.as_ref()),
            };
            let links = scan(&text, dialect, &res);
            // ★ 진단(`NSQL_TRACE_OBJLINK` · 09-30): 판정에 쓰인 값 — 세션 · 현재 스키마 · 계정 · 수집 계정 · 스냅숏 현재 스키마 · 링크별 결과.
            if std::env::var_os("NSQL_TRACE_OBJLINK").is_some() {
                let snap_cur = snap
                    .current_schema
                    .map(|s| names.get(s).to_string())
                    .unwrap_or_default();
                let mut line = format!(
                    "[objlink] sess={} cur={:?} sess_cur={:?} spec_user={:?} spec_schema={:?} collector={:?} snap_cur={snap_cur:?} schemas={} links:",
                    self.sess.id,
                    res.cur,
                    self.sess.cur_schema,
                    spec.as_ref().and_then(|s| s.user.clone()),
                    spec.as_ref().and_then(|s| s.schema.clone()),
                    res.collector,
                    snap.schemas.len()
                );
                for l in &links {
                    line.push_str(&format!(
                        " {}{}={}",
                        l.schema
                            .as_deref()
                            .map(|s| format!("{s}."))
                            .unwrap_or_default(),
                        l.name,
                        if l.known { "ok" } else { "NO" }
                    ));
                }
                trace_lines.push(line);
            }
            (links, res.needs.into_inner(), snap.stamp)
        };
        for line in trace_lines.drain(..) {
            // 값이 "1"이 아니면 파일 경로로 보고 거기에도 덧붙인다(자체 관리 폴더 · 진단).
            if let Some(path) = std::env::var_os("NSQL_TRACE_OBJLINK") {
                let path = path.to_string_lossy().into_owned();
                if path != "1" {
                    use std::io::Write as _;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                    {
                        let _ = writeln!(f, "{line}");
                    }
                }
            }
            self.log_win.push(LogEntry::new(LogKind::Info, line));
        }
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
        // ★ 메뉴가 가리키던 링크도 구간으로 이어받는다(10-01 ㉗-k · 사용자 "메뉴 항목을 눌러도 아무 일도 없다": 메뉴를 연 뒤 항목으로
        //   가는 동안 컬럼 선적재 응답으로 stamp가 바뀌어 재분석되면 `menu_link`가 None이 돼 클릭이 아무 동작도 하지 않았다).
        let old_menu = self
            .objlinks
            .menu_link
            .and_then(|k| self.objlinks.links.get(k))
            .map(|l| l.range);
        let menu_link = old_menu.and_then(|r| links.iter().position(|l| l.range == r));
        self.objlinks = ObjLinks {
            active: true,
            tab,
            rev,
            stamp,
            sess_key: self.objlink_sess_key(),
            region,
            partial,
            display,
            links,
            hot,
            menu_link,
            // 머무름 모드 상태는 재분석을 넘어 유지된다(모드가 끝날 때만 끈다).
            hover: self.objlinks.hover,
            hover_due: self.objlinks.hover_due,
            hover_at: self.objlinks.hover_at,
            card: None,
            card_pressed: None,
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

    /// ★ 포인터가 멈춘 자리를 기억한다(머무름 툴팁 · Ctrl 없음) — 머무름 모드 중 링크를 벗어나면 모드를 끝낸다.
    /// `objlink_hover`보다 먼저 부른다(모드가 끝나면 그쪽이 걷는다).
    pub(crate) fn objlink_rest(&mut self, p: Point) {
        if self.primary {
            self.objlinks.hover_due = None;
            return;
        }
        let ms = self.settings.int("objlink.hover_ms").max(0) as u64;
        if ms == 0 || !self.settings.flag("objlink.tooltip") {
            self.objlinks.hover_due = None;
            return;
        }
        if self.objlinks.hover {
            // 링크 위를 떠났다 → 끝(다음 멈춤에서 다시 잰다) · 카드 영역(카드 ∪ 링크 + 여유) 안은 떠난 것이 아니다.
            // ★ 다른 링크 위로 갔으면 카드 영역 안이라도 머무름을 끝낸다(카드 영역이 이웃 단어를 덮어 테이블 이름 위에서도 컬럼 카드가
            //   남던 결함 · 사용자 10-06) — 아래에서 새 자리로 다시 잰다.
            let over_other = self
                .objlink_at(p)
                .is_some_and(|k| Some(k) != self.objlinks.hot);
            if over_other || (self.objlink_at(p).is_none() && !self.objlink_card_zone(p)) {
                self.objlink_hover_end();
            } else {
                return;
            }
        }
        let inside = self.editors.cur().bounds().contains(p) && self.focus_allows_hover();
        self.objlinks.hover_due =
            inside.then(|| std::time::Instant::now() + std::time::Duration::from_millis(ms));
        self.objlinks.hover_at = inside.then_some(p);
    }

    /// 머무름 툴팁을 띄워도 되는 상태(팝업·메뉴가 없고 편집기가 보일 때).
    fn focus_allows_hover(&self) -> bool {
        self.open_menus() == 0 && !self.intel.is_open() && !self.objlink_menu.is_open()
    }

    /// 머무름 마감(사건 루프 틱) — 때가 됐으면 분석을 켜고 그 자리의 링크를 강조한다(없으면 즉시 끈다). 돌려주는 값 = 다음 깨울 시각.
    pub(crate) fn objlink_hover_tick(
        &mut self,
        now: std::time::Instant,
    ) -> Option<std::time::Instant> {
        let due = self.objlinks.hover_due?;
        if now < due {
            return Some(due);
        }
        self.objlinks.hover_due = None;
        let p = self.objlinks.hover_at?;
        if self.primary || !self.focus_allows_hover() {
            return None;
        }
        self.objlinks.hover = true;
        self.objlink_sync();
        if !self.objlinks.active {
            self.objlinks.hover = false;
            return None;
        }
        self.objlink_hover(p);
        if self.objlinks.hot.is_none() {
            self.objlink_hover_end();
        }
        None
    }

    /// 머무름 모드 끝(키 입력 · 링크 이탈 · 탭 전환) — 분석 결과를 걷고 다시 그린다.
    pub(crate) fn objlink_hover_end(&mut self) {
        self.objlinks.hover_due = None;
        if self.objlinks.hover {
            self.objlinks.hover = false;
            self.objlink_sync();
            self.redraw();
        }
    }

    /// 포인터가 hover 카드 안인가(카드가 떠 있을 때만).
    pub(crate) fn objlink_card_contains(&self, p: Point) -> bool {
        self.objlinks
            .card
            .as_ref()
            .is_some_and(|c| c.rect.contains(p))
    }

    /// 포인터가 hover 카드 **영역** 안인가(`card_zone_contains` · 여유 = 8 px × 배율) — 링크 → 카드 이동 중 유지 판정.
    fn objlink_card_zone(&self, p: Point) -> bool {
        let pad = (8.0 * self.scale).round() as i32;
        self.objlinks
            .card
            .as_ref()
            .is_some_and(|c| card_zone_contains(c.rect, c.anchor, pad, p))
    }

    /// hover 카드의 버튼 명세(id · 라벨 · 활성) — 그리기 전에 셈(설정 끔 = 없음 = 글만 있는 툴팁).
    pub(crate) fn objlink_card_buttons(&self) -> Vec<(&'static str, String, bool)> {
        if !self.settings.flag("objlink.card_buttons") {
            return Vec::new();
        }
        let Some(k) = self.objlinks.hot else {
            return Vec::new();
        };
        let target = self.objlink_reveal_target(k);
        let can_rows = target
            .as_ref()
            .is_some_and(|t| t.kind.is_relation() && t.member.is_none());
        vec![
            (
                "card.reveal",
                t(Msg::MnObjLinkReveal).to_string(),
                target.is_some(),
            ),
            ("card.rows", t(Msg::MnObjRows).to_string(), can_rows),
            ("card.copy", t(Msg::MnObjLinkCopyDesc).to_string(), true),
        ]
    }

    /// hover 카드 안 MouseDown — 카드 안이면 true(편집기로 가지 않는다 · 빈 자리도 삼킨다). 버튼 위면 **누름만** 기억하고
    /// 동작은 `objlink_card_release`(같은 버튼에서 놓을 때 · 사용자 10-06)에서.
    fn objlink_card_click(&mut self, p: Point) -> bool {
        let Some(card) = self.objlinks.card.as_ref() else {
            return false;
        };
        if !card.rect.contains(p) {
            return false;
        }
        self.objlinks.card_pressed = card
            .buttons
            .iter()
            .find(|(_, r, on)| *on && r.contains(p))
            .map(|(id, _, _)| *id);
        true
    }

    /// hover 카드 버튼 놓음 — 누른 버튼과 같은 버튼 위에서 놓았으면 동작하고 카드를 닫는다(동작은 링크 `k`가 살아 있는 동안 =
    /// 머무름을 끝내기 **전에**). 돌려주는 값 = 누름이 있었다(편집기로 가지 않는다).
    pub(crate) fn objlink_card_release(&mut self, p: Point) -> bool {
        let Some(pressed) = self.objlinks.card_pressed.take() else {
            return false;
        };
        let Some(card) = self.objlinks.card.clone() else {
            return true;
        };
        let hit = card
            .buttons
            .iter()
            .find(|(id, r, on)| *on && r.contains(p) && *id == pressed)
            .map(|(id, _, _)| *id);
        let k = card.link;
        match hit {
            Some("card.reveal") => {
                self.objlink_reveal(k);
                self.objlink_hover_end();
            }
            Some("card.rows") => {
                if let Some(t) = self.objlink_reveal_target(k) {
                    let sql = nsql_catalog::select_template(self.sess.dialect, &t.schema, &t.name);
                    self.objlink_hover_end();
                    self.run_in_fresh_tab(sql);
                }
            }
            Some("card.copy") => {
                self.objlink_copy_desc(k);
                self.objlink_hover_end();
            }
            _ => {}
        }
        true
    }

    /// 마우스 이동 — 링크 위면 강조·툴팁(표시 방식에 따라 다시 그리기).
    pub(crate) fn objlink_hover(&mut self, p: Point) {
        if !self.objlinks.active {
            return;
        }
        // 카드 영역(카드 ∪ 링크 + 여유) 안에서는 링크 판정을 바꾸지 않는다(버튼까지 가는 길에 카드가 사라지지 않게 ·
        // 링크와 카드 사이 틈도 포함) · 버튼 hover 색만 다시 그린다.
        if self.objlink_card_zone(p)
            && !self
                .objlink_at(p)
                .is_some_and(|k| Some(k) != self.objlinks.hot)
        {
            self.redraw();
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
        let cur = self.objlink_cur_schema();
        let id = {
            let (names, snap) = self.explorer.meta_view(spec.as_ref());
            self.objlink_resolve(names, &snap, schema.as_deref(), cur.as_deref(), &table)
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

    /// 링크 판정의 세션 키(세션 id · 현재 스키마 · 계정 · 수집 계정) — 바뀌면 재판정.
    fn objlink_sess_key(&self) -> String {
        let spec = self.sess.spec.as_ref();
        format!(
            "{}|{}|{}|{}",
            self.sess.id,
            self.objlink_cur_schema().unwrap_or_default(),
            spec.and_then(|s| s.user.clone()).unwrap_or_default(),
            self.explorer.meta_account(spec).unwrap_or_default()
        )
    }

    /// 탭 세션의 현재 스키마(접속 `?schema=` → 사용자) — 링크 판정·설명·선적재가 **같은 규칙**으로 이름을 푼다(사용자 09-30
    /// "BISCM으로 접속했는데 SQLEDU 객체 설명이 보인다" = 설명 경로가 전 스키마 조회를 쓰고 있었다).
    fn objlink_cur_schema(&self) -> Option<String> {
        // 서버가 답한 현재 스키마(접속 직후) → 접속 `?schema=` → 사용자 이름.
        self.sess
            .cur_schema
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.sess
                    .spec
                    .as_ref()
                    .and_then(|s| s.schema.clone().or_else(|| s.user.clone()))
            })
            .filter(|s| !s.is_empty())
    }

    /// 이름 → 객체 id(풀이 + 접근성 · 96 §6) — 판정·설명·선적재가 같은 규칙.
    fn objlink_resolve(
        &self,
        names: &Interner,
        snap: &Snapshot,
        schema: Option<&str>,
        cur: Option<&str>,
        name: &str,
    ) -> Option<ObjId> {
        let spec = self.sess.spec.as_ref();
        // SQL Server = 현재 스키마 → dbo 폴백(㉗-i · `SnapResolver::resolve`와 같은 규칙).
        let id = snap
            .lookup_resolvable_from(names, schema, cur, name)
            .or_else(|| {
                (self.sess.dialect == nsql_core::Dialect::Mssql
                    && schema.is_none()
                    && !cur.is_some_and(|c| c.eq_ignore_ascii_case("dbo")))
                .then(|| snap.lookup_resolvable_from(names, None, Some("dbo"), name))
                .flatten()
            })?;
        let o = snap.object(id)?;
        let collector = self.explorer.meta_account(spec);
        schema_visible(
            names.get(o.schema),
            cur,
            spec.and_then(|s| s.user.as_deref()),
            collector.as_deref(),
        )
        .then_some(id)
    }

    /// 링크의 설명(코멘트) — (종류 라벨, 표시 이름, 설명 · 미확인 = None).
    fn objlink_describe(&self, k: usize) -> Option<(String, String, Option<String>)> {
        let link = self.objlinks.links.get(k)?;
        let cur = self.objlink_cur_schema();
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
                let id = table.and_then(|tb| {
                    self.objlink_resolve(names, &snap, link.schema.as_deref(), cur.as_deref(), tb)
                });
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
            // ★ 시스템 패키지 멤버(㉙ · `DBMS_XPLAN.DISPLAY_CURSOR`) = 내장 표의 시그니처가 설명.
            LinkKind::Routine
                if link.owner.as_deref().is_some_and(|o| {
                    nsql_script::builtins::package(Some(self.sess.dialect), o).is_some()
                }) =>
            {
                nsql_script::builtins::signature(
                    Some(self.sess.dialect),
                    &format!("{}.{}", link.owner.as_deref().unwrap_or(""), link.name),
                )
                .map(str::to_string)
            }
            // 테이블·루틴도 판정과 같은 길(세션 현재 스키마 + PUBLIC + 접근성 · 96 §6) — 전 스키마 조회 금지(사용자 09-30 "엄격하게").
            _ => self
                .objlink_resolve(
                    names,
                    &snap,
                    link.schema.as_deref(),
                    cur.as_deref(),
                    &link.name,
                )
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

    /// 클릭(좌 = 설정 `objlink.click` — 설명 복사(기본) 또는 객체 탐색기에서 보기 · **Shift+좌** = `이름 - 설명` 복사 ·
    /// 우 = 메뉴) — 링크 위였으면 true(편집기로 가지 않는다). "보기"인데 실제 객체로 풀리지 않는 링크는 설명 복사로 돌아간다.
    /// 머무름 모드(Ctrl 없음)의 클릭은 링크 동작이 아니다 — 모드를 끝내고 편집기로 보낸다(캐럿 이동).
    pub(crate) fn objlink_click(&mut self, p: Point, right: bool) -> bool {
        if !self.objlinks.active {
            return false;
        }
        // hover 카드의 버튼(좌클릭) — Ctrl 유무와 무관.
        if !right && self.objlink_card_click(p) {
            return true;
        }
        if !self.primary && self.objlinks.hover {
            self.objlink_hover_end();
            return false;
        }
        let Some(k) = self.objlink_at(p) else {
            return false;
        };
        if right {
            self.objlink_open_menu(k, p);
        } else if self.shift {
            self.objlink_copy_name_desc(k);
        } else {
            let can_reveal = self.objlink_reveal_target(k).is_some();
            match click_action(self.settings.get("objlink.click"), can_reveal) {
                ClickAction::Reveal => self.objlink_reveal(k),
                ClickAction::Copy => self.objlink_copy_desc(k),
            }
        }
        true
    }

    /// ★ 링크 → 탐색기 찾기 대상(10-01 ㉗): 판정과 같은 길(`objlink_resolve` · 세션 현재 스키마 · 접근성) — 테이블/뷰/루틴 = 그 객체 ·
    ///   컬럼 = 소속 테이블 + 멤버(컬럼) · 패키지 멤버(`pkg.proc`) = 패키지 + 멤버. 못 풀면 None(메뉴 흐림).
    pub(crate) fn objlink_reveal_target(&self, k: usize) -> Option<crate::explorer::RevealTarget> {
        let link = self.objlinks.links.get(k)?;
        let cur = self.objlink_cur_schema();
        let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
        let resolve = |name: &str| {
            self.objlink_resolve(names, &snap, link.schema.as_deref(), cur.as_deref(), name)
                .and_then(|id| snap.object(id))
                .map(|o| {
                    (
                        names.get(o.schema).to_string(),
                        o.kind,
                        names.get(o.name).to_string(),
                    )
                })
        };
        let (schema, kind, name, member) = match link.kind {
            LinkKind::Column => {
                let (s, k, n) = resolve(link.owner.as_deref()?)?;
                (s, k, n, Some(link.name.clone()))
            }
            LinkKind::Routine => match link.owner.as_deref().and_then(resolve) {
                Some((s, k, n)) if k == nsql_catalog::ObjectKind::Package => {
                    (s, k, n, Some(link.name.clone()))
                }
                _ => {
                    let (s, k, n) = resolve(&link.name)?;
                    (s, k, n, None)
                }
            },
            LinkKind::Table => {
                let (s, k, n) = resolve(&link.name)?;
                (s, k, n, None)
            }
        };
        // SQL Server 다른 DB 객체 = 복합 열쇠 `DB.스키마`(⑭) → DB와 스키마로.
        let (db, schema) = match (self.sess.dialect, schema.split_once('.')) {
            (nsql_core::Dialect::Mssql, Some((d, s))) if !s.is_empty() => {
                (Some(d.to_string()), s.to_string())
            }
            _ => (None, schema),
        };
        Some(crate::explorer::RevealTarget {
            db,
            schema,
            kind,
            name,
            member,
        })
    }

    /// 메뉴 "객체 탐색기에서 보기": 탐색기를 보이게 하고 그 서버 칸에서 찾아 선택(비동기 읽기는 응답마다 이어진다).
    pub(crate) fn objlink_reveal(&mut self, k: usize) {
        let Some(t) = self.objlink_reveal_target(k) else {
            return;
        };
        if !self.explorer.is_visible() {
            self.menu_action("view.explorer");
        }
        let spec = self.sess.spec.clone();
        if self.explorer.reveal(spec.as_ref(), t) {
            self.set_focus(Focus::Explorer);
            self.explorer_actions();
        }
        self.redraw();
    }

    /// 테이블 이름(`[스키마.]이름` · 인용 허용)과 멤버(컬럼)로 객체 탐색기에서 찾기 — 결과 그리드 열 머리 메뉴의 길(T-180 ⑤).
    /// 해석은 편집기 링크와 같다(세션 현재 스키마 · 접근성 · SQL Server dbo 폴백). 못 풀면 상태줄 안내 + `false`.
    pub(crate) fn reveal_table_member(&mut self, table: &str, member: Option<String>) -> bool {
        let (schema_q, name) = nsql_io::split_table(self.sess.dialect, table);
        let cur = self.objlink_cur_schema();
        let found = {
            let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
            self.objlink_resolve(names, &snap, schema_q.as_deref(), cur.as_deref(), &name)
                .and_then(|id| snap.object(id))
                .map(|o| {
                    (
                        names.get(o.schema).to_string(),
                        o.kind,
                        names.get(o.name).to_string(),
                    )
                })
        };
        let Some((schema, kind, name)) = found else {
            self.sess.status = t(Msg::StObjRevealNone).into();
            self.redraw();
            return false;
        };
        let (db, schema) = match (self.sess.dialect, schema.split_once('.')) {
            (nsql_core::Dialect::Mssql, Some((d, s))) if !s.is_empty() => {
                (Some(d.to_string()), s.to_string())
            }
            _ => (None, schema),
        };
        if !self.explorer.is_visible() {
            self.menu_action("view.explorer");
        }
        let spec = self.sess.spec.clone();
        let target = crate::explorer::RevealTarget {
            db,
            schema,
            kind,
            name,
            member,
        };
        let ok = self.explorer.reveal(spec.as_ref(), target);
        if ok {
            self.set_focus(Focus::Explorer);
            self.explorer_actions();
        }
        self.redraw();
        ok
    }

    /// 테이블 이름(`[스키마.]이름`)을 메타 객체로(세션 현재 스키마 · 접근성 · SQL Server dbo 폴백) → (스키마, 이름, id).
    fn resolve_table_obj(&self, table: &str) -> Option<(String, String, nsql_run::meta::ObjId)> {
        let (schema_q, name) = nsql_io::split_table(self.sess.dialect, table);
        let cur = self.objlink_cur_schema();
        let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
        let id = self.objlink_resolve(names, &snap, schema_q.as_deref(), cur.as_deref(), &name)?;
        let o = snap.object(id)?;
        Some((
            names.get(o.schema).to_string(),
            names.get(o.name).to_string(),
            id,
        ))
    }

    /// 테이블의 제약(기본 키 · 외래 키) — `Loaded` = (종류, 열들, 참조 테이블, 참조 열들) 목록 · `Unknown` = 상세 요청을
    /// 보내고 None. 참조 열들은 FK만(비면 모름 → 부모 PK 순서 폴백).
    #[allow(clippy::type_complexity)]
    fn table_keys(
        &mut self,
        id: nsql_run::meta::ObjId,
    ) -> Option<Vec<(char, Vec<String>, Option<String>, Vec<String>)>> {
        let spec = self.sess.spec.clone();
        let (names, snap) = self.explorer.meta_view(spec.as_ref());
        match snap.detail(id) {
            nsql_run::meta::DetailState::Loaded { detail, .. } => Some(
                detail
                    .keys
                    .iter()
                    .map(|k| {
                        (
                            k.kind,
                            k.cols.iter().map(|c| names.get(*c).to_string()).collect(),
                            k.ref_table.map(|t| names.get(t).to_string()),
                            k.ref_cols
                                .iter()
                                .map(|c| names.get(*c).to_string())
                                .collect(),
                        )
                    })
                    .collect(),
            ),
            nsql_run::meta::DetailState::Loading => None,
            nsql_run::meta::DetailState::Unknown => {
                self.explorer.request_detail(spec.as_ref(), id);
                None
            }
        }
    }

    /// ★ 결과 그리드의 외래 키 열 목록을 메타에서 맞춘다(T-180 ⑥ · 결과 도착·메타 도착·설정 변경 때) — 단일 테이블 결과만 ·
    /// 제약을 모르면 한 번 요청(백그라운드 메타 세션 · 테이블마다 캐시) · 끄면 비우고 요청도 않는다.
    pub(crate) fn grid_fk_sync(&mut self) {
        if !self.settings.flag("grid.fk_follow") {
            self.grid.set_fk_cols(Vec::new());
            return;
        }
        let Some((_, _, id)) = self
            .grid
            .reveal_table()
            .and_then(|t| self.resolve_table_obj(&t))
        else {
            self.grid.set_fk_cols(Vec::new());
            return;
        };
        let cols = self
            .table_keys(id)
            .map(|keys| {
                keys.into_iter()
                    .filter(|(k, _, _, _)| *k == 'R')
                    .flat_map(|(_, cols, _, _)| cols)
                    .collect()
            })
            .unwrap_or_default();
        self.grid.set_fk_cols(cols);
    }

    /// ★ 셀 메뉴 "참조 행 보기"(T-180 ⑥): 원본 행 `row`의 열 `col`이 속한 외래 키 → 부모 테이블을 **참조 열로** 조회해 새 결과 탭에.
    /// 참조 열 = 메타의 FK 참조 컬럼(`ref_cols` · T-178 10-06)이 있으면 그것(유니크 키 참조도 맞다) · 없으면 부모 PK 순서
    /// (JOIN 조건 조각과 같은 규칙) · NULL 키·열 수 불일치·제약 미로딩 = 상태줄.
    pub(crate) fn follow_fk(&mut self, row: usize, col: &str) {
        let fail = |a: &mut Self, why: Msg| {
            a.sess.status = tf(Msg::StFkCannot, &[t(why)]);
            a.redraw();
        };
        let Some((_, _, id)) = self
            .grid
            .reveal_table()
            .and_then(|t| self.resolve_table_obj(&t))
        else {
            return fail(self, Msg::StObjRevealNone);
        };
        let Some(keys) = self.table_keys(id) else {
            return fail(self, Msg::StFkLoading);
        };
        let Some((_, fk_cols, Some(parent), ref_cols)) =
            keys.into_iter().find(|(k, cols, rt, _)| {
                *k == 'R' && rt.is_some() && cols.iter().any(|c| c.eq_ignore_ascii_case(col))
            })
        else {
            return fail(self, Msg::StFkNoPk);
        };
        let Some((pschema, pname, pid)) = self.resolve_table_obj(&parent) else {
            return fail(self, Msg::StObjRevealNone);
        };
        let pk_cols = if !ref_cols.is_empty() && ref_cols.len() == fk_cols.len() {
            ref_cols
        } else {
            let Some(pkeys) = self.table_keys(pid) else {
                return fail(self, Msg::StFkLoading);
            };
            let Some((_, pk_cols, _, _)) = pkeys.into_iter().find(|(k, _, _, _)| *k == 'P') else {
                return fail(self, Msg::StFkNoPk);
            };
            if pk_cols.len() != fk_cols.len() {
                return fail(self, Msg::StFkNoPk);
            }
            pk_cols
        };
        let values = self.grid.row_values(row, &fk_cols);
        if values.len() != fk_cols.len() || values.iter().any(|(_, v)| *v == nsql_core::Value::Null)
        {
            return fail(self, Msg::StFkNull);
        }
        let d = self.sess.dialect;
        let conds: Vec<String> = pk_cols
            .iter()
            .zip(values.iter())
            .map(|(pk, (_, v))| {
                format!(
                    "{} = {}",
                    nsql_catalog::quote_ident(d, pk),
                    v.to_sql_literal(d)
                )
            })
            .collect();
        let sql = format!(
            "SELECT * FROM {} WHERE {}",
            nsql_catalog::qualified(d, &pschema, &pname),
            conds.join(" AND ")
        );
        self.run_in_fresh_tab(sql);
    }

    /// ★ 명령 `obj.reveal`(F4 · 팔레트 · T-180): **캐럿 아래** 객체를 객체 탐색기에서 찾는다 — 우클릭 메뉴와 같은 판정
    /// (실제 객체로 풀릴 때만) · 링크는 Ctrl을 누르는 동안만 분석되므로 잠시 누른 것으로 치고 분석한 뒤 되돌린다.
    /// 캐럿이 링크 안이거나 양 끝에 닿아 있으면 그 링크 · 없으면 상태줄 안내.
    pub(crate) fn objlink_reveal_at_caret(&mut self) {
        if self.focus != Focus::Editor {
            return;
        }
        let was = self.primary;
        self.primary = true;
        self.objlink_sync();
        let caret = self.editors.cur().caret();
        let hit = self
            .objlinks
            .links
            .iter()
            .position(|l| l.range.0 <= caret && caret <= l.range.1);
        match hit {
            Some(k) if self.objlink_reveal_target(k).is_some() => self.objlink_reveal(k),
            // 못 푼 이름인데 다른 스키마에 후보가 있으면 **후보 메뉴**를 캐럿 아래에(T-180 ⑦) — F4로도 고를 수 있게.
            Some(k) if !self.objlink_candidates(k).is_empty() => {
                let tb = self.editors.cur();
                if let Some(p) = tb.caret_point() {
                    let at = Point {
                        x: p.x,
                        y: p.y + tb.line_h(),
                    };
                    self.objlink_open_menu(k, at);
                }
            }
            _ => {
                self.sess.status = t(Msg::StObjRevealNone).into();
                self.redraw();
            }
        }
        self.primary = was;
        if !was {
            self.objlink_sync();
        }
    }

    /// 자체 시험(기동 명령 `objlink.reveal:<이름>`): 분석된 링크 가운데 이름이 같은 첫 링크로 찾기.
    pub(crate) fn objlink_reveal_named(&mut self, name: &str) -> bool {
        // 링크는 Ctrl(⌘)을 누르는 동안만 분석된다 → 시험에서는 잠시 누른 것으로 치고 분석한 뒤 되돌린다.
        let was = self.primary;
        self.primary = true;
        self.objlink_sync();
        let ok = self.objlink_reveal_named_inner(name);
        self.primary = was;
        if !was {
            self.objlink_sync();
        }
        ok
    }

    /// 자체 시험(기동 명령 `objlink.dump:<파일>` · ㉙): 분석된 링크 한 줄씩 `종류|이름|known|설명`(Ctrl을 잠시 누른 것으로 치고 분석).
    pub(crate) fn objlink_dump_text(&mut self) -> String {
        let was = self.primary;
        self.primary = true;
        self.objlink_sync();
        let mut out = String::new();
        for k in 0..self.objlinks.links.len() {
            let l = &self.objlinks.links[k];
            let desc = self
                .objlink_describe(k)
                .and_then(|(_, _, d)| d)
                .unwrap_or_default();
            // 다섯째 열 = "탐색기에서 보기" 활성 여부(㉗-g · 메뉴가 흐리던 보고).
            let can = self.objlink_reveal_target(k).is_some();
            out.push_str(&format!(
                "{:?}|{}|{}|{desc}|reveal={can}
",
                l.kind,
                l.qualified(true),
                l.known
            ));
        }
        self.primary = was;
        if !was {
            self.objlink_sync();
        }
        out
    }

    fn objlink_reveal_named_inner(&mut self, name: &str) -> bool {
        let Some(k) = self
            .objlinks
            .links
            .iter()
            .position(|l| l.qualified(false).eq_ignore_ascii_case(name))
        else {
            return false;
        };
        self.objlink_reveal(k);
        true
    }

    /// ★ 링크 k의 우클릭 메뉴를 p에 연다(실제 우클릭 · 자체 시험 `objlink.menu:` 공용 · ㉗-k).
    pub(crate) fn objlink_open_menu(&mut self, k: usize, p: Point) {
        self.objlinks.menu_link = Some(k);
        // 진단(㉗-g · 메뉴가 흐리던 보고): 판정 근거 한 줄을 로그 창에.
        {
            let spec = self.sess.spec.clone();
            let (own, has, used) = self.explorer.meta_pane_info(spec.as_ref());
            let l = &self.objlinks.links[k];
            let line = format!(
                "[objlink] menu link={} kind={:?} known={} reveal={} cur={:?} own_pane={:?} own_meta={} used_pane={}",
                l.qualified(true),
                l.kind,
                l.known,
                self.objlink_reveal_target(k).is_some(),
                self.objlink_cur_schema(),
                own,
                has,
                used
            );
            self.log_win.push(LogEntry::new(LogKind::Info, line));
        }
        // ★ "객체 탐색기에서 보기"(10-01 ㉗) = 실제 객체로 풀릴 때만 활성(미확인 링크 = 흐림).
        let can = self.objlink_reveal_target(k).is_some();
        let mut items = vec![
            CtxItem::item("objlink.copy_desc", t(Msg::MnObjLinkCopyDesc)),
            CtxItem::item("objlink.copy_name_desc", t(Msg::MnObjLinkCopyNameDesc)),
            CtxItem::Separator,
            CtxItem::maybe("objlink.reveal", t(Msg::MnObjLinkReveal), can),
        ];
        // ★ 후보 팝업(T-180 ⑦): 현재 스키마에서 못 푼 이름이 **다른 스키마**에 있으면 고르게 — 자동으로 바꿔 풀지는 않는다(엄격).
        if !can {
            let cands = self.objlink_candidates(k);
            if !cands.is_empty() {
                let children: Vec<CtxItem> = cands
                    .iter()
                    .take(12)
                    .map(|(label, id)| {
                        CtxItem::item(format!("objlink.cand:{}", id.0), label.clone())
                    })
                    .collect();
                items.push(CtxItem::Separator);
                items.push(CtxItem::submenu(
                    "objlink.cands",
                    t(Msg::MnObjLinkCandidates),
                    children,
                ));
            }
        }
        let host = self.window_rect();
        self.objlink_menu.set_scale(self.scale);
        self.objlink_menu
            .open_at(p.x, p.y, items, host, px(100.0, self.scale));
        self.redraw();
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
            self.objlink_menu_pick(&id);
            self.redraw();
            return true;
        }
        if consumed || self.objlink_menu.is_open() {
            self.redraw();
            return true;
        }
        false
    }

    /// 메뉴 항목 확정(마우스 확정 · 자체 시험 `objlink.pick:<id>` 공용 · ㉗-k): 메뉴가 가리키던 링크로 동작 · 번호가 없으면 로그.
    pub(crate) fn objlink_menu_pick(&mut self, id: &str) {
        let Some(k) = self.objlinks.menu_link.take() else {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!(
                    "[objlink] pick id={id} but menu_link=None (links={})",
                    self.objlinks.links.len()
                ),
            ));
            return;
        };
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            format!("[objlink] pick id={id} link={k}"),
        ));
        if let Some(oid) = id
            .strip_prefix("objlink.cand:")
            .and_then(|n| n.parse::<u32>().ok())
        {
            self.reveal_obj_id(ObjId(oid));
            return;
        }
        match id {
            "objlink.copy_desc" => self.objlink_copy_desc(k),
            "objlink.copy_name_desc" => self.objlink_copy_name_desc(k),
            "objlink.reveal" => self.objlink_reveal(k),
            _ => {}
        }
    }

    /// 다른 스키마의 같은 이름 후보(T-180 ⑦) — 스키마를 쓰지 않은 테이블·루틴 링크가 현재 스키마에서 안 풀릴 때만 ·
    /// (표시 라벨 `SCHEMA.NAME [종류]`, 객체 id).
    fn objlink_candidates(&self, k: usize) -> Vec<(String, ObjId)> {
        let Some(link) = self.objlinks.links.get(k) else {
            return Vec::new();
        };
        if link.schema.is_some() || matches!(link.kind, LinkKind::Column) {
            return Vec::new();
        }
        let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
        snap.lookup_any_schema(names, &link.name)
            .into_iter()
            .filter_map(|id| {
                let o = snap.object(id)?;
                Some((
                    format!(
                        "{}.{}  [{:?}]",
                        names.get(o.schema),
                        names.get(o.name),
                        o.kind
                    ),
                    id,
                ))
            })
            .collect()
    }

    /// 메타 객체 id로 객체 탐색기에서 찾기(후보 팝업의 선택).
    fn reveal_obj_id(&mut self, id: ObjId) {
        let target = {
            let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
            snap.object(id).map(|o| {
                (
                    names.get(o.schema).to_string(),
                    o.kind,
                    names.get(o.name).to_string(),
                )
            })
        };
        let Some((schema, kind, name)) = target else {
            return;
        };
        let (db, schema) = match (self.sess.dialect, schema.split_once('.')) {
            (nsql_core::Dialect::Mssql, Some((d, s))) if !s.is_empty() => {
                (Some(d.to_string()), s.to_string())
            }
            _ => (None, schema),
        };
        if !self.explorer.is_visible() {
            self.menu_action("view.explorer");
        }
        let spec = self.sess.spec.clone();
        let ok = self.explorer.reveal(
            spec.as_ref(),
            crate::explorer::RevealTarget {
                db,
                schema,
                kind,
                name,
                member: None,
            },
        );
        if ok {
            self.set_focus(Focus::Explorer);
            self.explorer_actions();
        }
        self.redraw();
    }

    /// 자체 시험(㉗-k): 이름으로 링크를 찾아 **우클릭한 것처럼** 메뉴를 연다(Ctrl을 잠시 누른 것으로 치고 분석).
    pub(crate) fn objlink_menu_named(&mut self, name: &str) -> bool {
        let was = self.primary;
        self.primary = true;
        self.objlink_sync();
        let found = self
            .objlinks
            .links
            .iter()
            .position(|l| l.qualified(false).eq_ignore_ascii_case(name));
        // 링크 글자의 가운데(왼쪽 위 꼭짓점은 경계라 `index_at_point`가 앞 글자를 줄 수 있다).
        let pt = found.and_then(|k| {
            let tb = self.editors.cur();
            let (a, e) = self.objlinks.links[k].range;
            let p0 = tb.point_at(a)?;
            let p1 = tb.point_at(e)?;
            Some(Point {
                x: (p0.x + p1.x) / 2,
                y: p0.y + tb.line_h() / 2,
            })
        });
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            format!(
                "[objlink] menu_named {name}: active={} links={} found={found:?} point={pt:?} suitable={} enabled={}",
                self.objlinks.active,
                self.objlinks.links.len(),
                self.objlink_suitable(),
                self.objlink_enabled()
            ),
        ));
        let Some(p) = pt else {
            self.primary = was;
            return false;
        };
        let diag = {
            let tb = self.editors.cur();
            let k = found.unwrap_or(0);
            let r = self.objlinks.links.get(k).map(|l| l.range);
            format!(
                "idx_at={:?} range={:?} end={:?} bounds={:?}",
                tb.index_at_point(p),
                r,
                r.and_then(|(_, e)| tb.point_at(e)),
                tb.bounds()
            )
        };
        let ok = found.is_some();
        if let Some(k) = found {
            self.objlink_open_menu(k, p);
        }
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            format!(
                "[objlink] menu_named click ok={ok} open={} {diag}",
                self.objlink_menu.is_open()
            ),
        ));
        self.primary = was;
        ok
    }

    /// 자체 시험(㉗-k): 메뉴가 열린 채 재분석을 **강제**(선적재 응답으로 stamp가 바뀐 상황 흉내) — 메뉴 링크가 살아남아야 한다.
    pub(crate) fn objlink_resync_forced(&mut self) {
        let was = self.primary;
        self.primary = true;
        self.objlinks.stamp = u64::MAX;
        // 메뉴가 열려 있으면 미루는 규칙이 있으므로 여기서는 그 규칙을 지나쳐 재분석 본체만 돈다(최악 조건).
        let menu_was_open = self.objlink_menu.is_open();
        if menu_was_open {
            self.objlink_menu_force_closed_for_resync = true;
        }
        self.objlink_sync();
        self.objlink_menu_force_closed_for_resync = false;
        self.primary = was;
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
        // `point_at`의 y = 줄 **바닥**(아래 팝업용) → 링크 글자 사각형은 한 줄 위부터(협업 V1 10-06 "카드가 글자를 덮음"의 원인).
        let lh = tb.line_h();
        Some((text, Rect::new(p0.x, p0.y - lh, (p1.x - p0.x).max(1), lh)))
    }
}

/// ★ 팝업 층: 설명 **카드**(툴팁 글 + 동작 버튼 줄 · T-179 ③) — 버튼이 없으면 `paint_tip`과 같은 모양. 배치는 `paint_tip`의
/// 규칙(`pos`) 뒤 `nudge_into`로 표면 안에. 돌려주는 값 = 사건 처리용 배치(호스트가 `objlinks.card`에 둔다).
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_hover_card(
    dc: &mut dyn nexa_ctl::draw::DrawCtx,
    th: &nexa_ctl::Theme,
    tip: Option<&(String, Rect)>,
    link: usize,
    buttons: &[(&'static str, String, bool)],
    pointer: Option<Point>,
    pos: &str,
    scale: f32,
    clamp: Rect,
    flip_host: Rect,
) -> Option<CardLayout> {
    let (text, anchor) = tip?;
    if buttons.is_empty() {
        paint_tip(dc, th, tip, pos, scale, clamp, flip_host);
        return None;
    }
    let s = |v: f32| (v * scale).round() as i32;
    dc.select_font(nexa_ctl::FontSlot::Status, false);
    let lines: Vec<&str> = text.split('\n').collect();
    let th_line = dc.text_height();
    let text_w = lines.iter().map(|l| dc.text_width(l)).max().unwrap_or(0);
    let btn_h = th_line + s(6.0);
    let widths: Vec<i32> = buttons
        .iter()
        .map(|(_, l, _)| dc.text_width(l) + s(12.0))
        .collect();
    let btn_row_w: i32 = widths.iter().sum::<i32>() + s(6.0) * (buttons.len() as i32 - 1);
    let w = text_w.max(btn_row_w) + s(12.0);
    let h = th_line * lines.len() as i32 + s(8.0) + btn_h + s(8.0);
    let host = nexa_ctl::geom::popup_host(clamp, dc.surface_size());
    let x = match pos {
        "above" => anchor.x,
        "top_left" | "bottom_left" => anchor.x - w - s(4.0),
        _ => anchor.right() + s(4.0),
    };
    // 뒤집기 판정은 **편집기 영역**(위 공간 = 툴바·탭을 덮는 자리가 아니라 편집기 안) · 밀어 넣기는 창 전체.
    let y = flip_vertical(pos, *anchor, h, s(6.0), flip_host.y, flip_host.bottom());
    let r = nexa_ctl::geom::nudge_into(Rect::new(x, y, w, h), host);
    dc.fill_round_rect_alpha(r, s(4.0), th.text, 0.92);
    for (i, line) in lines.iter().enumerate() {
        dc.text(
            r.x + s(6.0),
            r.y + s(4.0) + th_line * i as i32,
            r,
            line,
            th.panel_bg,
        );
    }
    // 버튼 줄 — 머티리얼 텍스트 버튼(옅은 채움 · 외곽선 없음 · hover = 조금 진하게 · 비활성 = 흐린 글).
    let by = r.y + s(4.0) + th_line * lines.len() as i32 + s(6.0);
    let mut bx = r.x + s(6.0);
    let mut out = Vec::with_capacity(buttons.len());
    for ((id, label, on), bw) in buttons.iter().zip(widths) {
        let br = Rect::new(bx, by, bw, btn_h);
        let hot = *on && pointer.is_some_and(|p| br.contains(p));
        if *on {
            dc.fill_round_rect_alpha(br, s(3.0), th.panel_bg, if hot { 0.38 } else { 0.18 });
        }
        let tx = br.x + (br.w - dc.text_width(label)) / 2;
        let ty = dc.text_center_y(br.y, br.h);
        dc.text(
            tx,
            ty,
            br,
            label,
            if *on { th.panel_bg } else { th.text_dim },
        );
        out.push((*id, br, *on));
        bx = br.right() + s(6.0);
    }
    dc.select_font(nexa_ctl::FontSlot::Base, false);
    Some(CardLayout {
        rect: r,
        buttons: out,
        link,
        anchor: *anchor,
    })
}

/// 팝업 층: 설명 툴팁을 링크 글자 기준 `pos`(top_right 기본 · 61 §2-2 = nexa-ctl `draw_tooltip_in`이 표면 안으로 맞춘다)에.
/// `flip_host` = 위/아래 뒤집기 판정 영역(편집기 사각형) · `clamp` = 밀어 넣기 영역(창).
pub(crate) fn paint_tip(
    dc: &mut dyn nexa_ctl::draw::DrawCtx,
    th: &nexa_ctl::Theme,
    tip: Option<&(String, Rect)>,
    pos: &str,
    scale: f32,
    clamp: Rect,
    flip_host: Rect,
) {
    let Some((text, link)) = tip else { return };
    let s = |v: f32| (v * scale).round() as i32;
    // 툴팁 크기(draw_tooltip_in과 같은 셈법: Status 글꼴 · 폭 +12 · 높이 = 줄 수 × 글자 높이 + 8).
    dc.select_font(nexa_ctl::FontSlot::Status, false);
    let lines: Vec<&str> = text.split('\n').collect();
    let w = lines.iter().map(|l| dc.text_width(l)).max().unwrap_or(0) + s(12.0);
    let h = dc.text_height() * lines.len() as i32 + s(8.0);
    let x = match pos {
        // 캐럿 위(시그니처 카드) — 왼쪽을 기준 x에 맞춘다.
        "above" => link.x,
        "top_left" | "bottom_left" => link.x - w - s(4.0),
        _ => link.right() + s(4.0),
    };
    // 위 공간이 없으면 링크 아래로(글자를 덮지 않게 · 판정 = 편집기 영역) — 그래도 안 들어가는 것은 draw_tooltip_in이 밀어 넣는다.
    let y = flip_vertical(pos, *link, h, s(6.0), flip_host.y, flip_host.bottom());
    // draw_tooltip_in = 기준 rect 아래 6px · 가로 중앙 → 원하는 (x, y)에 오도록 0×0 기준을 역산한다.
    let anchor = Rect::new(x + w / 2, y - s(6.0), 0, 0);
    nexa_ctl::draw::draw_tooltip_in(dc, th, anchor, (clamp.x, clamp.w), text, scale);
}

/// Ctrl+좌클릭이 할 일(`objlink.click`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ClickAction {
    Copy,
    Reveal,
}

/// 설정값과 "실제 객체로 풀리는가"로 동작을 정한다 — `reveal`이어도 못 푸는 링크(미확인 · 내장 표)는 복사.
fn click_action(setting: Option<&str>, can_reveal: bool) -> ClickAction {
    if setting == Some("reveal") && can_reveal {
        ClickAction::Reveal
    } else {
        ClickAction::Copy
    }
}

#[cfg(test)]
mod tests {
    /// 카드 영역 = 카드 ∪ 링크 둘레 상자 + 여유: 틈·비스듬한 경로는 안 · 멀리는 밖.
    #[test]
    fn card_zone_bridges_link_and_card() {
        use super::card_zone_contains;
        use nexa_ctl::geom::{Point, Rect};
        let link = Rect::new(100, 200, 40, 16); // 글자
        let card = Rect::new(144, 150, 300, 44); // top_right: 오른쪽 위
        let z = |x, y| card_zone_contains(card, link, 8, Point { x, y });
        assert!(z(120, 208), "링크 위");
        assert!(z(200, 170), "카드 위");
        assert!(z(142, 197), "링크와 카드 사이 틈");
        assert!(z(141, 180), "비스듬히 올라가는 길(둘레 상자 안)");
        assert!(z(96, 220), "여유 8 px 안");
        assert!(!z(60, 208), "왼쪽 멀리");
        assert!(!z(200, 260), "아래 멀리");
    }

    /// 세로 뒤집기: 위가 모자라면 아래 · 아래가 모자라면 위 · 둘 다 모자라면 정방향(밀어 넣기 몫).
    #[test]
    fn flip_vertical_rules() {
        use super::flip_vertical;
        use nexa_ctl::geom::Rect;
        let link = Rect::new(100, 20, 40, 16); // 창 위쪽 1행
                                               // top_right: 위(20-6-44 < 0) 안 됨 → 아래(36+6).
        assert_eq!(flip_vertical("top_right", link, 44, 6, 0, 600), 42);
        // 위 공간이 있으면 정방향.
        let mid = Rect::new(100, 300, 40, 16);
        assert_eq!(flip_vertical("top_right", mid, 44, 6, 0, 600), 250);
        // bottom_right: 아래가 모자라면 위.
        let low = Rect::new(100, 580, 40, 16);
        assert_eq!(flip_vertical("bottom_right", low, 44, 6, 0, 600), 530);
        assert_eq!(flip_vertical("bottom_right", mid, 44, 6, 0, 600), 322);
        // 둘 다 모자라면 정방향 값(호출자가 밀어 넣는다).
        assert_eq!(flip_vertical("top_right", link, 44, 6, 0, 60), -30);
    }

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

    /// ★ 접근성(96 §6 · 사용자 09-30): 자기 현재 스키마 · PUBLIC = 보임 · 다른 스키마 = 수집 계정 == 세션 계정일 때만.
    #[test]
    fn schema_visible_rules() {
        use super::schema_visible;
        // SQLEDU 세션(수집 계정 BISCM): 자기 스키마 O · BISCM 것 X · PUBLIC O.
        assert!(schema_visible(
            "SQLEDU",
            Some("SQLEDU"),
            Some("SQLEDU"),
            Some("BISCM")
        ));
        assert!(!schema_visible(
            "BISCM",
            Some("SQLEDU"),
            Some("SQLEDU"),
            Some("BISCM")
        ));
        assert!(schema_visible(
            "PUBLIC",
            Some("SQLEDU"),
            Some("SQLEDU"),
            Some("BISCM")
        ));
        // BISCM 세션(수집 계정 BISCM): SQLEDU 것도 O(권한이 있어 수집됐다) · 계정 모르면 X.
        assert!(schema_visible(
            "SQLEDU",
            Some("BISCM"),
            Some("BISCM"),
            Some("BISCM")
        ));
        assert!(!schema_visible(
            "SQLEDU",
            Some("BISCM"),
            None,
            Some("BISCM")
        ));
        assert!(!schema_visible(
            "SQLEDU",
            Some("BISCM"),
            Some("BISCM"),
            None
        ));
        // 대소문자 무시.
        assert!(schema_visible("sqledu", Some("SQLEDU"), None, None));
    }

    /// ㉙ 시스템 객체 분류(순수): 패키지 · 사전 객체 · 접두 · 모르는 이름.
    #[test]
    fn builtin_class_rules() {
        use nsql_core::Dialect;
        assert_eq!(
            builtin_class(Some(Dialect::Oracle), None, "DBMS_XPLAN"),
            Some(ObjClass::Package)
        );
        assert_eq!(
            builtin_class(Some(Dialect::Oracle), None, "dbms_output"),
            Some(ObjClass::Package)
        );
        assert_eq!(
            builtin_class(Some(Dialect::Oracle), None, "ALL_TABLES"),
            Some(ObjClass::Relation)
        );
        assert_eq!(
            builtin_class(Some(Dialect::Oracle), Some("SYS"), "all_tables"),
            Some(ObjClass::Relation)
        );
        assert_eq!(
            builtin_class(Some(Dialect::Oracle), Some("BISCM"), "ALL_TABLES"),
            None
        );
        assert_eq!(
            builtin_class(Some(Dialect::Oracle), None, "NO_SUCH_THING"),
            None
        );
        assert_eq!(
            builtin_class(Some(Dialect::Mssql), Some("sys"), "objects"),
            Some(ObjClass::Relation)
        );
    }
}

#[cfg(test)]
mod click_tests {
    use super::{click_action, ClickAction};

    /// MC/DC — 설정이 reveal일 때만 · 풀릴 때만 보기(각 조건이 단독으로 결과를 뒤집는다).
    #[test]
    fn click_action_mcdc() {
        assert_eq!(click_action(Some("reveal"), true), ClickAction::Reveal);
        assert_eq!(click_action(Some("copy"), true), ClickAction::Copy);
        assert_eq!(click_action(Some("reveal"), false), ClickAction::Copy);
        assert_eq!(click_action(None, true), ClickAction::Copy);
    }
}
