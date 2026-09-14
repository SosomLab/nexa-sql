//! 툴바 아이콘 — **코드로 그린 알파 마스크**(사용자 09-14 "글꼴이 아니라 이미지로"). 정적 자원 0바이트.
//!
//! 256 좌표계의 도형을 4×4 슈퍼샘플링으로 `SIDE`×`SIDE` 1채널 커버리지로 래스터해 [`ToolIcon::Mask`]로 넘긴다 —
//! 색은 툴바가 테마 기준색(hover/pressed = accent)으로 틴트하므로 여기선 **모양만** 정의한다.
//! 마스크는 프로세스 수명 동안 한 번만 만들어 `Box::leak`(5개 × 4KB · 툴바가 `&'static`을 요구).

use nexa_ctl::ToolIcon;

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
fn shape_new(x: f32, y: f32) -> bool {
    stroke(x, y, (128.0, 60.0), (128.0, 196.0), 26.0) || stroke(x, y, (60.0, 128.0), (196.0, 128.0), 26.0)
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

/// 플러그 — 접속 창. 왼쪽 케이블 · 둥근 몸통 · 오른쪽 핀 2개 · 핀이 꽂히는 소켓 테두리.
fn shape_connect(x: f32, y: f32) -> bool {
    // 케이블(둥근 끝 획) → 몸통
    let cable = stroke(x, y, (16.0, 128.0), (58.0, 128.0), 22.0);
    let body = in_rounded_rect(x, y, 52.0, 84.0, 80.0, 88.0, 20.0);
    // 핀 2개
    let pin1 = in_rounded_rect(x, y, 128.0, 100.0, 46.0, 16.0, 7.0);
    let pin2 = in_rounded_rect(x, y, 128.0, 140.0, 46.0, 16.0, 7.0);
    // 소켓 — 오른쪽에 열린 ㄷ자 테두리(획 14)
    let sock_outer = in_rounded_rect(x, y, 186.0, 70.0, 56.0, 116.0, 18.0);
    let sock_inner = in_rounded_rect(x, y, 200.0, 84.0, 60.0, 88.0, 10.0);
    let socket = sock_outer && !sock_inner;
    cable || body || pin1 || pin2 || socket
}

/// ≡ — 로그 창.
fn shape_log(x: f32, y: f32) -> bool {
    [72.0, 128.0, 184.0]
        .iter()
        .any(|&cy| stroke(x, y, (52.0, cy), (204.0, cy), 22.0))
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
        ] {
            let (opaque, partial) = coverage(s);
            assert!(opaque > 100, "채워진 픽셀이 있어야 한다: {opaque}");
            assert!(partial > 20, "가장자리는 부분 커버리지(AA): {partial}");
            assert!(opaque < (SIDE * SIDE) as usize / 2, "슬롯을 다 덮지 않는다");
        }
    }

    #[test]
    fn connect_has_gap_between_pins_and_socket_is_open() {
        // 핀 사이(y=128)는 비어 있고, 소켓 안쪽(x=230, y=128)도 비어 있다(꽂히는 자리).
        assert!(!shape_connect(150.0, 128.0));
        assert!(!shape_connect(230.0, 128.0));
        assert!(shape_connect(150.0, 108.0), "위 핀");
        assert!(shape_connect(90.0, 128.0), "몸통");
        assert!(shape_connect(192.0, 128.0), "소켓 왼쪽 벽");
    }
}
