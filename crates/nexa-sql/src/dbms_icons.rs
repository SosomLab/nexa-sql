//! DBMS 아이콘(사용자 10-04) — `assets/dbms/<light|dark>/<64|20>/<id>.png` 내장(빌드 때 `build.rs`가 표를 만든다).
//! 큰 자리(서버 정보) = 64 px 원본 · 작은 자리(연결 정보) = 20 px 원본(작은 크기 전용 그림)을 **표시 크기로 줄여** 쓴다.
//! 테마(밝음/어두움)마다 그림이 따로 있다. 고르기 = 제품 힌트 → 방언 → `generic`. PNG는 처음 쓸 때 한 번 푼다(스레드 로컬 캐시).

use nexa_gfx::IconImage;
use nsql_core::Dialect;

include!(concat!(env!("OUT_DIR"), "/dbms_icons_table.rs"));

/// 아이콘 없는 DBMS의 대체 그림.
const GENERIC: &str = "generic";

/// 그림 원본 크기 — 자리에 따라 고른다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Src {
    /// 64 px 원본(서버 정보 · 큰 표시).
    Large,
    /// 20 px 원본(연결 정보 · 작은 표시 전용 그림).
    Small,
}

/// 힌트 글에서 찾을 제품 키워드(소문자) → 아이콘 id. 앞에 있는 것이 우선(방언보다 먼저 본다 — MySQL 방언의 MariaDB 등).
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
    ("sql server", "sqlserver"),
    ("mssql", "sqlserver"),
    ("mysql", "mysql"),
    ("sqlite", "sqlite"),
];

/// 방언만으로는 제품을 모르는 연결(ODBC · 미접속)에서 힌트의 **낱말**이 아이콘 id와 같으면 그 그림 — 짧은 id(`h2` · `gel` ·
/// `rds` …)는 프로필 이름과 우연히 겹치기 쉬워 이 길이부터만 본다.
const WORD_MATCH_MIN: usize = 5;

fn entry(id: &str) -> Option<&'static (&'static str, [&'static [u8]; 4])> {
    TABLE
        .binary_search_by(|(n, _)| (*n).cmp(id))
        .ok()
        .map(|i| &TABLE[i])
}

/// 방언 + 제품 힌트(프로필 이름 · 접속 설명 · 소문자 비교) → 아이콘 id. 힌트 → 방언 → (ODBC·미접속이면 낱말 = id) → `generic`.
pub(crate) fn pick(dialect: Option<Dialect>, hint: &str) -> &'static str {
    let h = hint.to_ascii_lowercase();
    if let Some((_, n)) = HINTS.iter().find(|(kw, _)| h.contains(kw)) {
        return n;
    }
    match dialect {
        Some(Dialect::Oracle) => "oracle",
        Some(Dialect::Mssql) => "sqlserver",
        Some(Dialect::Postgres) => "postgresql",
        Some(Dialect::Mysql) => "mysql",
        Some(Dialect::Sqlite) => "sqlite",
        Some(Dialect::Odbc) | None => h
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= WORD_MATCH_MIN)
            .find_map(|w| entry(w).map(|e| e.0))
            .unwrap_or(match dialect {
                Some(_) => "odbc",
                None => GENERIC,
            }),
    }
}

/// 표의 열(`build.rs` `DBMS_SLOTS` 순서): light/64 · dark/64 · light/20 · dark/20.
fn slot(dark: bool, src: Src) -> usize {
    usize::from(dark) + if src == Src::Small { 2 } else { 0 }
}

/// id + 테마 + 원본 크기 → 푼 원본 그림(스레드 로컬 캐시 · 모르는 id = `generic`).
fn source(id: &str, dark: bool, src: Src) -> std::rc::Rc<IconImage> {
    type Key = (&'static str, usize);
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<Key, std::rc::Rc<IconImage>>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let e = entry(id)
        .or_else(|| entry(GENERIC))
        .expect("generic 아이콘은 항상 있다");
    let k = slot(dark, src);
    CACHE.with(|c| {
        c.borrow_mut()
            .entry((e.0, k))
            .or_insert_with(|| {
                let img = nexa_gfx::image::decode(e.1[k], 256 * 256)
                    .unwrap_or_else(|_| IconImage::from_rgba(1, 1, vec![0; 4]));
                std::rc::Rc::new(img)
            })
            .clone()
    })
}

/// 면적 평균 축소(알파 가중) — 64 → 22 px 같은 큰 배율 축소에서 2×2 보간은 선·글자가 깨진다.
fn shrink(src: &IconImage, w: u32, h: u32) -> IconImage {
    let (sw, sh) = (src.w as usize, src.h as usize);
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    // 대상 픽셀이 덮는 원본 구간 [a, b)와 원본 픽셀 i의 겹침 길이(고정 소수 대신 f32 — 아이콘 크기라 충분).
    let span = |d: usize, dn: usize, sn: usize| {
        let a = d as f32 * sn as f32 / dn as f32;
        let b = (d + 1) as f32 * sn as f32 / dn as f32;
        (a, b)
    };
    for y in 0..h {
        let (y0, y1) = span(y, h, sh);
        for x in 0..w {
            let (x0, x1) = span(x, w, sw);
            let (mut r, mut g, mut b, mut a, mut area) = (0f32, 0f32, 0f32, 0f32, 0f32);
            for sy in (y0 as usize)..(y1.ceil() as usize).min(sh) {
                let wy = (y1.min((sy + 1) as f32) - y0.max(sy as f32)).max(0.0);
                for sx in (x0 as usize)..(x1.ceil() as usize).min(sw) {
                    let wx = (x1.min((sx + 1) as f32) - x0.max(sx as f32)).max(0.0);
                    let wgt = wx * wy;
                    let p = &src.rgba[(sy * sw + sx) * 4..][..4];
                    let pa = f32::from(p[3]) * wgt;
                    r += f32::from(p[0]) * pa;
                    g += f32::from(p[1]) * pa;
                    b += f32::from(p[2]) * pa;
                    a += pa;
                    area += wgt;
                }
            }
            if a > 0.0 && area > 0.0 {
                let o = &mut out[(y * w + x) * 4..][..4];
                o[0] = (r / a).round() as u8;
                o[1] = (g / a).round() as u8;
                o[2] = (b / a).round() as u8;
                o[3] = (a / area).round() as u8;
            }
        }
    }
    IconImage::from_rgba(w as u32, h as u32, out)
}

/// id + 테마 + 원본 크기 + 표시 크기(정사각 px) → 그 크기의 그림(호출자가 캐시). 줄일 때 = 면적 평균 · 같으면 그대로 · 키울 때 = 2×2 보간.
pub(crate) fn image(id: &str, dark: bool, src: Src, px: u32) -> IconImage {
    let base = source(id, dark, src);
    let px = px.max(1);
    if px == base.w && px == base.h {
        (*base).clone()
    } else if px < base.w {
        shrink(&base, px, px)
    } else {
        base.resized(px, px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_every_icon_decodes_at_its_size() {
        assert!(
            TABLE.windows(2).all(|w| w[0].0 < w[1].0),
            "id 정렬(이진 탐색)"
        );
        assert!(entry(GENERIC).is_some());
        for (id, _) in TABLE {
            for dark in [false, true] {
                for (src, n) in [(Src::Large, 64), (Src::Small, 20)] {
                    let img = source(id, dark, src);
                    assert_eq!((img.w, img.h), (n, n), "{id} dark={dark} {src:?}");
                    assert!(img.rgba.chunks(4).any(|p| p[3] > 0), "{id}: 빈 그림");
                }
            }
        }
    }

    #[test]
    fn pick_hint_then_dialect_then_word_then_generic() {
        assert_eq!(pick(Some(Dialect::Oracle), "PROD"), "oracle");
        assert_eq!(pick(Some(Dialect::Mssql), "BISCM_MS"), "sqlserver");
        assert_eq!(pick(Some(Dialect::Mysql), "maria-dev"), "mariadb");
        assert_eq!(pick(Some(Dialect::Odbc), "Tibero DEV"), "tibero");
        assert_eq!(pick(Some(Dialect::Odbc), "zeta"), "odbc");
        // ODBC·미접속 = 낱말이 id와 같으면 그 그림(5자 이상만).
        assert_eq!(pick(Some(Dialect::Odbc), "snowflake-dw"), "snowflake");
        assert_eq!(pick(None, "teradata prod"), "teradata");
        assert_eq!(
            pick(Some(Dialect::Odbc), "h2 rds"),
            "odbc",
            "짧은 id는 안 본다"
        );
        // 방언을 아는 연결은 낱말 일치로 뒤집히지 않는다(프로필 이름 우연 일치).
        assert_eq!(pick(Some(Dialect::Oracle), "redis cache"), "oracle");
        assert_eq!(pick(None, ""), GENERIC);
        // 고른 id는 전부 표에 있다.
        for (_, id) in HINTS {
            assert!(entry(id).is_some(), "{id}");
        }
        for id in [
            "oracle",
            "sqlserver",
            "postgresql",
            "mysql",
            "sqlite",
            "odbc",
        ] {
            assert!(entry(id).is_some(), "{id}");
        }
    }

    #[test]
    fn shrink_keeps_color_and_coverage_and_sizes() {
        // 불투명 단색 = 줄여도 같은 색 · 완전 불투명.
        let solid = IconImage::from_rgba(64, 64, [10u8, 200, 30, 255].repeat(64 * 64));
        let s = shrink(&solid, 22, 22);
        assert_eq!((s.w, s.h), (22, 22));
        assert!(s.rgba.chunks(4).all(|p| p == [10, 200, 30, 255]));
        // 왼쪽 절반만 불투명 = 평균 알파 ≈ 절반 · 투명 픽셀의 색이 섞이지 않는다(알파 가중).
        let mut half = vec![0u8; 64 * 64 * 4];
        for y in 0..64 {
            for x in 0..32 {
                half[(y * 64 + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let s = shrink(&IconImage::from_rgba(64, 64, half), 1, 1);
        assert_eq!(&s.rgba[..3], &[255, 0, 0]);
        assert!((i32::from(s.rgba[3]) - 128).abs() <= 1, "{}", s.rgba[3]);
        // 표시 크기별 결과 크기(줄임 · 그대로 · 키움).
        for (src, px) in [
            (Src::Large, 43),
            (Src::Large, 64),
            (Src::Small, 16),
            (Src::Small, 20),
            (Src::Small, 32),
        ] {
            let i = image("oracle", false, src, px);
            assert_eq!((i.w, i.h), (px, px));
        }
        assert_eq!(
            image("no-such-dbms", true, Src::Small, 20),
            image(GENERIC, true, Src::Small, 20)
        );
    }
}
