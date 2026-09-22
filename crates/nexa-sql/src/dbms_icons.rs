//! DBMS 아이콘(사용자 09-22) — `assets/dbms/*.svg` 내장. 지금은 **generic 틀 + `<text>` 라벨(≤6자)**로 DBMS별 파일을 만들어 두었고,
//! 나중에 그 파일을 실제 로고(경로만 있는 SVG)로 바꾸면 **같은 로더가 로고를 그린다**(사용자 09-22): `<text>`가 있으면 틀 + 라벨(틀은
//! 라벨 폭만큼 가로로 늘려 그린다) · 없으면 경로 마스크만(정방형). 아이콘 없는 DBMS = `generic-database` + 힌트에서 만든 두 글자.
//! 래스터 = `toolicons::svg_glyph_vb`(경로마다 `fill-rule` 존중 · 알파 합성 · 64px 마스크 → 필요한 크기로 축소).

use crate::toolicons::svg_glyph_vb;
use nexa_ctl::controls::ctxmenu::MenuIcon;
use nexa_gfx::IconImage;
use nsql_core::Dialect;

/// 내장 아이콘(이름 · SVG 원문). 이름 = 파일 이름(확장자 없이).
const ICONS: &[(&str, &str)] = &[
    ("oracle", include_str!("../assets/dbms/oracle.svg")),
    (
        "microsoftsqlserver",
        include_str!("../assets/dbms/microsoftsqlserver.svg"),
    ),
    ("postgresql", include_str!("../assets/dbms/postgresql.svg")),
    ("mysql", include_str!("../assets/dbms/mysql.svg")),
    ("mariadb", include_str!("../assets/dbms/mariadb.svg")),
    ("sqlite", include_str!("../assets/dbms/sqlite.svg")),
    ("tibero", include_str!("../assets/dbms/tibero.svg")),
    ("altibase", include_str!("../assets/dbms/altibase.svg")),
    ("cubrid", include_str!("../assets/dbms/cubrid.svg")),
    ("db2", include_str!("../assets/dbms/db2.svg")),
    ("sybase", include_str!("../assets/dbms/sybase.svg")),
    ("odbc", include_str!("../assets/dbms/odbc.svg")),
    (
        "generic-database",
        include_str!("../assets/dbms/generic-database.svg"),
    ),
];

/// 힌트 글에서 찾을 제품 키워드(소문자) → 아이콘 이름. 앞에 있는 것이 우선(방언보다 먼저 본다 — MySQL 방언의 MariaDB 등).
const HINTS: &[(&str, &str)] = &[
    ("mariadb", "mariadb"),
    ("maria", "mariadb"),
    ("tibero", "tibero"),
    ("altibase", "altibase"),
    ("cubrid", "cubrid"),
    ("db2", "db2"),
    ("sybase", "sybase"),
    ("postgres", "postgresql"),
    ("oracle", "oracle"),
    ("sql server", "microsoftsqlserver"),
    ("mssql", "microsoftsqlserver"),
    ("mysql", "mysql"),
    ("sqlite", "sqlite"),
];

/// 루트 아이콘 선택 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pick {
    /// 아이콘 이름(`ICONS`의 키).
    pub name: &'static str,
    /// 틀 위에 그릴 라벨 줄들(SVG `<text>`마다 한 줄 · ≤3자 = 1줄 세로 중앙 · 4~6자 = 3자 + 나머지 2줄 · 사용자 09-22).
    /// 비어 있으면 로고 파일(경로만) = 라벨 없음.
    pub lines: Vec<String>,
    /// 틀(border) 색 = SVG 첫 `<path>`의 `fill="#RRGGBB"`(DBMS 대표색 · 없으면 호출자의 기본색).
    pub color: Option<(u8, u8, u8)>,
}

fn svg_of(name: &str) -> (&'static str, &'static str) {
    ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .or_else(|| ICONS.iter().find(|(n, _)| *n == "generic-database"))
        .copied()
        .expect("generic-database 아이콘은 항상 있다")
}

/// SVG의 `<text …>…</text>` 전부(순서대로 · 태그 안 공백 정리).
fn texts_of(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<text") {
        let r = &rest[i..];
        let (Some(open), Some(close)) = (r.find('>'), r.find("</text>")) else {
            break;
        };
        if let Some(body) = r.get(open + 1..close) {
            let body = body.trim();
            if !body.is_empty() {
                out.push(body.to_string());
            }
        }
        rest = &r[close + 7..];
    }
    out
}

/// SVG 첫 `<path … fill="#RRGGBB">`의 색.
fn frame_color(svg: &str) -> Option<(u8, u8, u8)> {
    let i = svg.find("<path")?;
    let tag_end = svg[i..].find('>')? + i;
    let tag = &svg[i..tag_end];
    let f = tag.find("fill=\"#")? + 7;
    let hex = tag.get(f..f + 6)?;
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// 라벨(≤6자)을 줄로 나눈다 — ≤3자 = 한 줄 · 4자 이상 = 앞 3자 + 나머지(사용자 09-22).
#[cfg(test)]
pub(crate) fn split_label(label: &str) -> Vec<String> {
    let cs: Vec<char> = label.chars().collect();
    if cs.len() <= 3 {
        vec![label.to_string()]
    } else {
        vec![cs[..3].iter().collect(), cs[3..].iter().collect()]
    }
}

/// 방언 + 제품 힌트(프로필 이름 · 접속 설명 · 소문자 비교) → 아이콘. 힌트 → 방언 → generic(+힌트 첫 낱말 두 글자 · 없으면 `Db`).
pub(crate) fn pick(dialect: Option<Dialect>, hint: &str) -> Pick {
    let h = hint.to_ascii_lowercase();
    let by_hint = HINTS.iter().find(|(kw, _)| h.contains(kw)).map(|(_, n)| *n);
    let name = by_hint.unwrap_or(match dialect {
        Some(Dialect::Oracle) => "oracle",
        Some(Dialect::Mssql) => "microsoftsqlserver",
        Some(Dialect::Postgres) => "postgresql",
        Some(Dialect::Mysql) => "mysql",
        Some(Dialect::Sqlite) => "sqlite",
        Some(Dialect::Odbc) => "odbc",
        None => "generic-database",
    });
    let (name, svg) = svg_of(name);
    let lines = if name == "generic-database" {
        let w = hint
            .split(|c: char| !c.is_alphanumeric())
            .find(|w| !w.is_empty())
            .unwrap_or("Db");
        let mut cs = w.chars();
        let a = cs.next().map(|c| c.to_ascii_uppercase()).unwrap_or('D');
        let b = cs.next().map(|c| c.to_ascii_lowercase()).unwrap_or('b');
        vec![format!("{a}{b}")]
    } else {
        texts_of(svg)
    };
    Pick {
        name,
        lines,
        color: frame_color(svg),
    }
}

/// SVG 원문에서 `<path … d="…" [fill-rule="evenodd"]>`들을 뽑는다(`<text>` 등은 무시).
fn paths(svg: &'static str) -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<path") {
        let tag = &rest[i..];
        let end = tag.find('>').unwrap_or(tag.len());
        let tag_body = &tag[..end];
        let evenodd = tag_body.contains("fill-rule=\"evenodd\"");
        if let Some(di) = tag_body.find(" d=\"") {
            let d0 = &tag_body[di + 4..];
            if let Some(dl) = d0.find('"') {
                out.push((&d0[..dl], evenodd));
            }
        }
        rest = &rest[i + end..];
    }
    out
}

/// 이름 → 64px 알파 마스크(경로들의 합 · 스레드 로컬 캐시).
pub(crate) fn mask(name: &str) -> MenuIcon {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<&'static str, MenuIcon>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let (key, svg) = svg_of(name);
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let mut acc: Vec<u8> = Vec::new();
                let (mut w, mut h) = (0u32, 0u32);
                for (d, evenodd) in paths(svg) {
                    let m = svg_glyph_vb(d, [0.0, 0.0, 24.0, 24.0], evenodd);
                    if acc.is_empty() {
                        acc = m.alpha.to_vec();
                        (w, h) = (m.w, m.h);
                    } else {
                        for (x, y) in acc.iter_mut().zip(m.alpha.iter()) {
                            *x = (*x).max(*y);
                        }
                    }
                }
                if acc.is_empty() {
                    return svg_glyph_vb("", [0.0, 0.0, 24.0, 24.0], false);
                }
                MenuIcon::from_alpha(w, h, &acc)
            })
            .clone()
    })
}

/// 이름 + 색 + 크기(폭·높이 — 라벨이 있으면 폭을 늘려 그린다) → 탐색기용 그림(호출자가 캐시).
pub(crate) fn image(name: &str, rgb: (u8, u8, u8), w: u32, h: u32) -> IconImage {
    let m = mask(name);
    let base = IconImage::from_alpha_tinted(m.w, m.h, &m.alpha, rgb);
    base.resized(w.max(1), h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_icon_has_paths_and_rasterizes() {
        for (name, svg) in ICONS {
            assert!(!paths(svg).is_empty(), "{name}: <path> 없음");
            assert!(mask(name).alpha.iter().any(|&a| a > 0), "{name}: 빈 마스크");
            if *name != "generic-database" {
                let t = texts_of(svg);
                assert!(!t.is_empty(), "{name}: <text> 라벨이 없다");
                let total: usize = t.iter().map(|l| l.chars().count()).sum();
                assert!(total <= 6 && t.len() <= 2, "{name}: 라벨 6자·2줄 초과");
                assert!(
                    t.iter().all(|l| l.chars().count() <= 3),
                    "{name}: 한 줄 3자 초과"
                );
                assert!(frame_color(svg).is_some(), "{name}: 틀 색이 없다");
            }
        }
    }

    #[test]
    fn pick_hint_then_dialect_then_generic_and_split_rule() {
        let o = pick(Some(Dialect::Oracle), "PROD");
        assert_eq!(o.lines, vec!["ORA", "CLE"]);
        assert_eq!(o.color, Some((0xC7, 0x46, 0x34)), "틀 색 = 파일의 fill");
        assert_eq!(pick(Some(Dialect::Mysql), "maria-dev").name, "mariadb");
        let t = pick(Some(Dialect::Odbc), "Tibero DEV");
        assert_eq!(
            (t.name, t.lines.clone()),
            ("tibero", vec!["TIB".to_string(), "ERO".to_string()])
        );
        assert_eq!(pick(Some(Dialect::Odbc), "zeta").lines, vec!["OD", "BC"]);
        let g = pick(None, "hana-erp");
        assert_eq!(
            (g.name, g.lines.clone(), g.color),
            ("generic-database", vec!["Ha".to_string()], None)
        );
        assert_eq!(pick(None, "").lines, vec!["Db"]);
        assert_eq!(split_label("DB2"), vec!["DB2"]);
        assert_eq!(split_label("MSSQL"), vec!["MSS", "QL"]);
        // 로고(경로만)로 바꾼 파일 = 라벨 없음.
        assert!(texts_of("<svg><path d=\"M0 0h1v1z\"/></svg>").is_empty());
    }
}
