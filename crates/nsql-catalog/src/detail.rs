//! ★ 객체 상세(docs/86 · T-223 · 09-25): 선택한 객체 하나의 정보를 **섹션 목록**(표 또는 글)으로 — 탐색기 아래 객체 상세 패널과
//! CLI `nsql cat detail`이 같은 함수를 쓴다. 유형별 내용은 트리 하위 폴더 표(`sub_kinds`)와 같은 원천에서 나온다(83 §1).
//! L3 층(85 §4): 필요할 때 즉시 · 호스트가 표시 뒤 회수.

use crate::gen::{generate, GenOpts, GenSpec, GenWhat};
use crate::tree::{sub_items, sub_kinds, SubKind};
use crate::{columns, comments, source, ObjectInfo, ObjectKind};
use nsql_core::{DbError, Session};

/// 섹션 종류(라벨은 호스트가 i18n으로).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionId {
    Properties,
    Columns,
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
    fn text(id: SectionId, text: String) -> Self {
        DetailSection {
            id,
            headers: Vec::new(),
            rows: Vec::new(),
            text: Some(text),
        }
    }
}

/// 소스가 의미 있는 종류(루틴·뷰·트리거·타입) — 있으면 DDL 대신 소스를 보인다.
fn has_source(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Procedure
            | ObjectKind::Function
            | ObjectKind::Package
            | ObjectKind::Trigger
            | ObjectKind::SchemaTrigger
            | ObjectKind::View
            | ObjectKind::MaterializedView
            | ObjectKind::Type
    )
}

/// 객체 상세 섹션(86 §3 표) — 속성 → 컬럼 → 하위 폴더별 표(비면 뺀다) → 소스/DDL. 섹션 하나가 실패해도 나머지는 낸다(실패 = 뺀다).
pub fn object_details(
    s: &mut dyn Session,
    o: &ObjectInfo,
    opts: GenOpts,
) -> Result<Vec<DetailSection>, DbError> {
    let d = s.dialect();
    let mut out = Vec::new();
    // 속성.
    let mut props: Vec<Vec<String>> = vec![
        vec!["type".into(), o.kind.code().to_string()],
        vec!["schema".into(), o.schema.clone()],
        vec!["name".into(), o.name.clone()],
    ];
    if !o.status.is_empty() {
        props.push(vec!["status".into(), o.status.clone()]);
    }
    if !o.modified.is_empty() {
        props.push(vec!["modified".into(), o.modified.clone()]);
    }
    if !o.extra.is_empty() {
        props.push(vec!["detail".into(), o.extra.clone()]);
    }
    // 코멘트(관계 · 테이블 + 컬럼 · 86 §4 설명 = 코멘트가 있으면 그것) — 실패·없음 = 빈.
    let (tcomment, col_comments) = if o.kind.is_relation() {
        comments(s, &o.schema, &o.name)
    } else {
        (None, Vec::new())
    };
    if let Some(c) = &tcomment {
        if !c.trim().is_empty() {
            props.push(vec!["comment".into(), c.clone()]);
        }
    }
    out.push(DetailSection::table(
        SectionId::Properties,
        vec![HeaderId::Property, HeaderId::Value],
        props,
    ));
    // 컬럼(관계) — 트리와 같은 원천 · 컬럼 코멘트가 하나라도 있으면 열을 붙인다.
    if o.kind.is_relation() {
        if let Ok(cols) = columns(s, &o.schema, &o.name) {
            let with_comment = !col_comments.is_empty();
            let rows = cols
                .into_iter()
                .map(|c| {
                    let mut r = vec![
                        c.position.to_string(),
                        c.name.clone(),
                        c.data_type,
                        if c.nullable {
                            "NULL".into()
                        } else {
                            "NOT NULL".into()
                        },
                        c.default,
                    ];
                    if with_comment {
                        r.push(
                            col_comments
                                .iter()
                                .find(|(n, _)| n.eq_ignore_ascii_case(&c.name))
                                .map(|(_, cm)| cm.clone())
                                .unwrap_or_default(),
                        );
                    }
                    r
                })
                .collect();
            let mut headers = vec![
                HeaderId::Num,
                HeaderId::Name,
                HeaderId::Type,
                HeaderId::Nullable,
                HeaderId::Default,
            ];
            if with_comment {
                headers.push(HeaderId::Comment);
            }
            out.push(DetailSection::table(SectionId::Columns, headers, rows));
        }
    }
    // 하위 폴더(제약·인덱스·트리거·인자 … · 83 §1 표) — 컬럼은 위에서 · 비면 뺀다.
    for &sub in sub_kinds(d, o.kind) {
        if sub == SubKind::Columns && o.kind.is_relation() {
            continue;
        }
        let Ok(items) = sub_items(s, o, sub) else {
            continue;
        };
        if items.is_empty() {
            continue;
        }
        let rows = items
            .into_iter()
            .map(|it| vec![it.name, it.detail, it.status])
            .collect();
        out.push(DetailSection::table(
            SectionId::Sub(sub),
            vec![HeaderId::Name, HeaderId::Detail, HeaderId::Status],
            rows,
        ));
    }
    // 소스(루틴·뷰·트리거·타입) 또는 DDL(그 밖 · Generate SQL과 같은 옵션).
    let mut had_source = false;
    if has_source(o.kind) {
        if let Ok(src) = source(s, &o.schema, o.kind, &o.name) {
            if !src.trim().is_empty() {
                out.push(DetailSection::text(SectionId::Source, src));
                had_source = true;
            }
        }
    }
    if !had_source {
        let spec = GenSpec {
            owner: o.clone(),
            what: GenWhat::Ddl,
            sub: None,
            opts,
        };
        if let Ok(ddl) = generate(s, &spec) {
            if !ddl.trim().is_empty() {
                out.push(DetailSection::text(SectionId::Ddl, ddl));
            }
        }
    }
    Ok(out)
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
    fn source_kinds() {
        assert!(has_source(ObjectKind::Procedure));
        assert!(has_source(ObjectKind::View));
        assert!(!has_source(ObjectKind::Table));
        assert!(!has_source(ObjectKind::Index));
    }
}
