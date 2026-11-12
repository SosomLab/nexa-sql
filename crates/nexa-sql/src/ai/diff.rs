//! ★ AI ↔ 편집기 연동의 순수 계산(docs/110 §18 · 사용자 11-12): 변경 미리보기(승인 카드에 넣는 전후 줄 글) · 답 글의 줄 번호 찾기.

use nexa_ctl::diff::{align, hunks, RowKind};

/// 변경 미리보기: 지금 글 `cur` → 새 글 `new`의 바뀐 덩어리를 `-`(지워짐) / `+`(새 줄) / ` `(문맥 `ctx`줄)로. 줄 머리는
/// `@@ <지금 줄 번호> @@`. `max_lines`를 넘으면 잘라 "… (N lines more)". 바뀐 것이 없으면 빈 글.
pub(crate) fn preview(cur: &str, new: &str, ctx: usize, max_lines: usize) -> String {
    let a: Vec<String> = cur.split('\n').map(str::to_string).collect();
    let b: Vec<String> = new.split('\n').map(str::to_string).collect();
    let rows = align(&a, &b);
    let mut out: Vec<String> = Vec::new();
    let mut last_end = 0usize;
    for h in hunks(&rows) {
        let from = h.rows.start.saturating_sub(ctx).max(last_end);
        let to = (h.rows.end + ctx).min(rows.len());
        if from >= to {
            continue;
        }
        // 덩어리가 이어지면 머리 줄을 다시 쓰지 않는다.
        if from > last_end || out.is_empty() {
            let line_no = rows[from..to]
                .iter()
                .find_map(|r| r.a)
                .map_or(a.len(), |x| x + 1);
            out.push(format!("@@ {line_no} @@"));
        }
        for r in &rows[from..to] {
            match r.kind {
                RowKind::Equal => {
                    if let Some(x) = r.a {
                        out.push(format!("  {}", a[x]));
                    }
                }
                RowKind::Delete => {
                    if let Some(x) = r.a {
                        out.push(format!("- {}", a[x]));
                    }
                }
                RowKind::Insert => {
                    if let Some(y) = r.b {
                        out.push(format!("+ {}", b[y]));
                    }
                }
                RowKind::Replace => {
                    if let Some(x) = r.a {
                        out.push(format!("- {}", a[x]));
                    }
                    if let Some(y) = r.b {
                        out.push(format!("+ {}", b[y]));
                    }
                }
            }
        }
        last_end = to;
    }
    if out.len() > max_lines {
        let more = out.len() - max_lines;
        out.truncate(max_lines);
        out.push(format!("… ({more} lines more)"));
    }
    out.join("\n")
}

/// 글 안의 줄 번호 언급(바이트 범위 · 줄 번호): `263행` · `263번째 줄` · `263번 줄` · `줄 263` · `line 263` · `Line 263`.
/// 1 ~ 9,999,999만(연도·건수 같은 다른 숫자는 낱말이 붙지 않아 걸리지 않는다).
pub(crate) fn line_refs(text: &str) -> Vec<(usize, usize, usize)> {
    static RX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let rx = RX.get_or_init(|| {
        regex::Regex::new(r"(?i)(\d{1,7})\s?(?:행|번째 줄|번 줄)|(?:줄|line)\s(\d{1,7})")
            .expect("rx")
    });
    rx.captures_iter(text)
        .filter_map(|c| {
            let m = c.get(0)?;
            let n: usize = c.get(1).or_else(|| c.get(2))?.as_str().parse().ok()?;
            (n >= 1).then_some((m.start(), m.end(), n))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn preview_shows_changed_lines_with_context() {
        let cur = "a\nb\nc\nd\ne\nf";
        let new = "a\nb\nX\nd\ne\nf";
        let p = preview(cur, new, 1, 50);
        assert_eq!(p, "@@ 2 @@\n  b\n- c\n+ X\n  d");
        assert_eq!(preview(cur, cur, 3, 50), "");
        // 줄 추가 · 삭제.
        let p = preview("a\nb", "a\nn\nb", 0, 50);
        assert!(p.contains("+ n") && !p.contains("- "), "{p}");
        let p = preview("a\nb\nc", "a\nc", 0, 50);
        assert!(p.contains("- b") && !p.contains("+ "), "{p}");
        // 길이 제한.
        let long_new: String = (0..40).map(|i| format!("n{i}\n")).collect();
        let p = preview("x", &long_new, 0, 5);
        assert!(p.lines().count() == 6 && p.ends_with("lines more)"), "{p}");
    }

    #[test]
    fn line_refs_find_korean_and_english_mentions() {
        let r = line_refs("오류는 263행과 line 12, 그리고 5번째 줄 · 줄 77 입니다. 2026년 200건");
        let nums: Vec<usize> = r.iter().map(|x| x.2).collect();
        assert_eq!(nums, vec![263, 12, 5, 77]);
        let t = "see Line 9";
        let r = line_refs(t);
        assert_eq!(&t[r[0].0..r[0].1], "Line 9");
        assert!(line_refs("no numbers here").is_empty());
    }
}
