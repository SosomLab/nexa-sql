//! 툴바 아이콘 — **코드로 그린 알파 마스크**(사용자 09-14 "글꼴이 아니라 이미지로"). 정적 자원 0바이트.
//!
//! 256 좌표계의 도형을 4×4 슈퍼샘플링으로 `SIDE`×`SIDE` 1채널 커버리지로 래스터해 [`ToolIcon::Mask`]로 넘긴다 —
//! 색은 툴바가 테마 기준색(hover/pressed = accent)으로 틴트하므로 여기선 **모양만** 정의한다.
//! 마스크는 프로세스 수명 동안 한 번만 만들어 `Box::leak`(5개 × 4KB · 툴바가 `&'static`을 요구).

use nexa_ctl::{MenuIcon, ToolIcon};

/// 마스크 한 변(px) — 툴바 슬롯(≈24~32px)에 contain 맞춤이라 64면 어떤 배율에서도 충분하다.
const SIDE: u32 = 64;
const SS: u32 = 4;

/// 점–선분 거리(둥근 끝 획).
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

fn in_rounded_rect(x: f32, y: f32, x0: f32, y0: f32, w: f32, h: f32, r: f32) -> bool {
    if x < x0 || y < y0 || x > x0 + w || y > y0 + h {
        return false;
    }
    let cx = x.clamp(x0 + r, x0 + w - r);
    let cy = y.clamp(y0 + r, y0 + h - r);
    (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r
}

fn in_triangle(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let s = |p: (f32, f32), q: (f32, f32)| (x - q.0) * (p.1 - q.1) - (p.0 - q.0) * (y - q.1);
    let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// 삼각형을 무게중심 쪽으로 `k`배 축소(외곽선용 안쪽 삼각형).
fn shrink(a: (f32, f32), b: (f32, f32), c: (f32, f32), k: f32) -> [(f32, f32); 3] {
    let g = ((a.0 + b.0 + c.0) / 3.0, (a.1 + b.1 + c.1) / 3.0);
    let f = |p: (f32, f32)| (g.0 + (p.0 - g.0) * k, g.1 + (p.1 - g.1) * k);
    [f(a), f(b), f(c)]
}

// ── 도형(256 좌표계)

/// ＋ — 새 스크립트.
/// Material 좌표(viewBox 960) → 이 도형 좌표(256).
const M: f32 = 256.0 / 960.0;

/// 점이 다각형 안인가(짝홀 규칙 · 오목 다각형 가능) — Material SVG의 직선 경로를 좌표 그대로 옮길 때.
fn in_poly(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn shape_new(x: f32, y: f32) -> bool {
    // Material `note_add`계(사용자 SVG 09-16): 둥근 사각 틀(두께 80 · 오른쪽 위가 열림) + 오른쪽 위 십자.
    let (x, y) = (x / M, y / M); // 960 좌표계에서 판정
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| x >= x0 && x <= x1 && y >= y0 && y <= y1;
    let frame = in_rounded_rect(x, y, 120.0, 120.0, 720.0, 720.0, 80.0)
        && !rect(200.0, 200.0, 760.0, 760.0);
    let notch = (x > 440.0 && y < 200.0) || (x > 760.0 && y < 600.0);
    let plus = rect(680.0, 320.0, 760.0, 560.0) || rect(520.0, 400.0, 920.0, 480.0);
    (frame && !notch) || plus
}

/// 📁 — 열기(폴더 외곽선 + 탭).
fn shape_open(x: f32, y: f32) -> bool {
    // Material `file_open`(사용자 SVG 09-16): 접힌 귀 문서 외곽선(왼쪽 모서리 r80) + 오른쪽 아래 ↘ 내보내기 화살표.
    let (x, y) = (x / M, y / M);
    let doc = in_poly(
        x,
        y,
        &[
            (160.0, 880.0),
            (160.0, 80.0),
            (560.0, 80.0),
            (800.0, 320.0),
            (800.0, 560.0),
            (720.0, 560.0),
            (720.0, 360.0),
            (520.0, 360.0),
            (520.0, 160.0),
            (240.0, 160.0),
            (240.0, 800.0),
            (600.0, 800.0),
            (600.0, 880.0),
        ],
    ) && in_rounded_rect(x, y, 160.0, 80.0, 640.0, 800.0, 80.0);
    let arrow = in_poly(
        x,
        y,
        &[
            (878.0, 895.0),
            (760.0, 777.0),
            (760.0, 866.0),
            (680.0, 866.0),
            (680.0, 640.0),
            (906.0, 640.0),
            (906.0, 720.0),
            (816.0, 720.0),
            (934.0, 838.0),
        ],
    );
    doc || arrow
}

/// 💾 — 저장(플로피: 외곽선 + 위 슬롯 + 아래 라벨).
fn shape_save(x: f32, y: f32) -> bool {
    // Material `save`(사용자 SVG 09-16 재변경): 플로피 — 둥근 틀(오른쪽 위 모서리 사선) + 라벨 + 원판.
    let (x, y) = (x / M, y / M);
    let frame = in_rounded_rect(x, y, 120.0, 120.0, 720.0, 720.0, 80.0) && x - y <= 560.0;
    let hole = in_poly(
        x,
        y,
        &[
            (760.0, 314.0),
            (646.0, 200.0),
            (200.0, 200.0),
            (200.0, 760.0),
            (760.0, 760.0),
        ],
    );
    let label = (240.0..=600.0).contains(&x) && (240.0..=400.0).contains(&y);
    let disc = (x - 480.0) * (x - 480.0) + (y - 600.0) * (y - 600.0) <= 120.0 * 120.0;
    (frame && !hole) || label || disc
}

fn shape_save_as(x: f32, y: f32) -> bool {
    // Material `save_as`(사용자 SVG 09-16): 플로피(오른쪽 아래 비움) + 라벨 + 원판(사선 절단) + 연필 외곽선.
    let (x, y) = (x / M, y / M);
    let frame = in_poly(
        x,
        y,
        &[
            (120.0, 840.0),
            (120.0, 120.0),
            (680.0, 120.0),
            (840.0, 280.0),
            (840.0, 492.0),
            (760.0, 482.0),
            (760.0, 313.0),
            (647.0, 200.0),
            (200.0, 200.0),
            (200.0, 760.0),
            (440.0, 760.0),
            (440.0, 840.0),
        ],
    ) && in_rounded_rect(x, y, 120.0, 120.0, 720.0, 720.0, 80.0);
    let label = (240.0..=600.0).contains(&x) && (240.0..=400.0).contains(&y);
    let disc =
        (x - 480.0) * (x - 480.0) + (y - 600.0) * (y - 600.0) <= 120.0 * 120.0 && x + y <= 1204.0;
    let pencil = in_poly(
        x,
        y,
        &[
            (520.0, 920.0),
            (520.0, 797.0),
            (741.0, 577.0),
            (803.0, 560.0),
            (860.0, 600.0),
            (863.0, 700.0),
            (643.0, 920.0),
        ],
    ) && !in_poly(
        x,
        y,
        &[
            (580.0, 860.0),
            (618.0, 860.0),
            (739.0, 738.0),
            (702.0, 701.0),
            (580.0, 822.0),
        ],
    );
    frame || label || disc || pencil
}

const TRI: [(f32, f32); 3] = [(76.0, 44.0), (212.0, 128.0), (76.0, 212.0)];

/// ▷ — 한 문장 실행(외곽선).
fn shape_run_statement(x: f32, y: f32) -> bool {
    let [a, b, c] = TRI;
    let [ia, ib, ic] = shrink(a, b, c, 0.62);
    in_triangle(x, y, a, b, c) && !in_triangle(x, y, ia, ib, ic)
}

/// ▶ — 전체 실행(채움).
fn shape_run_all(x: f32, y: f32) -> bool {
    let [a, b, c] = TRI;
    in_triangle(x, y, a, b, c)
}

/// 둥근 모서리 깎기 — 모서리 사각 영역 안이면서 원 밖이면 제외(Material q 곡선 근사 · r80).
fn corner_ok(x: f32, y: f32, cx: f32, cy: f32, quad_x: bool, quad_y: bool) -> bool {
    let qx = if quad_x { x > cx } else { x < cx };
    let qy = if quad_y { y > cy } else { y < cy };
    let in_quad = qx && qy;
    !in_quad || (x - cx) * (x - cx) + (y - cy) * (y - cy) <= 80.0 * 80.0
}

fn shape_connect(x: f32, y: f32) -> bool {
    // Material `power`(사용자 SVG 09-16): 플러그 몸통 외곽선(위 모서리 r80) + 핀 2개 − 안쪽 구멍.
    let (x, y) = (x / M, y / M);
    let body = in_poly(
        x,
        y,
        &[
            (380.0, 840.0),
            (380.0, 720.0),
            (240.0, 580.0),
            (240.0, 280.0),
            (720.0, 280.0),
            (720.0, 580.0),
            (580.0, 720.0),
            (580.0, 840.0),
        ],
    ) && corner_ok(x, y, 320.0, 360.0, false, false)
        && corner_ok(x, y, 640.0, 360.0, true, false);
    let pins = ((320.0..=400.0).contains(&x) || (560.0..=640.0).contains(&x))
        && (120.0..=280.0).contains(&y);
    let hole = in_poly(
        x,
        y,
        &[
            (460.0, 760.0),
            (500.0, 760.0),
            (500.0, 686.0),
            (640.0, 546.0),
            (640.0, 360.0),
            (320.0, 360.0),
            (320.0, 546.0),
            (460.0, 686.0),
        ],
    );
    (body || pins) && !hole
}

/// 접속 해제 — Material `power_off`(사용자 SVG 09-16): 플러그를 사선이 가르고 아래·위 조각으로 나뉜 모양.
fn shape_disconnect(x: f32, y: f32) -> bool {
    let (x, y) = (x / M, y / M);
    // 아래 조각 + 사선(한 다각형 · SVG 꼭짓점 그대로 · 왼쪽 위 모서리 곡선은 (283,283) 꼭짓점으로 근사).
    let lower = in_poly(
        x,
        y,
        &[
            (380.0, 840.0),
            (380.0, 720.0),
            (240.0, 580.0),
            (240.0, 360.0),
            (283.0, 283.0),
            (360.0, 360.0),
            (320.0, 360.0),
            (320.0, 546.0),
            (460.0, 686.0),
            (460.0, 760.0),
            (500.0, 760.0),
            (500.0, 686.0),
            (537.0, 649.0),
            (56.0, 168.0),
            (112.0, 112.0),
            (848.0, 848.0),
            (792.0, 904.0),
            (594.0, 706.0),
            (580.0, 720.0),
            (580.0, 840.0),
        ],
    );
    // 위 조각(핀 둘 + 오른쪽 위 몸통 · 오른쪽 위 모서리 r80).
    let upper = in_poly(
        x,
        y,
        &[
            (686.0, 572.0),
            (640.0, 526.0),
            (640.0, 360.0),
            (474.0, 360.0),
            (320.0, 206.0),
            (320.0, 120.0),
            (400.0, 120.0),
            (400.0, 280.0),
            (560.0, 280.0),
            (560.0, 120.0),
            (640.0, 120.0),
            (640.0, 320.0),
            (600.0, 280.0),
            (640.0, 280.0),
            (720.0, 280.0),
            (720.0, 538.0),
        ],
    ) && corner_ok(x, y, 640.0, 360.0, true, false);
    lower || upper
}

/// ≡ — 로그 창.
fn shape_log(x: f32, y: f32) -> bool {
    [72.0, 128.0, 184.0]
        .iter()
        .any(|&cy| stroke(x, y, (52.0, cy), (204.0, cy), 22.0))
}

// ── 메뉴 아이콘 도형(우클릭 메뉴 · DBeaver식 · 사용자 09-15) ──

/// 복사 — 겹친 사각형 두 장(외곽선).
fn shape_copy(x: f32, y: f32) -> bool {
    let back = in_rounded_rect(x, y, 48.0, 40.0, 120.0, 140.0, 12.0)
        && !in_rounded_rect(x, y, 70.0, 62.0, 76.0, 96.0, 6.0)
        && !in_rounded_rect(x, y, 96.0, 84.0, 120.0, 140.0, 12.0);
    let front = in_rounded_rect(x, y, 96.0, 84.0, 120.0, 140.0, 12.0)
        && !in_rounded_rect(x, y, 118.0, 106.0, 76.0, 96.0, 6.0);
    back || front
}

/// 잘라내기 — 가위(고리 둘 + 날 둘).
fn shape_cut(x: f32, y: f32) -> bool {
    let ring = |cx: f32, cy: f32| {
        let d = ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt();
        (26.0..=44.0).contains(&d)
    };
    ring(76.0, 188.0)
        || ring(180.0, 188.0)
        || stroke(x, y, (96.0, 156.0), (196.0, 44.0), 22.0)
        || stroke(x, y, (160.0, 156.0), (60.0, 44.0), 22.0)
}

/// 붙여넣기 — 클립보드(판 + 집게).
fn shape_paste(x: f32, y: f32) -> bool {
    let board = in_rounded_rect(x, y, 56.0, 60.0, 144.0, 156.0, 14.0)
        && !in_rounded_rect(x, y, 78.0, 82.0, 100.0, 112.0, 6.0);
    let clip = in_rounded_rect(x, y, 96.0, 40.0, 64.0, 40.0, 10.0)
        && !in_rounded_rect(x, y, 112.0, 52.0, 32.0, 16.0, 4.0);
    board || clip
}

/// 전체 선택 — 점선 사각형(모서리 4개 + 변 중앙 점).
fn shape_select_all(x: f32, y: f32) -> bool {
    let corner = |cx: f32, cy: f32| in_rounded_rect(x, y, cx - 18.0, cy - 18.0, 36.0, 36.0, 4.0);
    let dot = |cx: f32, cy: f32| in_rounded_rect(x, y, cx - 11.0, cy - 11.0, 22.0, 22.0, 4.0);
    corner(48.0, 48.0)
        || corner(208.0, 48.0)
        || corner(48.0, 208.0)
        || corner(208.0, 208.0)
        || dot(128.0, 48.0)
        || dot(128.0, 208.0)
        || dot(48.0, 128.0)
        || dot(208.0, 128.0)
}

/// 표(CSV·텍스트·Markdown) — 격자.
fn shape_table(x: f32, y: f32) -> bool {
    let frame = in_rounded_rect(x, y, 40.0, 48.0, 176.0, 160.0, 12.0)
        && !in_rounded_rect(x, y, 60.0, 68.0, 136.0, 120.0, 4.0);
    frame
        || stroke(x, y, (40.0, 104.0), (216.0, 104.0), 18.0)
        || stroke(x, y, (40.0, 152.0), (216.0, 152.0), 18.0)
        || stroke(x, y, (128.0, 48.0), (128.0, 208.0), 18.0)
}

/// JSON — 중괄호 한 쌍.
fn shape_braces(x: f32, y: f32) -> bool {
    let left = stroke(x, y, (100.0, 44.0), (72.0, 44.0), 22.0)
        || stroke(x, y, (72.0, 44.0), (72.0, 116.0), 22.0)
        || stroke(x, y, (72.0, 116.0), (48.0, 128.0), 22.0)
        || stroke(x, y, (48.0, 128.0), (72.0, 140.0), 22.0)
        || stroke(x, y, (72.0, 140.0), (72.0, 212.0), 22.0)
        || stroke(x, y, (72.0, 212.0), (100.0, 212.0), 22.0);
    let right = stroke(x, y, (156.0, 44.0), (184.0, 44.0), 22.0)
        || stroke(x, y, (184.0, 44.0), (184.0, 116.0), 22.0)
        || stroke(x, y, (184.0, 116.0), (208.0, 128.0), 22.0)
        || stroke(x, y, (208.0, 128.0), (184.0, 140.0), 22.0)
        || stroke(x, y, (184.0, 140.0), (184.0, 212.0), 22.0)
        || stroke(x, y, (184.0, 212.0), (156.0, 212.0), 22.0);
    left || right
}

/// SQL — 데이터베이스 원통.
fn shape_db(x: f32, y: f32) -> bool {
    let ell = |cy: f32, rx: f32, ry: f32| {
        let nx = (x - 128.0) / rx;
        let ny = (y - cy) / ry;
        nx * nx + ny * ny
    };
    let top = {
        let d = ell(72.0, 88.0, 32.0);
        (0.55..=1.0).contains(&d)
    };
    let mid = {
        let d = ell(128.0, 88.0, 32.0);
        (0.55..=1.0).contains(&d) && y >= 128.0
    };
    let bot = {
        let d = ell(184.0, 88.0, 32.0);
        (0.55..=1.0).contains(&d) && y >= 184.0
    };
    let sides = (x - 40.0).abs() <= 11.0 && (72.0..=184.0).contains(&y)
        || (x - 216.0).abs() <= 11.0 && (72.0..=184.0).contains(&y);
    top || mid || bot || sides
}

/// 활동 막대 — 탐색기(겹친 문서 두 장 · VS Code Explorer 느낌).
fn shape_files(x: f32, y: f32) -> bool {
    let back = in_rounded_rect(x, y, 44.0, 36.0, 120.0, 150.0, 10.0)
        && !in_rounded_rect(x, y, 62.0, 54.0, 84.0, 114.0, 6.0)
        && !in_rounded_rect(x, y, 92.0, 76.0, 124.0, 150.0, 10.0);
    let front = in_rounded_rect(x, y, 92.0, 76.0, 124.0, 150.0, 10.0)
        && !in_rounded_rect(x, y, 110.0, 94.0, 88.0, 114.0, 6.0);
    let lines = stroke(x, y, (128.0, 130.0), (188.0, 130.0), 12.0)
        || stroke(x, y, (128.0, 160.0), (188.0, 160.0), 12.0)
        || stroke(x, y, (128.0, 190.0), (170.0, 190.0), 12.0);
    back || front || lines
}

/// 활동 막대 — 접속(플러그 = 툴바 접속 아이콘 재사용).
fn shape_plug(x: f32, y: f32) -> bool {
    shape_connect(x, y)
}

/// 활동 막대 — 환경 설정(톱니: 고리 + 이 8개).
fn shape_gear(x: f32, y: f32) -> bool {
    let (cx, cy) = (128.0, 128.0);
    let d = ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt();
    let ring = (46.0..=84.0).contains(&d);
    let mut tooth = false;
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::PI / 4.0;
        let (tx, ty) = (cx + a.cos() * 88.0, cy + a.sin() * 88.0);
        tooth |= stroke(
            x,
            y,
            (cx + a.cos() * 70.0, cy + a.sin() * 70.0),
            (tx, ty),
            30.0,
        );
    }
    ring || tooth
}

pub(crate) fn mi_files() -> MenuIcon {
    menu_icon(shape_files)
}
pub(crate) fn mi_plug() -> MenuIcon {
    menu_icon(shape_plug)
}
pub(crate) fn mi_gear() -> MenuIcon {
    menu_icon(shape_gear)
}

/// 메뉴 아이콘(알파 마스크 · 색은 메뉴가 상태색으로 틴트).
fn menu_icon(shape: fn(f32, f32) -> bool) -> MenuIcon {
    MenuIcon::from_alpha(SIDE, SIDE, rasterize(shape))
}

pub(crate) fn mi_copy() -> MenuIcon {
    menu_icon(shape_copy)
}
pub(crate) fn mi_cut() -> MenuIcon {
    menu_icon(shape_cut)
}
pub(crate) fn mi_paste() -> MenuIcon {
    menu_icon(shape_paste)
}
pub(crate) fn mi_select_all() -> MenuIcon {
    menu_icon(shape_select_all)
}
pub(crate) fn mi_table() -> MenuIcon {
    menu_icon(shape_table)
}
pub(crate) fn mi_braces() -> MenuIcon {
    menu_icon(shape_braces)
}
pub(crate) fn mi_db() -> MenuIcon {
    menu_icon(shape_db)
}

/// 도형 → `SIDE`×`SIDE` 커버리지(4×4 슈퍼샘플링) — 한 번 만들어 영구 보관.
fn rasterize(shape: fn(f32, f32) -> bool) -> &'static [u8] {
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
    Box::leak(out.into_boxed_slice())
}

fn mask(shape: fn(f32, f32) -> bool) -> ToolIcon {
    ToolIcon::Mask {
        w: SIDE,
        h: SIDE,
        alpha: rasterize(shape),
    }
}

pub(crate) fn new_script() -> ToolIcon {
    mask(shape_new)
}
pub(crate) fn open_file() -> ToolIcon {
    mask(shape_open)
}
pub(crate) fn save_file() -> ToolIcon {
    mask(shape_save)
}
/// 다른 이름으로 저장(Material `save_as` · 사용자 SVG 09-16).
pub(crate) fn save_as() -> ToolIcon {
    mask(shape_save_as)
}
pub(crate) fn run_statement() -> ToolIcon {
    mask(shape_run_statement)
}
pub(crate) fn run_all() -> ToolIcon {
    mask(shape_run_all)
}
pub(crate) fn connect() -> ToolIcon {
    mask(shape_connect)
}
pub(crate) fn log() -> ToolIcon {
    mask(shape_log)
}
pub(crate) fn disconnect() -> ToolIcon {
    mask(shape_disconnect)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(shape: fn(f32, f32) -> bool) -> (usize, usize) {
        let m = rasterize(shape);
        let opaque = m.iter().filter(|&&a| a == 255).count();
        let partial = m.iter().filter(|&&a| a > 0 && a < 255).count();
        (opaque, partial)
    }

    #[test]
    fn every_icon_has_body_and_antialiased_edge() {
        for s in [
            shape_new as fn(f32, f32) -> bool,
            shape_run_statement,
            shape_run_all,
            shape_connect,
            shape_log,
            shape_disconnect,
            shape_open,
            shape_save,
            shape_copy,
            shape_cut,
            shape_paste,
            shape_select_all,
            shape_table,
            shape_braces,
            shape_db,
            shape_files,
            shape_plug,
            shape_gear,
        ] {
            let (opaque, partial) = coverage(s);
            assert!(opaque > 100, "채워진 픽셀이 있어야 한다: {opaque}");
            assert!(partial > 20, "가장자리는 부분 커버리지(AA): {partial}");
            assert!(opaque < (SIDE * SIDE) as usize / 2, "슬롯을 다 덮지 않는다");
        }
    }

    /// Material `power`(09-16): 핀 둘 · 몸통 외곽선 · 안쪽 구멍 · 핀 사이는 빔. 좌표는 SVG(960)를 M으로 줄인 것.
    #[test]
    fn connect_plug_pins_outline_and_hole() {
        let at = |x: f32, y: f32| shape_connect(x * M, y * M);
        assert!(at(360.0, 200.0), "왼쪽 핀");
        assert!(at(600.0, 200.0), "오른쪽 핀");
        assert!(!at(480.0, 200.0), "핀 사이는 빔");
        assert!(at(280.0, 480.0), "몸통 왼쪽 벽");
        assert!(!at(480.0, 480.0), "안쪽 구멍");
        assert!(at(480.0, 800.0), "아래 목");
        assert!(!at(250.0, 290.0), "둥근 모서리 밖");
        // 해제: 사선이 지나가고 핀·아래 목은 남는다.
        let off = |x: f32, y: f32| shape_disconnect(x * M, y * M);
        assert!(off(480.0, 536.0), "사선 띠 가운데(y = x + 56)");
        assert!(off(360.0, 200.0), "핀");
        assert!(off(480.0, 800.0), "아래 목");
        assert!(!off(480.0, 420.0), "사선 위쪽 · 구멍 자리는 빔");
    }
}
