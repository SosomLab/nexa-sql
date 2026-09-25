//! 오브젝트 탐색기 아이콘(사용자 09-15) — 툴바와 같은 **코드로 그린 알파 마스크**(정적 자원 0 · 글꼴 무관).
//!
//! - 256 좌표계 도형 → `SIDE`×`SIDE`(32px) 커버리지 마스크를 종류별로 **처음 쓸 때 한 번** 래스터(`OnceLock` · 1KB/종류).
//! - 색은 종류별 고정 팔레트(DBeaver 관례: 폴더 주황 · 테이블 파랑 · 코드 초록 …) · DBMS 루트는 방언 색.
//! - 틴트 이미지는 호스트(탐색기)가 `(종류, 색)` 키로 캐시 — 종류 16개 × 4KB 수준. 설정 `explorer.icons`로 끌 수 있다.

use nexa_gfx::IconImage;
use nsql_core::Dialect;
use std::sync::OnceLock;

/// 마스크 한 변(px) — 트리 행(글꼴 높이 ≈ 17~24px)에 contain 맞춤.
const SIDE: u32 = 32;
const SS: u32 = 4;

/// 아이콘 종류(마스크 인덱스).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum IconKind {
    Dbms,
    Schema,
    Folder,
    Table,
    View,
    MatView,
    Procedure,
    Function,
    Package,
    PackageBody,
    Sequence,
    Trigger,
    Index,
    Synonym,
    Type,
    Column,
    // ── 83 §1(09-25) 하위 항목·방언 고유 종류.
    /// 제약(키·체크·외래 키) — 열쇠.
    Constraint,
    /// 파티션 — 칸 나눈 상자.
    Partition,
    /// 확장 속성 — 꼬리표.
    Property,
    /// 규칙·정책 — 방패.
    Rule,
    /// 큐 — 쌓인 막대.
    Queue,
    /// DB 링크 — 고리 둘.
    Link,
    /// 잡·스케줄러 — 시계.
    Job,
    /// Java — 컵.
    Java,
}

impl IconKind {
    const ALL: [IconKind; 24] = [
        IconKind::Dbms,
        IconKind::Schema,
        IconKind::Folder,
        IconKind::Table,
        IconKind::View,
        IconKind::MatView,
        IconKind::Procedure,
        IconKind::Function,
        IconKind::Package,
        IconKind::PackageBody,
        IconKind::Sequence,
        IconKind::Trigger,
        IconKind::Index,
        IconKind::Synonym,
        IconKind::Type,
        IconKind::Column,
        IconKind::Constraint,
        IconKind::Partition,
        IconKind::Property,
        IconKind::Rule,
        IconKind::Queue,
        IconKind::Link,
        IconKind::Job,
        IconKind::Java,
    ];

    fn idx(self) -> usize {
        IconKind::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    fn shape(self) -> fn(f32, f32) -> bool {
        match self {
            IconKind::Dbms => shape_dbms,
            IconKind::Schema => shape_schema,
            IconKind::Folder => shape_folder,
            IconKind::Table => shape_table,
            IconKind::View => shape_view,
            IconKind::MatView => shape_matview,
            IconKind::Procedure => shape_procedure,
            IconKind::Function => shape_function,
            IconKind::Package => shape_package,
            IconKind::PackageBody => shape_package_body,
            IconKind::Sequence => shape_sequence,
            IconKind::Trigger => shape_trigger,
            IconKind::Index => shape_index,
            IconKind::Synonym => shape_synonym,
            IconKind::Type => shape_type,
            IconKind::Column => shape_column,
            IconKind::Constraint => shape_constraint,
            IconKind::Partition => shape_partition,
            IconKind::Property => shape_property,
            IconKind::Rule => shape_rule,
            IconKind::Queue => shape_queue,
            IconKind::Link => shape_link,
            IconKind::Job => shape_job,
            IconKind::Java => shape_java,
        }
    }

    /// 종류별 기본 색(RGB) — 라이트/다크 모두 읽히는 중간 명도.
    pub(crate) fn color(self) -> (u8, u8, u8) {
        match self {
            IconKind::Dbms => (0x6B, 0x7B, 0x95),
            IconKind::Schema | IconKind::Folder => (0xE8, 0xA3, 0x3D),
            IconKind::Table => (0x3A, 0x7B, 0xD5),
            IconKind::View | IconKind::MatView => (0x2F, 0x8F, 0xC8),
            IconKind::Procedure
            | IconKind::Function
            | IconKind::Package
            | IconKind::PackageBody => (0x4E, 0x9A, 0x51),
            IconKind::Sequence => (0x8E, 0x63, 0xC8),
            IconKind::Trigger => (0xD9, 0x82, 0x2B),
            IconKind::Index => (0x2E, 0x9E, 0x9E),
            IconKind::Synonym => (0x6B, 0x7B, 0x95),
            IconKind::Type => (0x9C, 0x5B, 0xB0),
            IconKind::Column => (0x8A, 0x8A, 0x8A),
            IconKind::Constraint => (0xC9, 0x8A, 0x1B),
            IconKind::Partition => (0x5B, 0x8D, 0xD9),
            IconKind::Property => (0x8A, 0x8A, 0x8A),
            IconKind::Rule => (0x9C, 0x5B, 0xB0),
            IconKind::Queue => (0xD9, 0x82, 0x2B),
            IconKind::Link => (0x6B, 0x7B, 0x95),
            IconKind::Job => (0x4E, 0x9A, 0x51),
            IconKind::Java => (0xE0, 0x6C, 0x2E),
        }
    }
}

/// DBMS 루트 색(브랜드 근사).
pub(crate) fn dbms_color(d: Dialect) -> (u8, u8, u8) {
    match d {
        Dialect::Oracle => (0xC7, 0x46, 0x34),
        Dialect::Mssql => (0x00, 0x78, 0xD4),
        Dialect::Postgres => (0x33, 0x67, 0x91),
        Dialect::Mysql => (0x00, 0x75, 0x8F),
        Dialect::Sqlite => (0x00, 0x3B, 0x57),
        Dialect::Odbc => (0x6B, 0x7B, 0x95),
    }
}

// ── 기본 도형(256 좌표계)

/// 축 정렬 사각형(x0..=x1 · y0..=y1).
fn rect(x: f32, y: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> bool {
    (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
}

fn seg_dist(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let (wx, wy) = (x - ax, y - ay);
    let t = ((wx * vx + wy * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (px, py) = (ax + t * vx, ay + t * vy);
    ((x - px) * (x - px) + (y - py) * (y - py)).sqrt()
}

fn stroke(x: f32, y: f32, a: (f32, f32), b: (f32, f32), w: f32) -> bool {
    seg_dist(x, y, a.0, a.1, b.0, b.1) <= w / 2.0
}

fn rrect(x: f32, y: f32, x0: f32, y0: f32, w: f32, h: f32, r: f32) -> bool {
    if x < x0 || y < y0 || x > x0 + w || y > y0 + h {
        return false;
    }
    let cx = x.clamp(x0 + r, x0 + w - r);
    let cy = y.clamp(y0 + r, y0 + h - r);
    (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r
}

fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
    dx * dx + dy * dy <= 1.0
}

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r
}

fn tri(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let s = |p: (f32, f32), q: (f32, f32)| (x - q.0) * (p.1 - q.1) - (p.0 - q.0) * (y - q.1);
    let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

// ── 종류별 도형

/// 원통(데이터베이스) — 위 타원 + 몸통 + 아래 타원 · 디스크 경계 홈 2줄.
fn shape_dbms(x: f32, y: f32) -> bool {
    let body = ellipse(x, y, 128.0, 64.0, 88.0, 30.0)
        || rect(x, y, 40.0, 216.0, 64.0, 192.0)
        || ellipse(x, y, 128.0, 192.0, 88.0, 30.0);
    let groove = |cy: f32| {
        ellipse(x, y, 128.0, cy, 88.0, 30.0) && !ellipse(x, y, 128.0, cy, 88.0, 18.0) && y > cy
    };
    body && !groove(108.0) && !groove(150.0)
}

/// 스키마 — 카드(윤곽) + 머리띠.
fn shape_schema(x: f32, y: f32) -> bool {
    let outer = rrect(x, y, 40.0, 52.0, 176.0, 152.0, 22.0);
    let inner = rrect(x, y, 58.0, 70.0, 140.0, 116.0, 12.0);
    let head = rect(x, y, 40.0, 216.0, 52.0, 100.0) && outer;
    (outer && !inner) || head
}

/// 폴더 — 탭 + 몸통.
fn shape_folder(x: f32, y: f32) -> bool {
    rrect(x, y, 40.0, 56.0, 92.0, 44.0, 14.0) || rrect(x, y, 40.0, 84.0, 176.0, 118.0, 14.0)
}

/// 테이블 — 머리 행(채움) + 2×2 셀.
fn shape_table(x: f32, y: f32) -> bool {
    rrect(x, y, 40.0, 48.0, 176.0, 44.0, 10.0)
        || rrect(x, y, 40.0, 104.0, 82.0, 44.0, 6.0)
        || rrect(x, y, 134.0, 104.0, 82.0, 44.0, 6.0)
        || rrect(x, y, 40.0, 160.0, 82.0, 44.0, 6.0)
        || rrect(x, y, 134.0, 160.0, 82.0, 44.0, 6.0)
}

/// 뷰 — 테이블과 같은 셀 · 머리 행은 윤곽만.
fn shape_view(x: f32, y: f32) -> bool {
    let head =
        rrect(x, y, 40.0, 48.0, 176.0, 44.0, 10.0) && !rrect(x, y, 54.0, 60.0, 148.0, 20.0, 6.0);
    head || rrect(x, y, 40.0, 104.0, 82.0, 44.0, 6.0)
        || rrect(x, y, 134.0, 104.0, 82.0, 44.0, 6.0)
        || rrect(x, y, 40.0, 160.0, 82.0, 44.0, 6.0)
        || rrect(x, y, 134.0, 160.0, 82.0, 44.0, 6.0)
}

/// 구체화 뷰 — 뷰 + 오른쪽 아래 점.
fn shape_matview(x: f32, y: f32) -> bool {
    shape_view(x, y) || circle(x, y, 204.0, 204.0, 30.0)
}

/// 프로시저 — 톱니바퀴.
fn shape_procedure(x: f32, y: f32) -> bool {
    let ring = circle(x, y, 128.0, 128.0, 72.0) && !circle(x, y, 128.0, 128.0, 30.0);
    let teeth = (0..8).any(|k| {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        circle(x, y, 128.0 + 82.0 * a.cos(), 128.0 + 82.0 * a.sin(), 24.0)
    });
    ring || teeth
}

/// 함수 — `f`.
fn shape_function(x: f32, y: f32) -> bool {
    stroke(x, y, (120.0, 72.0), (120.0, 204.0), 26.0)
        || stroke(x, y, (120.0, 72.0), (150.0, 52.0), 26.0)
        || stroke(x, y, (150.0, 52.0), (176.0, 60.0), 26.0)
        || stroke(x, y, (84.0, 124.0), (164.0, 124.0), 24.0)
}

/// 패키지 — 상자(뚜껑 홈).
fn shape_package(x: f32, y: f32) -> bool {
    let body = rrect(x, y, 44.0, 64.0, 168.0, 144.0, 16.0);
    let seam = (104.0..=116.0).contains(&y);
    let tape = rect(x, y, 116.0, 140.0, 116.0, 208.0);
    (body && !seam) && !tape || (body && !seam && tape && y > 150.0)
}

/// 패키지 본문 — 상자 + 안쪽 점.
fn shape_package_body(x: f32, y: f32) -> bool {
    let body = rrect(x, y, 44.0, 64.0, 168.0, 144.0, 16.0);
    let seam = (104.0..=116.0).contains(&y);
    (body && !seam && !circle(x, y, 128.0, 162.0, 34.0)) || circle(x, y, 128.0, 162.0, 18.0)
}

/// 시퀀스 — 오름 막대 3개.
fn shape_sequence(x: f32, y: f32) -> bool {
    rrect(x, y, 48.0, 140.0, 40.0, 64.0, 8.0)
        || rrect(x, y, 108.0, 100.0, 40.0, 104.0, 8.0)
        || rrect(x, y, 168.0, 56.0, 40.0, 148.0, 8.0)
}

/// 트리거 — 번개.
fn shape_trigger(x: f32, y: f32) -> bool {
    tri(x, y, (148.0, 40.0), (84.0, 140.0), (132.0, 140.0))
        || tri(x, y, (148.0, 40.0), (132.0, 140.0), (176.0, 40.0))
        || tri(x, y, (108.0, 216.0), (132.0, 116.0), (172.0, 116.0))
        || tri(x, y, (108.0, 216.0), (172.0, 116.0), (120.0, 150.0))
}

/// 인덱스 — 줄 3개 + 위 화살표.
fn shape_index(x: f32, y: f32) -> bool {
    [76.0, 128.0, 180.0]
        .iter()
        .any(|&cy| stroke(x, y, (104.0, cy), (204.0, cy), 24.0))
        || stroke(x, y, (60.0, 200.0), (60.0, 84.0), 22.0)
        || tri(x, y, (28.0, 96.0), (60.0, 44.0), (92.0, 96.0))
}

/// 동의어 — 화살표(별칭 → 대상).
fn shape_synonym(x: f32, y: f32) -> bool {
    stroke(x, y, (48.0, 128.0), (160.0, 128.0), 26.0)
        || tri(x, y, (140.0, 80.0), (216.0, 128.0), (140.0, 176.0))
}

/// 타입 — `T`.
fn shape_type(x: f32, y: f32) -> bool {
    rrect(x, y, 48.0, 56.0, 160.0, 40.0, 10.0) || rrect(x, y, 108.0, 56.0, 40.0, 148.0, 10.0)
}

/// 컬럼 — 세로 알약.
fn shape_column(x: f32, y: f32) -> bool {
    rrect(x, y, 100.0, 56.0, 56.0, 144.0, 28.0)
}

/// 제약 — 열쇠(고리 + 대각 자루 + 톱니 둘).
fn shape_constraint(x: f32, y: f32) -> bool {
    (circle(x, y, 84.0, 92.0, 48.0) && !circle(x, y, 84.0, 92.0, 22.0))
        || stroke(x, y, (116.0, 124.0), (212.0, 220.0), 26.0)
        || stroke(x, y, (176.0, 184.0), (206.0, 154.0), 22.0)
        || stroke(x, y, (146.0, 154.0), (176.0, 124.0), 22.0)
}

/// 파티션 — 상자 윤곽 + 세로·가로 칸막이.
fn shape_partition(x: f32, y: f32) -> bool {
    (rrect(x, y, 40.0, 48.0, 176.0, 160.0, 14.0) && !rrect(x, y, 60.0, 68.0, 136.0, 120.0, 6.0))
        || rect(x, y, 118.0, 138.0, 48.0, 208.0)
        || rect(x, y, 40.0, 216.0, 118.0, 138.0)
}

/// 확장 속성 — 꼬리표(몸통 + 뾰족한 끝 + 구멍).
fn shape_property(x: f32, y: f32) -> bool {
    (rrect(x, y, 48.0, 76.0, 128.0, 104.0, 14.0)
        || tri(x, y, (176.0, 76.0), (228.0, 128.0), (176.0, 180.0)))
        && !circle(x, y, 84.0, 128.0, 16.0)
}

/// 규칙·정책 — 방패.
fn shape_rule(x: f32, y: f32) -> bool {
    let body = rrect(x, y, 56.0, 44.0, 144.0, 120.0, 22.0)
        || tri(x, y, (56.0, 150.0), (200.0, 150.0), (128.0, 228.0));
    body && !(rrect(x, y, 80.0, 68.0, 96.0, 80.0, 12.0)
        || tri(x, y, (84.0, 144.0), (172.0, 144.0), (128.0, 196.0)))
        || rect(x, y, 118.0, 138.0, 60.0, 196.0)
}

/// 큐 — 쌓인 막대 셋(위가 짧다 = 들어오는 순서).
fn shape_queue(x: f32, y: f32) -> bool {
    rrect(x, y, 72.0, 52.0, 136.0, 36.0, 12.0)
        || rrect(x, y, 56.0, 110.0, 152.0, 36.0, 12.0)
        || rrect(x, y, 40.0, 168.0, 176.0, 36.0, 12.0)
}

/// DB 링크 — 고리 둘이 겹친다.
fn shape_link(x: f32, y: f32) -> bool {
    (circle(x, y, 92.0, 128.0, 52.0) && !circle(x, y, 92.0, 128.0, 30.0))
        || (circle(x, y, 164.0, 128.0, 52.0) && !circle(x, y, 164.0, 128.0, 30.0))
}

/// 잡·스케줄러 — 시계(테 + 바늘 둘).
fn shape_job(x: f32, y: f32) -> bool {
    (circle(x, y, 128.0, 128.0, 96.0) && !circle(x, y, 128.0, 128.0, 74.0))
        || stroke(x, y, (128.0, 128.0), (128.0, 68.0), 22.0)
        || stroke(x, y, (128.0, 128.0), (176.0, 156.0), 22.0)
}

/// Java — 컵(몸통 + 손잡이 + 받침).
fn shape_java(x: f32, y: f32) -> bool {
    rrect(x, y, 52.0, 80.0, 128.0, 104.0, 18.0)
        || (circle(x, y, 190.0, 128.0, 40.0) && !circle(x, y, 190.0, 128.0, 20.0) && x > 170.0)
        || rrect(x, y, 40.0, 196.0, 176.0, 22.0, 10.0)
}

/// 도형 → `SIDE`×`SIDE` 커버리지(4×4 슈퍼샘플링).
fn rasterize(shape: fn(f32, f32) -> bool) -> Vec<u8> {
    let unit = 256.0 / SIDE as f32;
    let mut out = Vec::with_capacity((SIDE * SIDE) as usize);
    for py in 0..SIDE {
        for px in 0..SIDE {
            let mut hit = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) * unit;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) * unit;
                    if shape(x, y) {
                        hit += 1;
                    }
                }
            }
            out.push((hit * 255 / (SS * SS)) as u8);
        }
    }
    out
}

static MASKS: [OnceLock<Vec<u8>>; 24] = [const { OnceLock::new() }; 24];

/// 종류의 알파 마스크(처음 한 번 래스터 · 이후 재사용).
fn mask(kind: IconKind) -> &'static [u8] {
    MASKS[kind.idx()].get_or_init(|| rasterize(kind.shape()))
}

/// 틴트 이미지(호스트가 `(kind, rgb)`로 캐시한다).
pub(crate) fn image(kind: IconKind, rgb: (u8, u8, u8)) -> IconImage {
    IconImage::from_alpha_tinted(SIDE, SIDE, mask(kind), rgb)
}

/// ★ 시작 때 백그라운드에서 종류 아이콘 마스크 전부를 미리 굽는다(09-25 · 첫 표시 지연 0 · `OnceLock`이라 스레드 안전).
pub(crate) fn prewarm() {
    let _ = mask(IconKind::Dbms);
    let _ = mask(IconKind::Schema);
    let _ = mask(IconKind::Folder);
    let _ = mask(IconKind::Table);
    let _ = mask(IconKind::View);
    let _ = mask(IconKind::MatView);
    let _ = mask(IconKind::Procedure);
    let _ = mask(IconKind::Function);
    let _ = mask(IconKind::Package);
    let _ = mask(IconKind::PackageBody);
    let _ = mask(IconKind::Sequence);
    let _ = mask(IconKind::Trigger);
    let _ = mask(IconKind::Index);
    let _ = mask(IconKind::Synonym);
    let _ = mask(IconKind::Type);
    let _ = mask(IconKind::Column);
    let _ = mask(IconKind::Constraint);
    let _ = mask(IconKind::Partition);
    let _ = mask(IconKind::Property);
    let _ = mask(IconKind::Rule);
    let _ = mask(IconKind::Queue);
    let _ = mask(IconKind::Link);
    let _ = mask(IconKind::Job);
    let _ = mask(IconKind::Java);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_body() {
        for k in IconKind::ALL {
            let m = mask(k);
            let opaque = m.iter().filter(|&&a| a == 255).count();
            assert!(opaque > 20, "{k:?} too empty");
            assert!(opaque < (SIDE * SIDE) as usize * 9 / 10, "{k:?} too full");
        }
    }
}
