//! ★ T-110 3-4 미니맵 섹션 헤더(10-31 · docs/46 3순위 · VS Code 1.85 `minimap.showRegionSectionHeaders`/`MARK:`): 주석 **구분선**의 라벨을
//! 미니맵 띠에 작은 글자로 — 긴 스크립트에서 "지금 어디쯤"을 띠만 보고 안다. 순수 함수 + 시험 · 호스트는 활성 탭의 본문 세대가 바뀔 때만
//! 다시 훑는다(`Editors::minimap_sections_tick` · 설정 `editor.minimap_sections` · 기본 끔).
//!
//! 구분선 = 한 줄 주석(`--` · `/* … */` · `#`)으로 ① 구분 글자(`=` `-` `#` `*` `~` `_`)가 **3개 이상** 이어지는 줄 — 라벨 = 구분 글자를 걷어 낸 남은 글 ·
//! 글이 없으면 **다음 줄**이 글 있는 주석일 때 그 글 ② `#region 이름` · `region 이름` · `MARK: 이름` · `SECTION: 이름`(VS Code·Xcode 관례) —
//! 라벨 = 이름. 라벨은 [`MAX_LABEL`] 글자에서 자른다(`…`) · `#endregion`은 헤더가 아니다.

/// 라벨 최대 글자 수(띠 폭이 좁다 · 넘으면 `…`).
pub(crate) const MAX_LABEL: usize = 24;

const RULE_CHARS: &[char] = &['=', '-', '#', '*', '~', '_'];

/// 한 줄 주석의 몸통(주석 표식·닫는 `*/`를 걷은 글) — 주석이 아니면 None.
fn comment_body(line: &str) -> Option<&str> {
    let t = line.trim();
    let body = if let Some(b) = t.strip_prefix("--") {
        b
    } else if let Some(b) = t.strip_prefix("/*") {
        // 한 줄 블록 주석만(`*/`로 끝나는) — 여러 줄 블록의 첫 줄은 제외(본문 안 글이 섞인다).
        b.strip_suffix("*/")?
    } else {
        t.strip_prefix('#')?
    };
    Some(body.trim())
}

/// 구분 글자를 앞뒤에서 걷어 낸 라벨 글(빈 글 = 구분선만).
fn strip_rule(body: &str) -> &str {
    body.trim_matches(|c: char| RULE_CHARS.contains(&c) || c.is_whitespace())
}

/// 몸통이 구분선인가(구분 글자 3개 이상 연속).
fn has_rule(body: &str) -> bool {
    let mut run = 0usize;
    let mut last: Option<char> = None;
    for c in body.chars() {
        if RULE_CHARS.contains(&c) && last == Some(c) {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else if RULE_CHARS.contains(&c) {
            run = 1;
        } else {
            run = 0;
        }
        last = Some(c);
    }
    false
}

/// `#region 이름` · `region 이름` · `MARK: 이름` · `SECTION: 이름` → 이름(없으면 빈 글 · 표식이 아니면 None).
fn marker_label(body: &str) -> Option<&str> {
    let b = body.trim_start_matches(['#', ' ']);
    let lower = b.to_ascii_lowercase();
    for (kw, len) in [("region", 6), ("mark:", 5), ("section:", 8)] {
        if lower.starts_with(kw) {
            if lower.starts_with("regionend") || lower.starts_with("endregion") {
                return None;
            }
            let rest = b[len..].trim();
            // `region` 뒤에 글자가 바로 붙으면(`regional`) 표식이 아니다.
            if kw == "region" && !rest.is_empty() && !b[len..].starts_with(char::is_whitespace) {
                return None;
            }
            return Some(rest);
        }
    }
    None
}

fn clip(label: &str) -> String {
    let mut s: String = label.chars().take(MAX_LABEL).collect();
    if label.chars().count() > MAX_LABEL {
        s.push('…');
    }
    s
}

/// 본문 줄들 → (논리 줄 번호 0 기준, 라벨). 구분선만 있는 줄 + 다음 줄 주석 글 = 그 글(헤더 줄은 구분선) · 라벨 없는 구분선은 뺀다.
pub(crate) fn section_headers<'a>(
    lines: impl IntoIterator<Item = &'a str>,
) -> Vec<(usize, String)> {
    let lines: Vec<&str> = lines.into_iter().collect();
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let Some(body) = comment_body(lines[i]) else {
            i += 1;
            continue;
        };
        if let Some(name) = marker_label(body) {
            if !name.is_empty() {
                out.push((i, clip(name)));
            }
            i += 1;
            continue;
        }
        if has_rule(body) {
            let label = strip_rule(body);
            if !label.is_empty() {
                out.push((i, clip(label)));
            } else if let Some(next) = lines.get(i + 1).and_then(|l| comment_body(l)) {
                // `-- =====` 다음 줄 `-- 매출 집계` 꼴 — 라벨은 다음 줄 · 그 줄은 소비(두 번 안 잡음).
                let l2 = strip_rule(next);
                if !l2.is_empty() && !has_rule(next) && marker_label(next).is_none() {
                    out.push((i, clip(l2)));
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 구분선(글 있음/없음 + 다음 줄) · `#region`/`MARK:` · `#endregion` 제외 · 블록 주석 한 줄 · 본문 줄 무시 · 긴 라벨 자름 · 2연속 구분 글자는 아님.
    #[test]
    fn headers_from_rules_and_markers() {
        let src = "select 1;\n-- ===== 매출 집계 =====\nselect 2;\n-- ----------\n-- 재고 조정\nselect 3;\n/* *** 마감 *** */\n-- #region 정리\nselect 4;\n-- #endregion\n-- MARK: 끝\n-- -- 두 개는 아님\n--  ######\nselect 5;\n";
        let got = section_headers(src.lines());
        assert_eq!(
            got,
            vec![
                (1, "매출 집계".to_string()),
                (3, "재고 조정".to_string()),
                (6, "마감".to_string()),
                (7, "정리".to_string()),
                (10, "끝".to_string()),
            ],
            "{got:?}"
        );
        let long = format!("-- === {} ===", "가".repeat(30));
        let got = section_headers([long.as_str()]);
        assert_eq!(got[0].1.chars().count(), MAX_LABEL + 1);
        assert!(got[0].1.ends_with('…'));
        assert!(
            section_headers(["-- regional sales"]).is_empty(),
            "region 뒤에 글자가 붙으면 표식 아님"
        );
        assert!(
            section_headers(["/* ====", " 여러 줄 블록 */"]).is_empty(),
            "여러 줄 블록 주석의 첫 줄은 제외"
        );
    }
}
