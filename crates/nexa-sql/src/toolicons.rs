//! 툴바 아이콘 — **코드로 그린 알파 마스크**(사용자 09-14 "글꼴이 아니라 이미지로"). 정적 자원 0바이트.
//!
//! 256 좌표계의 도형을 4×4 슈퍼샘플링으로 `SIDE`×`SIDE` 1채널 커버리지로 래스터해 [`ToolIcon::Mask`]로 넘긴다 —
//! 색은 툴바가 테마 기준색(hover/pressed = accent)으로 틴트하므로 여기선 **모양만** 정의한다.
//! 마스크는 **모양마다 프로세스에서 한 번만** 래스터해 `Box::leak`(`memo` · 툴바가 `&'static`을 요구) — 09-22 Linux 기동 계측: 메모 없이
//! 호출마다 다시 래스터하면 SVG 경로 아이콘이 개당 ≈15 ms(64×64×16 표본 × 다각형 전체 검사)라 찾기 막대 11개 = 166 ms · 결과 탭마다 20개.

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
/// 보기 모드(사용자 09-16 SVG · Material `table` 계열): 둥근 사각 외곽에서 6칸(좁은 왼쪽 열 + 넓은 오른쪽 열 × 3행)을 뺀다.
fn shape_view_mode(x: f32, y: f32) -> bool {
    // SVG: 외곽 (80,160)-(880,800) r≈80 · 칸 x 160..280 / 360..800 · y 240..347 / 427..534 / 613..720 (y = svg + 960).
    let (sx, sy) = (x / M, y / M);
    if !in_rounded_rect(sx, sy, 80.0, 160.0, 800.0, 640.0, 80.0) {
        return false;
    }
    let rows = [(240.0, 347.0), (427.0, 534.0), (613.0, 720.0)];
    let cols = [(160.0, 280.0), (360.0, 800.0)];
    for (y0, y1) in rows {
        for (x0, x1) in cols {
            if sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1 {
                return false;
            }
        }
    }
    true
}

/// 새로고침(Material `refresh` 느낌): 원호(270° · 굵기 22) + 화살촉(오른쪽 위 끝).
fn shape_refresh(x: f32, y: f32) -> bool {
    let (cx, cy) = (128.0, 132.0);
    let (dx, dy) = (x - cx, y - cy);
    let r = (dx * dx + dy * dy).sqrt();
    let ring = (r - 76.0).abs() <= 11.0;
    // 각도(위 = -90°) · 오른쪽 위 90°를 비운다(-90°..0°).
    let ang = dy.atan2(dx).to_degrees();
    let gap = (-90.0..0.0).contains(&ang);
    let arc = ring && !gap;
    // 화살촉: 원호 끝(0° 지점 · 오른쪽)에서 위쪽으로.
    let tip = in_triangle(
        x,
        y,
        (cx + 76.0, cy - 40.0),
        (cx + 44.0, cy + 2.0),
        (cx + 108.0, cy + 2.0),
    );
    arc || tip
}

/// 전체 조회(사용자 SVG · Material `select_all`): 점선 테두리(모서리 둥근 4각 + 변마다 3칸) + 안쪽 고리 사각.
fn shape_fetch_all(x: f32, y: f32) -> bool {
    let (sx, sy) = (x / M, y / M);
    // 안쪽 사각 고리: 280..680 에서 360..600 을 뺀다.
    let inner = (280.0..=680.0).contains(&sx)
        && (280.0..=680.0).contains(&sy)
        && !((360.0..=600.0).contains(&sx) && (360.0..=600.0).contains(&sy));
    if inner {
        return true;
    }
    let in_rect = |x0: f32, y0: f32, x1: f32, y1: f32| sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1;
    // 네 모서리(바깥쪽 둥글게).
    let corner = |x0: f32, y0: f32, x1: f32, y1: f32, cx: f32, cy: f32| {
        in_rect(x0, y0, x1, y1) && ((sx - cx) * (sx - cx) + (sy - cy) * (sy - cy) <= 80.0 * 80.0)
    };
    if corner(120.0, 120.0, 200.0, 200.0, 200.0, 200.0)
        || corner(760.0, 120.0, 840.0, 200.0, 760.0, 200.0)
        || corner(120.0, 760.0, 200.0, 840.0, 200.0, 760.0)
        || corner(760.0, 760.0, 840.0, 840.0, 760.0, 760.0)
    {
        return true;
    }
    // 변마다 3칸.
    let dash = [(280.0, 360.0), (440.0, 520.0), (600.0, 680.0)];
    for (a, b) in dash {
        if in_rect(a, 120.0, b, 200.0)
            || in_rect(a, 760.0, b, 840.0)
            || in_rect(120.0, a, 200.0, b)
            || in_rect(760.0, a, 840.0, b)
        {
            return true;
        }
    }
    false
}

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

/// 활동 막대 — 확장(붙은 네모 셋 + 떨어진 네모 하나 · VS Code Extensions 느낌 · 사용자 09-19).
fn shape_extensions(x: f32, y: f32) -> bool {
    let sq = |x0: f32, y0: f32| {
        in_rounded_rect(x, y, x0, y0, 70.0, 70.0, 8.0)
            && !in_rounded_rect(x, y, x0 + 13.0, y0 + 13.0, 44.0, 44.0, 4.0)
    };
    sq(40.0, 88.0) || sq(40.0, 146.0) || sq(98.0, 146.0) || sq(128.0, 44.0)
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
pub(crate) fn mi_extensions() -> MenuIcon {
    menu_icon(shape_extensions)
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
/// 모양별 메모(키 = 함수 포인터 또는 SVG 경로의 주소·길이·종류). 값은 프로세스 수명 동안 산다.
fn memo(key: (usize, usize, u8), make: impl FnOnce() -> Vec<u8>) -> &'static [u8] {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Cache = Mutex<HashMap<(usize, usize, u8), &'static [u8]>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let m = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(a) = m.lock().ok().and_then(|g| g.get(&key).copied()) {
        return a;
    }
    let a: &'static [u8] = Box::leak(make().into_boxed_slice());
    if let Ok(mut g) = m.lock() {
        g.insert(key, a);
    }
    a
}

fn rasterize(shape: fn(f32, f32) -> bool) -> &'static [u8] {
    memo((shape as usize, 0, 0), || rasterize_raw(shape))
}

fn rasterize_raw(shape: fn(f32, f32) -> bool) -> Vec<u8> {
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
/// 결과 도구줄 보기 모드 버튼(▾는 툴바가 붙인다).
pub(crate) fn view_mode() -> ToolIcon {
    mask(shape_view_mode)
}
/// 결과 도구줄 새로고침.
pub(crate) fn refresh() -> ToolIcon {
    mask(shape_refresh)
}
/// 결과 도구줄 전체 조회(사용자 SVG 09-16).
pub(crate) fn fetch_all() -> ToolIcon {
    mask(shape_fetch_all)
}
/// 결과 도구줄 가져오기 중지(Material `stop_circle` · T-48b).
pub(crate) fn fetch_stop() -> ToolIcon {
    svg_tool("M320-320h320v-320H320v320ZM480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0 227-93t93-227q0-134-93-227t-227-93q-134 0-227 93t-93 227q0 134 93 227t227 93Zm0-320Z")
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

    /// `.5-.526`처럼 점으로 시작하는 수(codicon)가 토큰으로 나뉜다 — 예전엔 무한 루프.
    #[test]
    fn svg_tokens_dot_leading_numbers() {
        let v: Vec<f32> = svg_tokens("a.523.523 0 0 1 .5-.526")
            .iter()
            .filter(|t| t.0 == '#')
            .map(|t| t.1)
            .collect();
        assert_eq!(v, vec![0.523, 0.523, 0.0, 0.0, 1.0, 0.5, -0.526]);
    }

    /// codicon `replace`(16 viewBox · 호 `a` · evenodd): 상자 테두리는 채워지고 상자 속(구멍)은 빈다.
    #[test]
    fn codicon_replace_arc_and_evenodd() {
        let d = "M4 7L3 8v6l1 1h7l1-1V8l-1-1H4zm0 1h7v6H4V8zM5 2.489A1.482 1.482 0 0 1 6.48 1H8v.965H6.48a.523.523 0 0 0-.5.526V4.1H5z";
        let polys = svg_polys_vb(d, VB_CODICON);
        let k = 960.0 / 16.0;
        assert!(
            in_polys_evenodd(3.5 * k, 11.0 * k, &polys),
            "왼쪽 테두리(x 3~4)"
        );
        assert!(
            !in_polys_evenodd(7.5 * k, 11.0 * k, &polys),
            "상자 속은 구멍"
        );
        assert!(
            in_polys_evenodd(5.4 * k, 3.5 * k, &polys),
            "호로 이어진 세로획(x 5~6)"
        );
        let icon = svg_glyph_vb(d, VB_CODICON, true);
        assert_eq!(icon.w, SIDE);
    }
}

// ───────────────────────── Material Symbols SVG 경로 → 마스크(사용자 09-16 "구글 머티리얼 아이콘 SVG 그대로") ─────────────────────────
//
// `d` 문자열(M/L/H/V/C/S/Q/T/Z · 상대/절대 · 암묵 반복)을 다각형으로 펴고(베지어 = 선분 근사) **비영(non-zero) winding**으로
// 채운다 — Material 경로는 구멍을 반대 방향으로 감으므로 짝홀보다 비영이 원본과 같다. 좌표계 `viewBox="0 -960 960 960"`
// → y + 960 → 256 좌표. 아이콘은 프로세스 수명 동안 한 번 래스터해 `OnceLock`에 둔다.

/// 경로 토큰(명령 글자 · 숫자).
fn svg_tokens(d: &str) -> Vec<(char, f32)> {
    let mut out = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() {
            out.push((c, 0.0));
            i += 1;
        } else if c == '-' || c == '.' || c.is_ascii_digit() {
            let start = i;
            // 앞글자('-' 또는 '.')를 먼저 소비한다 — '.'로 시작하는 수(".5" · codicon)에서 인덱스가 멈춰 무한 루프였다(09-17).
            if c == '-' || c == '.' {
                i += 1;
            }
            let mut dot = c == '.';
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !dot {
                    dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            if let Ok(v) = d[start..i].parse::<f32>() {
                out.push(('#', v));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Material Symbols viewBox(`0 -960 960 960`).
const VB_MATERIAL: [f32; 4] = [0.0, -960.0, 960.0, 960.0];
/// VS Code codicon viewBox(`0 0 16 16`).
const VB_CODICON: [f32; 4] = [0.0, 0.0, 16.0, 16.0];

/// 경로 → 닫힌 다각형들(960 좌표 · Material viewBox).
fn svg_polys(d: &str) -> Vec<Vec<(f32, f32)>> {
    svg_polys_vb(d, VB_MATERIAL)
}

/// 타원호(SVG F.6.5 끝점 → 중심 변환) → 선분 근사.
#[allow(clippy::too_many_arguments)]
fn flatten_arc(
    cur: &mut Vec<(f32, f32)>,
    p0: (f32, f32),
    rx: f32,
    ry: f32,
    phi_deg: f32,
    large: bool,
    sweep: bool,
    p1: (f32, f32),
) {
    if p0 == p1 {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        cur.push(p1);
        return;
    }
    let phi = phi_deg.to_radians();
    let (cp, sp) = (phi.cos(), phi.sin());
    let dx2 = (p0.0 - p1.0) / 2.0;
    let dy2 = (p0.1 - p1.1) / 2.0;
    let x1p = cp * dx2 + sp * dy2;
    let y1p = -sp * dx2 + cp * dy2;
    let lambda = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let coef = (num / den).max(0.0).sqrt() * if large == sweep { -1.0 } else { 1.0 };
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cp * cxp - sp * cyp + (p0.0 + p1.0) / 2.0;
    let cy = sp * cxp + cp * cyp + (p0.1 + p1.1) / 2.0;
    let ang = |u: (f32, f32), v: (f32, f32)| -> f32 {
        let dot = u.0 * v.0 + u.1 * v.1;
        let len = (u.0 * u.0 + u.1 * u.1).sqrt() * (v.0 * v.0 + v.1 * v.1).sqrt();
        let a = (dot / len).clamp(-1.0, 1.0).acos();
        if u.0 * v.1 - u.1 * v.0 < 0.0 {
            -a
        } else {
            a
        }
    };
    let u = ((x1p - cxp) / rx, (y1p - cyp) / ry);
    let v = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let theta1 = ang((1.0, 0.0), u);
    let mut dtheta = ang(u, v);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }
    const N: usize = 16;
    for k in 1..=N {
        let t = theta1 + dtheta * k as f32 / N as f32;
        let (ct, st) = (t.cos(), t.sin());
        cur.push((
            cx + rx * ct * cp - ry * st * sp,
            cy + rx * ct * sp + ry * st * cp,
        ));
    }
}

/// 경로 → 닫힌 다각형들(960 좌표) — `vb` = viewBox(min-x · min-y · w · h)를 960 정사각으로 맞춘다.
fn svg_polys_vb(d: &str, vb: [f32; 4]) -> Vec<Vec<(f32, f32)>> {
    let toks = svg_tokens(d);
    let mut polys: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut cmd = 'M';
    let mut i = 0;
    let mut last_ctrl: Option<(f32, f32)> = None;
    let num = |toks: &[(char, f32)], i: &mut usize| -> Option<f32> {
        if *i < toks.len() && toks[*i].0 == '#' {
            let v = toks[*i].1;
            *i += 1;
            Some(v)
        } else {
            None
        }
    };
    let flatten_q = |cur: &mut Vec<(f32, f32)>, p0: (f32, f32), c: (f32, f32), p1: (f32, f32)| {
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            let u = 1.0 - t;
            cur.push((
                u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0,
                u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1,
            ));
        }
    };
    let flatten_c = |cur: &mut Vec<(f32, f32)>,
                     p0: (f32, f32),
                     c1: (f32, f32),
                     c2: (f32, f32),
                     p1: (f32, f32)| {
        for k in 1..=12 {
            let t = k as f32 / 12.0;
            let u = 1.0 - t;
            cur.push((
                u * u * u * p0.0
                    + 3.0 * u * u * t * c1.0
                    + 3.0 * u * t * t * c2.0
                    + t * t * t * p1.0,
                u * u * u * p0.1
                    + 3.0 * u * u * t * c1.1
                    + 3.0 * u * t * t * c2.1
                    + t * t * t * p1.1,
            ));
        }
    };
    while i < toks.len() {
        if toks[i].0 != '#' {
            cmd = toks[i].0;
            i += 1;
        }
        let rel = cmd.is_ascii_lowercase();
        let up = cmd.to_ascii_uppercase();
        match up {
            'M' => {
                let (Some(a), Some(b)) = (num(&toks, &mut i), num(&toks, &mut i)) else {
                    break;
                };
                if cur.len() > 1 {
                    polys.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                x = if rel { x + a } else { a };
                y = if rel { y + b } else { b };
                (sx, sy) = (x, y);
                cur.push((x, y));
                cmd = if rel { 'l' } else { 'L' };
                last_ctrl = None;
            }
            'L' => {
                let (Some(a), Some(b)) = (num(&toks, &mut i), num(&toks, &mut i)) else {
                    break;
                };
                x = if rel { x + a } else { a };
                y = if rel { y + b } else { b };
                cur.push((x, y));
                last_ctrl = None;
            }
            'H' => {
                let Some(a) = num(&toks, &mut i) else { break };
                x = if rel { x + a } else { a };
                cur.push((x, y));
                last_ctrl = None;
            }
            'V' => {
                let Some(b) = num(&toks, &mut i) else { break };
                y = if rel { y + b } else { b };
                cur.push((x, y));
                last_ctrl = None;
            }
            'Q' => {
                let (Some(a), Some(b), Some(c), Some(d2)) = (
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                ) else {
                    break;
                };
                let ctrl = if rel { (x + a, y + b) } else { (a, b) };
                let p1 = if rel { (x + c, y + d2) } else { (c, d2) };
                flatten_q(&mut cur, (x, y), ctrl, p1);
                last_ctrl = Some(ctrl);
                (x, y) = p1;
            }
            'T' => {
                let (Some(c), Some(d2)) = (num(&toks, &mut i), num(&toks, &mut i)) else {
                    break;
                };
                let ctrl = last_ctrl.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let p1 = if rel { (x + c, y + d2) } else { (c, d2) };
                flatten_q(&mut cur, (x, y), ctrl, p1);
                last_ctrl = Some(ctrl);
                (x, y) = p1;
            }
            'C' => {
                let (Some(a), Some(b), Some(c), Some(d2), Some(e), Some(f)) = (
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                ) else {
                    break;
                };
                let c1 = if rel { (x + a, y + b) } else { (a, b) };
                let c2 = if rel { (x + c, y + d2) } else { (c, d2) };
                let p1 = if rel { (x + e, y + f) } else { (e, f) };
                flatten_c(&mut cur, (x, y), c1, c2, p1);
                last_ctrl = Some(c2);
                (x, y) = p1;
            }
            'S' => {
                let (Some(c), Some(d2), Some(e), Some(f)) = (
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                ) else {
                    break;
                };
                let c1 = last_ctrl.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let c2 = if rel { (x + c, y + d2) } else { (c, d2) };
                let p1 = if rel { (x + e, y + f) } else { (e, f) };
                flatten_c(&mut cur, (x, y), c1, c2, p1);
                last_ctrl = Some(c2);
                (x, y) = p1;
            }
            'A' => {
                let (Some(rx), Some(ry), Some(rot), Some(la), Some(sw), Some(e), Some(f)) = (
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                    num(&toks, &mut i),
                ) else {
                    break;
                };
                let p1 = if rel { (x + e, y + f) } else { (e, f) };
                flatten_arc(&mut cur, (x, y), rx, ry, rot, la != 0.0, sw != 0.0, p1);
                last_ctrl = None;
                (x, y) = p1;
            }
            'Z' => {
                if cur.len() > 1 {
                    polys.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                (x, y) = (sx, sy);
                cur.push((x, y));
                last_ctrl = None;
            }
            _ => {
                i += 1;
            }
        }
    }
    if cur.len() > 2 {
        polys.push(cur);
    }
    let [vx, vy, vw, vh] = vb;
    for p in &mut polys {
        for v in p.iter_mut() {
            v.0 = (v.0 - vx) * 960.0 / vw;
            v.1 = (v.1 - vy) * 960.0 / vh;
        }
    }
    polys
}

/// 짝홀(even-odd) — `fill-rule="evenodd"` 경로(codicon)용.
fn in_polys_evenodd(x: f32, y: f32, polys: &[Vec<(f32, f32)>]) -> bool {
    let mut inside = false;
    for p in polys {
        let n = p.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (x0, y0) = p[i];
            let (x1, y1) = p[(i + 1) % n];
            if (y0 > y) != (y1 > y) && x < x0 + (y - y0) * (x1 - x0) / (y1 - y0) {
                inside = !inside;
            }
        }
    }
    inside
}

/// 비영 winding — 점이 다각형 집합 안인가.
fn in_polys_nonzero(x: f32, y: f32, polys: &[Vec<(f32, f32)>]) -> bool {
    let mut wn = 0i32;
    for p in polys {
        let n = p.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (x0, y0) = p[i];
            let (x1, y1) = p[(i + 1) % n];
            if y0 <= y {
                if y1 > y && (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0) > 0.0 {
                    wn += 1;
                }
            } else if y1 <= y && (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0) < 0.0 {
                wn -= 1;
            }
        }
    }
    wn != 0
}

/// 클로저 도형 → 커버리지(4×4 슈퍼샘플링).
fn rasterize_dyn(shape: &dyn Fn(f32, f32) -> bool) -> Vec<u8> {
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

/// Material Symbols `d` → 메뉴/버튼 아이콘 마스크(색은 그리는 쪽이 틴트).
pub(crate) fn svg_glyph(d: &str) -> MenuIcon {
    let alpha = memo((d.as_ptr() as usize, d.len(), 1), || {
        let polys = svg_polys(d);
        rasterize_dyn(&|x, y| in_polys_nonzero(x / M, y / M, &polys))
    });
    MenuIcon::from_alpha(SIDE, SIDE, alpha)
}

/// 임의 viewBox SVG `d` → 아이콘 마스크(`evenodd` = 짝홀 채움 · VS Code codicon).
pub(crate) fn svg_glyph_vb(d: &str, vb: [f32; 4], evenodd: bool) -> MenuIcon {
    let vbk = vb
        .iter()
        .fold(0usize, |a, v| a.rotate_left(13) ^ v.to_bits() as usize);
    let alpha = memo(
        (d.as_ptr() as usize, d.len() ^ vbk, 2 + u8::from(evenodd)),
        || {
            let polys = svg_polys_vb(d, vb);
            if evenodd {
                rasterize_dyn(&|x, y| in_polys_evenodd(x / M, y / M, &polys))
            } else {
                rasterize_dyn(&|x, y| in_polys_nonzero(x / M, y / M, &polys))
            }
        },
    );
    MenuIcon::from_alpha(SIDE, SIDE, alpha)
}

/// VS Code codicon 17×17 viewBox(`preserve-case` 계열).
const VB_CODICON17: [f32; 4] = [0.0, 0.0, 17.0, 17.0];

/// 임의 viewBox SVG 아이콘 — 사용자가 준 SVG 그대로(09-17). `codicon!` = 16×16 evenodd 사탕 표기.
macro_rules! svgicon {
    ($(#[$m:meta])* $name:ident, $vb:expr, $evenodd:expr, $d:expr) => {
        $(#[$m])*
        pub(crate) fn $name() -> MenuIcon {
            thread_local! {
                static CELL: std::cell::RefCell<Option<MenuIcon>> = const { std::cell::RefCell::new(None) };
            }
            CELL.with(|c| {
                c.borrow_mut()
                    .get_or_insert_with(|| svg_glyph_vb($d, $vb, $evenodd))
                    .clone()
            })
        }
    };
}

/// VS Code codicon(16×16 · evenodd · MIT).
macro_rules! codicon {
    ($(#[$m:meta])* $name:ident, $d:expr) => {
        svgicon!($(#[$m])* $name, VB_CODICON, true, $d);
    };
}

macro_rules! material {
    ($(#[$m:meta])* $name:ident, $d:expr) => {
        $(#[$m])*
        pub(crate) fn $name() -> MenuIcon {
            thread_local! {
                static CELL: std::cell::RefCell<Option<MenuIcon>> = const { std::cell::RefCell::new(None) };
            }
            CELL.with(|c| c.borrow_mut().get_or_insert_with(|| svg_glyph($d)).clone())
        }
    };
}

// Material Symbols Outlined 24px(viewBox 0 -960 960 960 · Apache-2.0) — 찾기 위젯(사용자 09-16 · VS Code 배치).
material!(/// `match_case` — 대소문자 구분(Aa).
    mi_match_case, "m131-252 165-440h79l165 440h-76l-39-112H247l-40 112h-76Zm139-176h131l-64-182h-4l-63 182Zm395 186q-51 0-81-27.5T554-342q0-44 34.5-72.5T677-443q23 0 45 4t38 11v-12q0-29-20.5-47T685-505q-23 0-42 9.5T610-468l-47-35q24-29 54.5-43t68.5-14q69 0 103 32.5t34 97.5v178h-63v-37h-4q-14 23-38 35t-53 12Zm12-54q35 0 59.5-24t24.5-56q-14-8-33.5-12.5T689-393q-32 0-50 14t-18 37q0 20 16 33t40 13Z");
material!(/// `match_word` — 단어 단위(ab 밑줄).
    mi_match_word, "M40-199v-200h80v120h720v-120h80v200H40Zm342-161v-34h-3q-13 20-35 31.5T294-351q-49 0-77-25.5T189-446q0-42 32.5-68.5T305-541q23 0 42.5 3.5T381-526v-14q0-27-18.5-43T312-599q-21 0-39.5 9T241-564l-43-32q19-27 48-41t67-14q62 0 95 29.5t33 85.5v176h-59Zm-66-134q-32 0-49 12.5T250-446q0 20 15 32.5t39 12.5q32 0 54.5-22.5T381-478q-14-8-32-12t-33-4Zm185 134v-401h62v113l-3 40h3q3-5 24-25.5t66-20.5q64 0 101 46t37 106q0 60-36.5 105.5T653-351q-41 0-62.5-18T563-397h-3v37h-59Zm143-238q-40 0-62 29.5T560-503q0 37 22 66t62 29q40 0 62.5-29t22.5-66q0-37-22.5-66T644-598Z");
material!(/// `regular_expression` — 정규식(.*).
    mi_regex, "M197-199q-56-57-86.5-130T80-482q0-80 30-153t87-130l57 57q-46 45-70 103.5T160-482q0 64 24.5 122.5T254-256l-57 57Zm183-41q-25 0-42.5-17.5T320-300q0-25 17.5-42.5T380-360q25 0 42.5 17.5T440-300q0 25-17.5 42.5T380-240Zm139-200v-71l-61 36-40-70 61-35-61-35 40-70 61 36v-71h80v71l61-36 40 70-61 35 61 35-40 70-61-36v71h-80Zm244 241-57-57q46-45 70-103.5T800-482q0-64-24.5-122.5T706-708l57-57q56 57 86.5 130T880-482q0 80-30 153t-87 130Z");
material!(/// `arrow_upward` — 이전 일치.
    mi_arrow_up, "M440-160v-487L216-423l-56-57 320-320 320 320-56 57-224-224v487h-80Z");
material!(/// `arrow_downward` — 다음 일치.
    mi_arrow_down, "M440-800v487L216-537l-56 57 320 320 320-320-56-57-224 224v-487h-80Z");
material!(/// `close` — 닫기.
    mi_close, "m256-200-56-56 224-224-224-224 56-56 224 224 224-224 56 56-224 224 224 224-56 56-224-224-224 224Z");
material!(/// `chevron_right` — 바꾸기 줄 접힘.
    mi_chevron_right, "M504-480 320-664l56-56 240 240-240 240-56-56 184-184Z");
material!(/// `expand_more` — 바꾸기 줄 펼침.
    mi_chevron_down, "M480-345 240-585l56-56 184 184 184-184 56 56-240 240Z");
codicon!(/// codicon `replace` — 바꾸기(사용자 제공 SVG 09-17).
    mi_find_replace, "M3.221 3.739l2.261 2.269L7.7 3.784l-.7-.7-1.012 1.007-.008-1.6a.523.523 0 0 1 .5-.526H8V1H6.48A1.482 1.482 0 0 0 5 2.489V4.1L3.927 3.033l-.706.706zm6.67 1.794h.01c.183.311.451.467.806.467.393 0 .706-.168.94-.503.236-.335.353-.78.353-1.333 0-.511-.1-.913-.301-1.207-.201-.295-.488-.442-.86-.442-.405 0-.718.194-.938.581h-.01V1H9v4.919h.89v-.386zm-.015-1.061v-.34c0-.248.058-.448.175-.601a.54.54 0 0 1 .445-.23.49.49 0 0 1 .436.233c.104.154.155.368.155.643 0 .33-.056.587-.169.768a.524.524 0 0 1-.47.27.495.495 0 0 1-.411-.211.853.853 0 0 1-.16-.532zM9 12.769c-.256.154-.625.231-1.108.231-.563 0-1.02-.178-1.369-.533-.349-.355-.523-.813-.523-1.374 0-.648.186-1.158.56-1.53.374-.376.875-.563 1.5-.563.433 0 .746.06.94.179v.998a1.26 1.26 0 0 0-.792-.276c-.325 0-.583.1-.774.298-.19.196-.283.468-.283.816 0 .338.09.603.272.797.182.191.431.287.749.287.282 0 .558-.092.828-.276v.946zM4 7L3 8v6l1 1h7l1-1V8l-1-1H4zm0 1h7v6H4V8z");
codicon!(/// codicon `replace-all` — 모두 바꾸기(사용자 제공 SVG 09-17).
    mi_replace_all, "M11.6 2.677c.147-.31.356-.465.626-.465.248 0 .44.118.573.353.134.236.201.557.201.966 0 .443-.078.798-.235 1.067-.156.268-.365.402-.627.402-.237 0-.416-.125-.537-.374h-.008v.31H11V1h.593v1.677h.008zm-.016 1.1a.78.78 0 0 0 .107.426c.071.113.163.169.274.169.136 0 .24-.072.314-.216.075-.145.113-.35.113-.615 0-.22-.035-.39-.104-.514-.067-.124-.164-.187-.29-.187-.12 0-.219.062-.297.185a.886.886 0 0 0-.117.48v.272zM4.12 7.695L2 5.568l.662-.662 1.006 1v-1.51A1.39 1.39 0 0 1 5.055 3H7.4v.905H5.055a.49.49 0 0 0-.468.493l.007 1.5.949-.944.656.656-2.08 2.085zM9.356 4.93H10V3.22C10 2.408 9.685 2 9.056 2c-.135 0-.285.024-.45.073a1.444 1.444 0 0 0-.388.167v.665c.237-.203.487-.304.75-.304.261 0 .392.156.392.469l-.6.103c-.506.086-.76.406-.76.961 0 .263.061.473.183.631A.61.61 0 0 0 8.69 5c.29 0 .509-.16.657-.48h.009v.41zm.004-1.355v.193a.75.75 0 0 1-.12.436.368.368 0 0 1-.313.17.276.276 0 0 1-.22-.095.38.38 0 0 1-.08-.248c0-.222.11-.351.332-.389l.4-.067zM7 12.93h-.644v-.41h-.009c-.148.32-.367.48-.657.48a.61.61 0 0 1-.507-.235c-.122-.158-.183-.368-.183-.63 0-.556.254-.876.76-.962l.6-.103c0-.313-.13-.47-.392-.47-.263 0-.513.102-.75.305v-.665c.095-.063.224-.119.388-.167.165-.049.315-.073.45-.073.63 0 .944.407.944 1.22v1.71zm-.64-1.162v-.193l-.4.068c-.222.037-.333.166-.333.388 0 .1.027.183.08.248a.276.276 0 0 0 .22.095.368.368 0 0 0 .312-.17c.08-.116.12-.26.12-.436zM9.262 13c.321 0 .568-.058.738-.173v-.71a.9.9 0 0 1-.552.207.619.619 0 0 1-.5-.215c-.12-.145-.181-.345-.181-.598 0-.26.063-.464.189-.612a.644.644 0 0 1 .516-.223c.194 0 .37.069.528.207v-.749c-.129-.09-.338-.134-.626-.134-.417 0-.751.14-1.001.422-.249.28-.373.662-.373 1.148 0 .42.116.764.349 1.03.232.267.537.4.913.4zM2 9l1-1h9l1 1v5l-1 1H3l-1-1V9zm1 0v5h9V9H3zm3-2l1-1h7l1 1v5l-1 1V7H6z");
svgicon!(/// codicon `preserve-case`(17×17 · 두 글자 경로만 · 배경 rect 제외) — 대소문자 보존(사용자 제공 SVG 09-17).
    mi_preserve_case, VB_CODICON17, false, "M9.02158 13.179H7.86061L6.91186 10.6698H3.11688L2.22431 13.179H1.0571L4.49006 4.22833H5.57613L9.02158 13.179ZM6.56857 9.72732L5.16417 5.91361C5.1184 5.78877 5.07263 5.58903 5.02685 5.3144H5.00189C4.96027 5.56823 4.91242 5.76796 4.85833 5.91361L3.46642 9.72732H6.56857ZM10.3448 13.179V4.22833H12.8915C13.6654 4.22833 14.2792 4.41767 14.7328 4.79633C15.1863 5.175 15.4131 5.6681 15.4131 6.27563C15.4131 6.78329 15.2758 7.22437 15.0012 7.59888C14.7265 7.97338 14.3479 8.2397 13.8652 8.39782V8.42279C14.4685 8.49353 14.9512 8.72239 15.3133 9.10938C15.6753 9.49221 15.8563 9.99155 15.8563 10.6074C15.8563 11.3731 15.5817 11.9931 15.0324 12.4674C14.4831 12.9418 13.7903 13.179 12.9539 13.179H10.3448ZM11.3934 5.17708V8.06701H12.467C13.0413 8.06701 13.4928 7.92969 13.8215 7.65506C14.1502 7.37626 14.3146 6.98511 14.3146 6.48161C14.3146 5.61192 13.7424 5.17708 12.5981 5.17708H11.3934ZM11.3934 9.00952V12.2303H12.8166C13.4324 12.2303 13.9089 12.0846 14.2459 11.7933C14.5871 11.5021 14.7577 11.1026 14.7577 10.5949C14.7577 9.53798 14.0379 9.00952 12.5981 9.00952H11.3934Z");
material!(/// `search` — 파일 검색 패널(활동 막대).
    mi_search, "M784-120 532-372q-30 24-69 38t-83 14q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 44-14 83t-38 69l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Z");
material!(/// `segment` — 선택 범위에서 찾기(≡).
    mi_in_selection, "M360-240v-80h480v80H360Zm0-200v-80h480v80H360ZM120-640v-80h720v80H120Z");

/// Material SVG → 툴바 아이콘(`&'static` 마스크 · 프로세스 수명 1회).
fn svg_tool(d: &str) -> ToolIcon {
    let alpha = memo((d.as_ptr() as usize, d.len(), 4), || {
        let polys = svg_polys(d);
        rasterize_dyn(&|x, y| in_polys_nonzero(x / M, y / M, &polys))
    });
    ToolIcon::Mask {
        w: SIDE,
        h: SIDE,
        alpha,
    }
}

/// Material `check` — Commit(툴바 · T-77).
pub(crate) fn commit() -> ToolIcon {
    svg_tool("M382-240 154-468l 57-57 171 171 367-367 57 57-424 424Z")
}
/// 트랜잭션 로그(툴바 · 09-17) — 문서 외곽 + 줄 3(코드 경로 · 상태색 틴트 · 배지는 툴바가 얹는다).
pub(crate) fn tx_log() -> ToolIcon {
    svg_tool("M200-120v-720h560v720H200Zm80-80h400v-560H280v560Zm60-90h280v-60H340v60Zm0-140h280v-60H340v60Zm0-140h280v-60H340v60Z")
}
/// Material `dns`(서버 스택) — 세션 목록(툴바 · 09-18): 서버 상자 둘 + 표시등.
pub(crate) fn sessions() -> ToolIcon {
    svg_tool("M200-120q-33 0-56.5-23.5T120-200v-160q0-33 23.5-56.5T200-440h560q33 0 56.5 23.5T840-360v160q0 33-23.5 56.5T760-120H200Zm0-80h560v-160H200v160Zm40-40q17 0 28.5-11.5T280-280q0-17-11.5-28.5T240-320q-17 0-28.5 11.5T200-280q0 17 11.5 28.5T240-240Zm-40-280q-33 0-56.5-23.5T120-600v-160q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v160q0 33-23.5 56.5T760-520H200Zm0-80h560v-160H200v160Zm40-40q17 0 28.5-11.5T280-680q0-17-11.5-28.5T240-720q-17 0-28.5 11.5T200-680q0 17 11.5 28.5T240-640Z")
}
/// Material `undo` — Rollback(툴바 · T-77).
pub(crate) fn rollback() -> ToolIcon {
    svg_tool("M280-200v-80h284q63 0 109.5-40T720-420q0-60-46.5-100T564-560H312l104 104-56 56-200-200 200-200 56 56-104 104h252q97 0 166.5 63T800-420q0 94-69.5 157T564-200H280Z")
}

#[cfg(test)]
mod svg_tests {
    use super::*;

    /// 경로 파서·비영 채움 — 화살표 마스크의 무게중심은 세로축 가운데·잉크가 있고 빈 배경은 0(사용자 09-16 Material SVG).
    #[test]
    fn material_paths_rasterize_with_ink_in_expected_places() {
        let up = mi_arrow_up();
        let ink: u32 = up.alpha.iter().map(|&a| a as u32).sum();
        assert!(ink > 0, "잉크가 있어야 한다");
        // 위 화살표: 위쪽 절반이 아래쪽 절반보다 잉크가 많다(머리 부분).
        let half = (SIDE * SIDE / 2) as usize;
        let top: u32 = up.alpha[..half].iter().map(|&a| a as u32).sum();
        let bottom: u32 = up.alpha[half..].iter().map(|&a| a as u32).sum();
        assert!(top > bottom, "위 화살표 머리는 위쪽에: {top} vs {bottom}");
        // 구멍이 있는 글자(match_case의 a 안)도 비영 규칙으로 뚫린다: 전체가 채워지지 않는다.
        let mc = mi_match_case();
        let filled = mc.alpha.iter().filter(|&&a| a == 255).count();
        assert!(filled < (SIDE * SIDE / 2) as usize);
        // 모서리(여백)는 비어 있다.
        assert_eq!(mc.alpha[0], 0);
        assert_eq!(mi_close().alpha[0], 0);
        // 파서: 상대 좌표·암묵 반복·소수.
        let polys = svg_polys("m10-20 5 5 5-5Z");
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0].len(), 3);
        assert!((polys[0][0].1 - 940.0).abs() < 0.01, "y + 960 보정");
    }
}
