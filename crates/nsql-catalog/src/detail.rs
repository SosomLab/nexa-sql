//! ★ 객체 상세(docs/86 · T-223 · 09-25): 선택한 객체 하나의 정보를 **섹션 목록**(표 또는 글)으로 — 탐색기 아래 객체 상세 패널과
//! CLI `nsql cat detail`이 같은 함수를 쓴다. 유형별 내용은 트리 하위 폴더 표(`sub_kinds`)와 같은 원천에서 나온다(83 §1).
//! L3 층(85 §4): 필요할 때 즉시 · 호스트가 표시 뒤 회수.

use crate::gen::GenOpts;
use crate::tree::{sub_items, sub_kinds, SubKind};
use crate::{comments, table_detail, ColumnInfo, ObjectInfo, ObjectKind};
use nsql_core::{DbError, Session};

/// 섹션 종류(라벨은 호스트가 i18n으로).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionId {
    Properties,
    Columns,
    /// 테이블 기본 키(이름 · 컬럼).
    PrimaryKey,
    /// 테이블 인덱스(이름 · 컬럼 · UNIQUE).
    Indexes,
    Sub(SubKind),
    Source,
    Ddl,
}

/// 표 머리글 종류(라벨은 호스트가 i18n으로).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderId {
    Property,
    Value,
    Num,
    Name,
    Type,
    Nullable,
    Default,
    Detail,
    Status,
    Comment,
}

/// 섹션 하나 = 표(`headers` + `rows`) 또는 글(`text`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailSection {
    pub id: SectionId,
    pub headers: Vec<HeaderId>,
    pub rows: Vec<Vec<String>>,
    pub text: Option<String>,
}

impl DetailSection {
    fn table(id: SectionId, headers: Vec<HeaderId>, rows: Vec<Vec<String>>) -> Self {
        DetailSection {
            id,
            headers,
            rows,
            text: None,
        }
    }
}

fn is_routine(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Procedure | ObjectKind::Function | ObjectKind::Package
    )
}

/// ★ 객체 상세 섹션(86 §3 · 사용자 09-25 "인텔리센스·탐색기에 필요한 데이터 범위 안에서"):
/// 테이블 = 이름·설명·**PK·인덱스** · 뷰/MV = 이름·설명·**사용 테이블(의존)** · 프로시저/함수/패키지 = 이름·**인자·기본값** ·
/// 그 밖 = 탐색기가 바로 쓰는 것(속성 + 하위 폴더 표). 소스·DDL·컬럼 목록은 넣지 않는다(탐색기 트리·Generate SQL이 담당).
/// 섹션 하나가 실패해도 나머지는 낸다(실패 = 뺀다).
pub fn object_details(
    s: &mut dyn Session,
    o: &ObjectInfo,
    _opts: GenOpts,
) -> Result<Vec<DetailSection>, DbError> {
    let d = s.dialect();
    let mut out = Vec::new();
    let mut props: Vec<Vec<String>> = vec![vec!["name".into(), format!("{}.{}", o.schema, o.name)]];
    // 설명 = 코멘트(관계) — 실패·없음 = 빈.
    if o.kind.is_relation() {
        let (tcomment, _) = comments(s, &o.schema, &o.name);
        if let Some(c) = tcomment.filter(|c| !c.trim().is_empty()) {
            props.push(vec!["comment".into(), c]);
        }
    }
    if !o.kind.is_relation() && !is_routine(o.kind) {
        if !o.status.is_empty() {
            props.push(vec!["status".into(), o.status.clone()]);
        }
        if !o.modified.is_empty() {
            props.push(vec!["modified".into(), o.modified.clone()]);
        }
        if !o.extra.is_empty() {
            props.push(vec!["detail".into(), o.extra.clone()]);
        }
    }
    out.push(DetailSection::table(
        SectionId::Properties,
        vec![HeaderId::Property, HeaderId::Value],
        props,
    ));
    match o.kind {
        // 테이블 = PK + 인덱스(제약 전체·컬럼은 트리에).
        ObjectKind::Table | ObjectKind::ExternalTable | ObjectKind::ForeignTable => {
            if let Ok(td) = table_detail(s, &o.schema, &o.name) {
                let pk: Vec<Vec<String>> = td
                    .keys
                    .iter()
                    .filter(|k| k.kind == 'P')
                    .map(|k| vec![k.name.clone(), k.cols.join(", ")])
                    .collect();
                if !pk.is_empty() {
                    out.push(DetailSection::table(
                        SectionId::PrimaryKey,
                        vec![HeaderId::Name, HeaderId::Detail],
                        pk,
                    ));
                }
                let idx: Vec<Vec<String>> = td
                    .indexes
                    .iter()
                    .map(|i| {
                        vec![
                            i.name.clone(),
                            i.cols.join(", "),
                            if i.unique {
                                "UNIQUE".into()
                            } else {
                                String::new()
                            },
                        ]
                    })
                    .collect();
                if !idx.is_empty() {
                    out.push(DetailSection::table(
                        SectionId::Indexes,
                        vec![HeaderId::Name, HeaderId::Detail, HeaderId::Status],
                        idx,
                    ));
                }
            }
        }
        // 뷰 = 사용 중인 테이블(의존 · 방언이 지원할 때).
        ObjectKind::View | ObjectKind::MaterializedView => {
            if sub_kinds(d, o.kind).contains(&SubKind::Dependencies) {
                if let Ok(items) = sub_items(s, o, SubKind::Dependencies) {
                    if !items.is_empty() {
                        out.push(DetailSection::table(
                            SectionId::Sub(SubKind::Dependencies),
                            vec![HeaderId::Name, HeaderId::Detail, HeaderId::Status],
                            items
                                .into_iter()
                                .map(|it| vec![it.name, it.detail, it.status])
                                .collect(),
                        ));
                    }
                }
            }
        }
        // 루틴 = 인자(이름 · 타입/방향/기본값).
        k if is_routine(k) => {
            for sub in [SubKind::Arguments, SubKind::Procedures, SubKind::Functions] {
                if !sub_kinds(d, k).contains(&sub) {
                    continue;
                }
                if let Ok(items) = sub_items(s, o, sub) {
                    if !items.is_empty() {
                        out.push(DetailSection::table(
                            SectionId::Sub(sub),
                            vec![HeaderId::Name, HeaderId::Detail, HeaderId::Status],
                            items
                                .into_iter()
                                .map(|it| vec![it.name, it.detail, it.status])
                                .collect(),
                        ));
                    }
                }
            }
        }
        // 그 밖 = 하위 폴더 표(비면 뺀다) — 탐색기와 같은 원천.
        _ => {
            for &sub in sub_kinds(d, o.kind) {
                let Ok(items) = sub_items(s, o, sub) else {
                    continue;
                };
                if items.is_empty() {
                    continue;
                }
                out.push(DetailSection::table(
                    SectionId::Sub(sub),
                    vec![HeaderId::Name, HeaderId::Detail, HeaderId::Status],
                    items
                        .into_iter()
                        .map(|it| vec![it.name, it.detail, it.status])
                        .collect(),
                ));
            }
        }
    }
    Ok(out)
}

/// ★ 컬럼 상세(86 §3): 이름 · 설명(컬럼 코멘트) · 타입 · NOT NULL · 기본값 — 코멘트 질의 1(실패 = 없음).
pub fn column_details(
    s: &mut dyn Session,
    owner: &ObjectInfo,
    col: &ColumnInfo,
) -> Result<Vec<DetailSection>, DbError> {
    let (_, col_comments) = comments(s, &owner.schema, &owner.name);
    let comment = col_comments
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(&col.name))
        .map(|(_, c)| c.clone())
        .filter(|c| !c.trim().is_empty());
    let mut rows = vec![vec!["name".into(), col.name.clone()]];
    if let Some(c) = comment {
        rows.push(vec!["comment".into(), c]);
    }
    rows.push(vec!["type".into(), col.data_type.clone()]);
    rows.push(vec![
        "nullable".into(),
        if col.nullable {
            "NULL".into()
        } else {
            "NOT NULL".into()
        },
    ]);
    if !col.default.is_empty() {
        rows.push(vec!["default".into(), col.default.clone()]);
    }
    rows.push(vec![
        "table".into(),
        format!("{}.{}", owner.schema, owner.name),
    ]);
    Ok(vec![DetailSection::table(
        SectionId::Properties,
        vec![HeaderId::Property, HeaderId::Value],
        rows,
    )])
}

/// 표를 고정폭 글로(패널 본문·CLI 공용): 열 폭 = 글자 수 최대 · 두 칸 띄움 · 머리글은 호스트가 준 라벨.
#[must_use]
pub fn render_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let ncol = headers
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    let mut widths = vec![0usize; ncol];
    fn cell(r: &[String], i: usize) -> &str {
        r.get(i).map(String::as_str).unwrap_or("")
    }
    for (i, h) in headers.iter().enumerate() {
        widths[i] = widths[i].max(h.chars().count());
    }
    for r in rows {
        for (i, w) in widths.iter_mut().enumerate() {
            *w = (*w).max(cell(r, i).chars().count());
        }
    }
    let line = |r: &[String]| -> String {
        let mut s = String::new();
        for (i, w) in widths.iter().enumerate() {
            let c = cell(r, i);
            s.push_str(c);
            if i + 1 < ncol {
                let pad = w.saturating_sub(c.chars().count()) + 2;
                s.extend(std::iter::repeat_n(' ', pad));
            }
        }
        s.trim_end().to_string()
    };
    let mut out = String::new();
    if !headers.is_empty() {
        out.push_str(&line(headers));
        out.push('\n');
        let total: usize = widths.iter().sum::<usize>() + 2 * ncol.saturating_sub(1);
        out.extend(std::iter::repeat_n('-', total));
        out.push('\n');
    }
    for r in rows {
        out.push_str(&line(r));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_table_aligns_and_trims() {
        let h = vec!["Name".to_string(), "Type".to_string()];
        let rows = vec![
            vec!["ID".to_string(), "NUMBER(10)".to_string()],
            vec!["NAME_LONG".to_string(), "VARCHAR2(20)".to_string()],
        ];
        let t = render_table(&h, &rows);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "Name       Type");
        assert!(lines[1].starts_with("----"));
        assert_eq!(lines[2], "ID         NUMBER(10)");
        assert_eq!(lines[3], "NAME_LONG  VARCHAR2(20)");
        assert!(render_table(&[], &[]).is_empty());
    }

    #[test]
    fn routine_kinds() {
        assert!(is_routine(ObjectKind::Procedure));
        assert!(is_routine(ObjectKind::Package));
        assert!(!is_routine(ObjectKind::Table));
        assert!(!is_routine(ObjectKind::View));
    }
}
