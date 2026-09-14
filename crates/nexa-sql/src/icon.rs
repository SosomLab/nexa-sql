//! 앱 아이콘(사용자 선택 09-14 · "Union" — 색이 다른 타원 세 장이 한 실린더로 융합 · `>_` 배지).
//!
//! ★ **코드로 그린다** — 정적 자원 0바이트(사용자 09-14 "정적 자원이 실행 파일 용량·메모리를 늘리지 않게").
//! `packaging/branding/icon.svg`(SSOT)의 도형을 같은 좌표계(256)로 옮겨 4×4 슈퍼샘플링으로 래스터한다 —
//! 어떤 크기든 벡터 품질 · 만든 픽셀은 `Icon`으로 넘긴 뒤 바로 해제(상주 메모리 0 · OS가 HICON 보관).
//! 배포용 PNG/ICO/ICNS는 `packaging/branding/`(설치기·번들 몫 · 실행 파일에 넣지 않는다).
//!
//! | OS | 창 아이콘의 효과 |
//! |---|---|
//! | Windows | 타이틀바(小 32) + 작업표시줄(大 64 — `with_taskbar_icon`) |
//! | Linux/X11 | 태스크바·창 전환기 · Wayland는 `.desktop` + hicolor PNG 몫 |
//! | macOS | 무시 — Dock 아이콘은 `.app` 번들의 `.icns` 몫 |

const BG: [u8; 3] = [0xF6, 0xF7, 0xF9];
const BORDER: [u8; 3] = [0xD5, 0xDA, 0xE1];
const GREEN: [u8; 3] = [0x2E, 0xA0, 0x43];
const BLUE: [u8; 3] = [0x3D, 0x8B, 0xFF];
const RED: [u8; 3] = [0xE5, 0x53, 0x4B];
const CAP: [u8; 3] = [0xFF, 0x8A, 0x80];
const INK: [u8; 3] = [0x1B, 0x24, 0x32];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];

/// 타원 밴드(위 y · 아래 y) — 아래에서 위 순서로 칠한다(위 밴드가 아래 밴드의 오목한 윗면을 덮는다).
const BANDS: [(f32, f32, [u8; 3]); 3] = [
    (158.0, 196.0, GREEN),
    (114.0, 152.0, BLUE),
    (70.0, 108.0, RED),
];
const RX: f32 = 84.0;
const RY: f32 = 27.0;
const CX: f32 = 128.0;
/// 배지 그룹 이동(사용자: 왼쪽 위로 6px).
const BADGE_DX: f32 = -6.0;

fn in_rounded_rect(x: f32, y: f32, x0: f32, y0: f32, w: f32, h: f32, r: f32) -> bool {
    if x < x0 || y < y0 || x > x0 + w || y > y0 + h {
        return false;
    }
    let cx = if x < x0 + r {
        x0 + r
    } else if x > x0 + w - r {
        x0 + w - r
    } else {
        x
    };
    let cy = if y < y0 + r {
        y0 + r
    } else if y > y0 + h - r {
        y0 + h - r
    } else {
        y
    };
    (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r
}

fn in_ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let dx = (x - cx) / rx;
    let dy = (y - cy) / ry;
    dx * dx + dy * dy <= 1.0
}

/// 점–선분 거리(둥근 끝 획).
fn seg_dist(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let (wx, wy) = (x - ax, y - ay);
    let t = ((wx * vx + wy * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (px, py) = (ax + t * vx, ay + t * vy);
    ((x - px) * (x - px) + (y - py) * (y - py)).sqrt()
}

/// 256 좌표계의 한 점 색(RGB · 불투명 여부). 그리기 순서 = SVG와 동일.
fn sample(x: f32, y: f32) -> Option<[u8; 3]> {
    if !in_rounded_rect(x, y, 8.0, 8.0, 240.0, 240.0, 56.0) {
        return None;
    }
    let mut c = if in_rounded_rect(x, y, 12.0, 12.0, 232.0, 232.0, 52.0) {
        BG
    } else {
        BORDER
    };
    for (yt, yb, col) in BANDS {
        let in_rect = (CX - RX..=CX + RX).contains(&x) && (yt..=yb).contains(&y);
        let in_bottom = y >= yb && in_ellipse(x, y, CX, yb, RX, RY);
        let in_top = in_ellipse(x, y, CX, yt, RX, RY);
        if (in_rect || in_bottom) && !in_top {
            c = col;
        }
    }
    if in_ellipse(x, y, CX, 70.0, RX, RY) {
        c = CAP;
        if in_ellipse(x, y, CX, 70.0, 50.0, 13.0) {
            // 흰색 55% 하이라이트
            c = [
                (f32::from(c[0]) * 0.45 + 255.0 * 0.55) as u8,
                (f32::from(c[1]) * 0.45 + 255.0 * 0.55) as u8,
                (f32::from(c[2]) * 0.45 + 255.0 * 0.55) as u8,
            ];
        }
    }
    // 배지
    let (bx, by) = (x - BADGE_DX, y - BADGE_DX);
    if in_rounded_rect(bx, by, 152.0, 152.0, 86.0, 86.0, 22.0) {
        c = INK;
        let chevron = seg_dist(bx, by, 174.0, 180.0, 192.0, 195.0) <= 5.5
            || seg_dist(bx, by, 192.0, 195.0, 174.0, 210.0) <= 5.5;
        if chevron || in_rounded_rect(bx, by, 200.0, 205.0, 20.0, 9.0, 4.0) {
            c = WHITE;
        }
    }
    let _ = WHITE;
    Some(c)
}

/// `side`×`side` RGBA(직선 알파) — 4×4 슈퍼샘플링. 64px = 64KB 미만의 일시 버퍼(Icon 생성 후 해제).
pub(crate) fn icon_rgba(side: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((side * side * 4) as usize);
    let unit = 256.0 / side as f32;
    for py in 0..side {
        for px in 0..side {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) * unit;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) * unit;
                    if let Some(c) = sample(x, y) {
                        r += u32::from(c[0]);
                        g += u32::from(c[1]);
                        b += u32::from(c[2]);
                        a += 1;
                    }
                }
            }
            // 덮인 샘플만 평균(직선 알파) · 알파 = 덮임 비율 · 하나도 안 덮이면 투명
            match (r.checked_div(a), g.checked_div(a), b.checked_div(a)) {
                (Some(r), Some(g), Some(b)) => {
                    out.extend_from_slice(&[
                        r as u8,
                        g as u8,
                        b as u8,
                        (a * 255 / (SS * SS)) as u8,
                    ]);
                }
                _ => out.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
    out
}

/// 창 속성에 아이콘을 붙인다 — 모든 창(메인·접속·로그)이 이 한 곳을 지난다. 변환 실패 = 아이콘 없이(fail-soft).
pub(crate) fn with_icon(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    let small = winit::window::Icon::from_rgba(icon_rgba(32), 32, 32).ok();
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows as _;
        let large = winit::window::Icon::from_rgba(icon_rgba(64), 64, 64).ok();
        attrs.with_taskbar_icon(large)
    };
    attrs.with_window_icon(small)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_transparent_center_red_badge_ink() {
        let s = 64;
        let px = icon_rgba(s);
        let at = |x: u32, y: u32| {
            let i = ((y * s + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        assert_eq!(at(0, 0)[3], 0, "모서리 밖 = 투명");
        assert_eq!(at(32, 28)[..3], RED, "위 밴드 = 빨강");
        assert_eq!(at(32, 48)[..3], GREEN, "아래 밴드 = 초록");
        assert_eq!(at(56, 50)[..3], INK, "배지 = 잉크");
        assert_eq!(at(18, 17)[..3], CAP, "윗면 = 캡");
        assert_eq!(px.len(), (s * s * 4) as usize);
    }

    /// `NSQL_ICON_DUMP=<경로.ppm>`이면 256px를 PPM으로 떨어뜨린다(육안 검수용 · 기본 무시).
    #[test]
    fn dump_ppm_when_asked() {
        let Ok(path) = std::env::var("NSQL_ICON_DUMP") else {
            return;
        };
        let s = 256u32;
        let px = icon_rgba(s);
        let mut out = format!(
            "P6
{s} {s}
255
"
        )
        .into_bytes();
        for p in px.chunks(4) {
            // 흰 바탕에 합성
            let a = u32::from(p[3]);
            for c in &p[..3] {
                out.push(((u32::from(*c) * a + 255 * (255 - a)) / 255) as u8);
            }
        }
        std::fs::write(path, out).expect("write ppm");
    }
}
