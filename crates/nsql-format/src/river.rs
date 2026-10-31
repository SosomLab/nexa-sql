//! ★ T-255 강(river) 정렬(10-30 사용자 결정 "Basic 옵션으로" · 10-31 구현 · docs/95 §10): `format.indent_style = river`일 때 레이아웃 IR을
//! **후처리**한다 — Walker(layout.rs)는 손대지 않는다.
//!
//! 규칙(95 §10-1): 질의 블록(같은 깊이의 절 키워드 묶음)마다 **강 열** = 절·JOIN 키워드 폭의 최댓값 · 절 키워드는 강 열에서 끝나게
//! 오른쪽 맞춤 · 첫 항목은 절 줄에 잇는다 · 이어지는 항목 = 강 열 + 1(줄 앞 콤마는 강 열에) · `ON`·`AND`/`OR`도 강에 오른쪽 맞춤 ·
//! 서브쿼리(더 깊은 절)는 자기 깊이의 강 · INSERT · MERGE 문장과 통과(Raw) 줄은 그대로(standard). 강 줄의 들여쓰기는 **공백**으로만
//! (열 정렬이 목적 · `Line::col`).

use crate::layout::{display_width, Line, Part, Role};
use crate::{is_keyword, Comma, Indent, Options};

/// 한 단계 들여쓰기의 표시 폭(강 열 계산의 기준 열 = 깊이 × 폭).
fn unit_width(opts: &Options) -> usize {
    match opts.indent {
        Indent::Tab => opts.tab_width.max(1),
        Indent::Spaces(n) => (n as usize).max(1),
    }
}

/// 줄 머리의 예약어 연속(`LEFT OUTER JOIN` · `GROUP BY` · `ON` · `AND`) — 첫 조각(Text)의 앞 낱말들 중 예약어인 것: (낱말 수, 표시 폭,
/// 첫 조각 전체가 예약어뿐인가). 예약어가 없으면 (0, 0, false). Walker는 `JOIN dept d` · `ON d.id`처럼 키워드와 뒤 글을 한 조각에 담는다.
fn keyword_head(parts: &[Part], tab_width: usize) -> (usize, usize, bool) {
    let Some(Part::Text(first)) = parts.first() else {
        return (0, 0, false);
    };
    let words: Vec<&str> = first.split_whitespace().collect();
    let n = words.iter().take_while(|w| is_keyword(w)).count();
    if n == 0 {
        return (0, 0, false);
    }
    let head = words[..n].join(" ");
    (n, display_width(&head, tab_width), n == words.len())
}

fn first_word(line: &Line) -> String {
    match line.parts.first() {
        Some(Part::Text(t)) => t
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase(),
        _ => String::new(),
    }
}

/// 이 줄이 속한 **강의 깊이**: 절·JOIN = 자기 깊이 · `ON`/`USING` 절 · 조건(AND/OR) · 항목 = 한 단계 위(0이면 0).
fn river_level(line: &Line) -> usize {
    match line.role {
        Role::Clause => {
            let w = first_word(line);
            if w == "ON" || w == "USING" {
                line.indent.saturating_sub(1)
            } else {
                line.indent
            }
        }
        Role::Join => line.indent,
        _ => line.indent.saturating_sub(1),
    }
}

/// 문장(세그먼트) 경계 = 깊이 0의 절·JOIN 줄 또는 Raw 줄. `None` = 강 적용 대상 아님(INSERT · MERGE · Raw).
fn segment_kind(line: &Line) -> Option<bool> {
    if line.role == Role::Raw {
        return Some(false);
    }
    if line.indent == 0 && matches!(line.role, Role::Clause | Role::SetOp) {
        let w = first_word(line);
        return match w.as_str() {
            // 문장 시작(강 대상) · 집합 연산자 뒤 블록도 새 강.
            "SELECT" | "WITH" | "UPDATE" | "DELETE" => Some(true),
            "INSERT" | "MERGE" => Some(false),
            _ if line.role == Role::SetOp => Some(true),
            _ => None,
        };
    }
    None
}

/// 강 정렬 적용(IR 후처리 · 멱등 = 같은 토큰열이면 같은 결과).
pub fn apply(lines: &mut Vec<Line>, opts: &Options) {
    let tw = opts.tab_width.max(1);
    let unit = unit_width(opts);
    // 1) 세그먼트 나누기: (시작, 끝, 적용 여부).
    let mut segs: Vec<(usize, usize, bool)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        // 머리 주석 등(경계 아님)은 다음 경계까지 한 세그먼트로(강 적용).
        let on = segment_kind(&lines[i]).unwrap_or(true);
        let mut j = i + 1;
        while j < lines.len() && segment_kind(&lines[j]).is_none() {
            j += 1;
        }
        // 집합 연산자(UNION …) 줄은 앞 블록에 붙여 두고 다음 SELECT부터 새 강.
        segs.push((i, j, on && lines[i].role != Role::Raw));
        i = j;
    }
    let mut out: Vec<Line> = Vec::with_capacity(lines.len());
    for (s, e, on) in segs {
        if !on {
            out.extend(lines[s..e].iter().cloned());
            continue;
        }
        let seg: Vec<Line> = lines[s..e].to_vec();
        // 2) 깊이별 강 폭 = 절·JOIN·ON·조건 줄 머리 예약어 폭의 최댓값.
        let mut width: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
        for l in &seg {
            if matches!(l.role, Role::Clause | Role::Join | Role::Cond) {
                let (n, w, _) = keyword_head(&l.parts, tw);
                if n > 0 {
                    let lv = river_level(l);
                    let cur = width.entry(lv).or_insert(0);
                    *cur = (*cur).max(w);
                }
            }
        }
        // 3) 줄마다 열 계산 · 절 줄에 첫 항목 잇기.
        let mut k = 0;
        while k < seg.len() {
            let mut l = seg[k].clone();
            let lv = river_level(&l);
            let base = lv * unit;
            let w = width.get(&lv).copied().unwrap_or(0);
            match l.role {
                Role::Clause | Role::Join | Role::Cond => {
                    let (n, kw, all_kw) = keyword_head(&l.parts, tw);
                    if n == 0 {
                        l.col = Some(base);
                    } else {
                        l.col = Some(base + w.saturating_sub(kw));
                        // 절 키워드만 있는 줄 + 다음 줄이 그 절의 첫 항목/첫 조건(콤마·AND/OR·주석 아님) → 같은 줄로.
                        let only_kw = all_kw && l.parts.len() == 1 && l.role == Role::Clause;
                        if only_kw {
                            if let Some(next) = seg.get(k + 1) {
                                let next_lv = river_level(next);
                                let joinable = matches!(next.role, Role::Item | Role::Cond)
                                    && next_lv == lv
                                    && next.indent == l.indent + 1
                                    && !matches!(next.parts.first(), Some(Part::Comma))
                                    && !matches!(first_word(next).as_str(), "AND" | "OR")
                                    && next.blank_before == 0;
                                if joinable {
                                    l.parts.extend(next.parts.iter().cloned());
                                    k += 1;
                                }
                            }
                        }
                    }
                }
                Role::Item => {
                    // 줄 앞 콤마 모드에서만 콤마가 강 열에 — 줄 끝 콤마 모드는 렌더가 콤마를 앞 줄로 옮기므로 항목 자리.
                    if opts.comma == Comma::Leading && matches!(l.parts.first(), Some(Part::Comma))
                    {
                        l.col = Some((base + w).saturating_sub(1));
                    } else {
                        l.col = Some(base + w + 1);
                    }
                }
                Role::Raw => {}
                // 닫는 괄호 · CASE · 주석 · 집합 연산자 · 그 밖 = 자기 깊이 열(공백).
                _ => {
                    l.col = Some(l.indent * unit);
                }
            }
            out.push(l);
            k += 1;
        }
    }
    *lines = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format_basic, IndentStyle};

    fn river_opts(comma: Comma) -> Options {
        Options {
            indent: Indent::Spaces(4),
            tab_width: 4,
            comma,
            indent_style: IndentStyle::River,
            final_newline: true,
            ..Options::default()
        }
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace()
            .flat_map(|w| w.split(',').map(str::to_string))
            .filter(|w| !w.is_empty())
            .collect()
    }

    /// 95 §10-1 모양: 절 키워드 오른쪽 맞춤 · 첫 항목 같은 줄 · 줄 앞 콤마는 강 열 · JOIN·ON·AND 강 맞춤 · 멱등 · 토큰 보존.
    #[test]
    fn river_aligns_clauses_and_keeps_tokens() {
        let o = river_opts(Comma::Leading);
        let src = "select a.id, a.name from emp a join dept d on d.id = a.dept_id where a.sal > 100 and d.loc = 'SEOUL' group by a.dept_id order by a.name";
        let out = format_basic(src, &o);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "  SELECT a.id", "{out}");
        assert_eq!(lines[1], "       , a.name", "{out}");
        assert_eq!(lines[2], "    FROM emp a", "{out}");
        assert_eq!(lines[3], "    JOIN dept d", "{out}");
        assert_eq!(lines[4], "      ON d.id = a.dept_id", "{out}");
        assert_eq!(lines[5], "   WHERE a.sal > 100", "{out}");
        assert_eq!(lines[6], "     AND d.loc = 'SEOUL'", "{out}");
        assert_eq!(lines[7], "GROUP BY a.dept_id", "{out}");
        assert_eq!(lines[8], "ORDER BY a.name", "{out}");
        // 토큰 보존 · 멱등.
        assert_eq!(
            words(src)
                .iter()
                .map(|w| w.to_ascii_uppercase())
                .collect::<Vec<_>>(),
            words(&out)
                .iter()
                .map(|w| w.to_ascii_uppercase())
                .collect::<Vec<_>>()
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
    }

    /// 줄 끝 콤마 · 넓은 JOIN(강이 넓어짐) · INSERT는 standard 그대로 · 탭 단위여도 강 줄은 공백.
    #[test]
    fn river_trailing_comma_wide_join_insert_untouched_and_spaces_only() {
        let mut o = river_opts(Comma::Trailing);
        o.indent = Indent::Tab;
        let src = "select a.x, b.y from t a left outer join u b on a.id = b.id where a.x = 1; insert into t (a) values (1);";
        let out = format_basic(src, &o);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "         SELECT a.x,", "{out}");
        assert_eq!(lines[1], "                b.y", "{out}");
        assert_eq!(lines[2], "           FROM t a", "{out}");
        assert_eq!(lines[3], "LEFT OUTER JOIN u b", "{out}");
        assert_eq!(lines[4], "             ON a.id = b.id", "{out}");
        assert_eq!(lines[5], "          WHERE a.x = 1;", "{out}");
        assert!(
            lines
                .iter()
                .any(|l| l.to_ascii_uppercase().starts_with("INSERT INTO")),
            "INSERT = standard(통과) 그대로\n{out}"
        );
        assert!(
            !lines.iter().take(6).any(|l| l.contains('\t')),
            "강 줄에 탭 없음\n{out}"
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
    }

    /// standard(기본)는 종전과 같다 — 강 모듈은 호출되지 않는다.
    #[test]
    fn standard_unchanged() {
        let o = Options {
            indent: Indent::Spaces(4),
            tab_width: 4,
            ..Options::default()
        };
        let out = format_basic("select a from t where x = 1", &o);
        assert!(out.starts_with("SELECT\n    a\nFROM\n    t\n"), "{out}");
    }
}
