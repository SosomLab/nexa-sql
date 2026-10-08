//! 편집기 | 결과 상하 분할의 **최소 높이**(사용자 10-09 "편집기 탭 영역 · 결과 탭 영역 각각 최소 크기 설정") — 순수 판정 + MC/DC.
//! 분할 비율(`layout.editor_split_pct`)은 그대로 두고, 배치와 스플리터 드래그가 이 함수로 편집기 높이를 **양쪽 최소 안으로** 잡는다.

/// 편집기 칸 높이(`editor_h` = 편집기 본체 + 아래 여백 `pad`)를 최소 높이 둘 안으로.
/// - 편집기 본체 ≥ `min_editor` → `editor_h ≥ min_editor + pad`
/// - 결과 영역 ≥ `min_result` → `editor_h ≤ body_h - pad - min_result`
/// - 둘을 다 못 지키는 작은 창 = 창 높이에 비례해 나눈다(최소의 비로 · 어느 한쪽이 0이 되지 않게).
pub(crate) fn clamp_editor_h(
    editor_h: i32,
    body_h: i32,
    pad: i32,
    min_editor: i32,
    min_result: i32,
) -> i32 {
    let lo = min_editor.max(0) + pad;
    let hi = body_h - pad - min_result.max(0);
    if lo <= hi {
        editor_h.clamp(lo, hi)
    } else {
        // 작은 창: 두 최소의 비로 나눈다(둘 다 0이면 반반).
        let total = (min_editor.max(0) + min_result.max(0)).max(1);
        let share = if min_editor + min_result <= 0 {
            0.5
        } else {
            min_editor.max(0) as f32 / total as f32
        };
        ((body_h - pad) as f32 * share).round() as i32 + pad
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_editor_h as f;

    /// MC/DC: 아래 한계 · 위 한계 · 안 · 작은 창(비례) · 0 최소.
    #[test]
    fn rules() {
        // body 1000 · pad 8 · 편집기 최소 120 · 결과 최소 120 → editor_h ∈ [128, 872].
        assert_eq!(f(50, 1000, 8, 120, 120), 128, "아래 한계");
        assert_eq!(f(950, 1000, 8, 120, 120), 872, "위 한계");
        assert_eq!(f(500, 1000, 8, 120, 120), 500, "안 = 그대로");
        // 최소 0 = 종전과 같음(아래 한계 = pad · 위 한계 = body - pad).
        assert_eq!(f(0, 1000, 8, 0, 0), 8);
        assert_eq!(f(5000, 1000, 8, 0, 0), 992);
        // 작은 창(300) · 최소 200+200 = 못 지킴 → 비례(반반) = (300-8)/2 + 8 = 154.
        assert_eq!(f(10, 300, 8, 200, 200), 154);
        // 비 1:3.
        assert_eq!(
            f(10, 300, 8, 100, 300),
            ((292.0_f32 * 0.25).round() as i32) + 8
        );
    }
}
