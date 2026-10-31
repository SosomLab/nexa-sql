//! ★ T-255 강(river) 정렬(10-30 사용자 결정 "Basic 옵션으로" · 10-31 구현 · docs/95 §10): `format.indent_style = river`일 때 레이아웃 IR을
//! **후처리**한다 — Walker(layout.rs)는 손대지 않는다.
//!
//! 규칙(95 §10-1): 질의 블록(같은 깊이의 절 키워드 묶음)마다 **강 열** = 절·JOIN 키워드 폭의 최댓값 · 절 키워드는 강 열에서 끝나게
//! 오른쪽 맞춤 · 첫 항목은 절 줄에 잇는다 · 이어지는 항목 = 강 열 + 1(줄 앞 콤마는 강 열에) · `ON`·`AND`/`OR`도 강에 오른쪽 맞춤 ·
//! 서브쿼리(더 깊은 절)는 자기 깊이의 강 · INSERT · MERGE 문장과 통과(Raw) 줄은 그대로(standard). 강 줄의 들여쓰기는 **공백**으로만
//! (열 정렬이 목적 · `Line::col`).
//!
//! ★ 2차(10-31 · 95 §10-1 "서브쿼리 안쪽 강 · CASE 상대 들여쓰기"): **열 프레임** — 앞 줄이 `(`로 끝나고 이 줄이 더 깊으면 안쪽 블록의 기준
//! 열 = `(` 다음 열(안쪽 강은 그 열 + 안쪽 최댓값) · 닫는 `)` = 여는 줄의 `(` 열 · 여러 줄 CASE = `CASE` 낱말 열 기준(WHEN/ELSE = +한 단 ·
//! END = CASE 열). 프레임은 중첩되고 깊이가 열림 이하로 돌아오면 닫힌다.

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

/// 세그먼트 줄마다 속한 **강의 깊이**: 절·JOIN = 자기 깊이 · `ON`/`USING` 절 = 한 단계 위 · **조건(AND/OR) = 가장 가까운 앞 절의 깊이**
/// (★ 10-31 결함 수정: `cond_indent = same`이면 조건 줄이 절 키워드와 같은 깊이라 종전 "한 단계 위" 가정이 틀렸다 · ON 조건은 늘 같은 열) ·
/// 항목·그 밖 = 한 단계 위(0이면 0).
fn river_levels(seg: &[Line]) -> Vec<usize> {
    let mut out = Vec::with_capacity(seg.len());
    // 최근 절(깊이, 강 깊이) — 더 깊은 절(서브쿼리)이 끝나 바깥으로 돌아오면 바깥 절이 다시 걸린다(`<=` 탐색).
    let mut clauses: Vec<(usize, usize)> = Vec::new();
    for line in seg {
        let lv = match line.role {
            Role::Clause => {
                let w = first_word(line);
                if w == "ON" || w == "USING" {
                    line.indent.saturating_sub(1)
                } else {
                    line.indent
                }
            }
            Role::Join => line.indent,
            Role::Cond => clauses
                .iter()
                .rev()
                .find(|(ind, _)| *ind <= line.indent)
                .map_or(line.indent.saturating_sub(1), |(_, lv)| *lv),
            _ => line.indent.saturating_sub(1),
        };
        if matches!(line.role, Role::Clause | Role::Join) {
            clauses.retain(|(ind, _)| *ind < line.indent);
            clauses.push((line.indent, lv));
        }
        out.push(lv);
    }
    out
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

/// ★ 열 프레임(서브쿼리 괄호 · 여러 줄 CASE): 연 줄의 깊이 · 안쪽 첫 깊이의 기준 열 · 닫는 줄의 열.
#[derive(Clone, Copy, Debug)]
struct Frame {
    open_indent: usize,
    inner0: usize,
    close_col: usize,
}

/// 깊이 `ind`의 기준 열 — 가장 안쪽의 열린 프레임(`ind > open_indent`) 기준 · 없으면 깊이 × 단위.
fn base_of(ind: usize, frames: &[Frame], unit: usize) -> usize {
    match frames.iter().rev().find(|f| ind > f.open_indent) {
        Some(f) => f.inner0 + (ind - f.open_indent - 1) * unit,
        None => ind * unit,
    }
}

/// 줄 앞 콤마 모드의 콤마 줄은 렌더가 `, `(2칸)를 먼저 그린다.
fn lead_width(line: &Line, opts: &Options) -> usize {
    if opts.comma == Comma::Leading && matches!(line.parts.first(), Some(Part::Comma)) {
        2
    } else {
        0
    }
}

/// 앞 줄(`prev`)이 이 줄(`cur`)의 블록을 열었는가 — `(`로 끝남 = 괄호 프레임 · `cur`가 CASE 가지(WHEN/ELSE) = CASE 프레임(`CASE` 낱말 열).
fn open_frame(prev: &Line, cur: &Line, opts: &Options, unit: usize) -> Option<Frame> {
    let col = prev.col?;
    let plain = prev.plain(opts);
    let text = plain.trim_end();
    let lead = lead_width(prev, opts);
    let tw = opts.tab_width.max(1);
    if text.ends_with('(') {
        let oc = col + lead + display_width(text, tw).saturating_sub(1);
        return Some(Frame {
            open_indent: cur.indent.saturating_sub(1),
            inner0: oc + 1,
            close_col: oc,
        });
    }
    if cur.role == Role::Case {
        let up = text.to_ascii_uppercase();
        let idx = up.rfind("CASE")?;
        let cc = col + lead + display_width(&text[..idx], tw);
        return Some(Frame {
            open_indent: cur.indent.saturating_sub(1),
            inner0: cc + unit,
            close_col: cc,
        });
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
        let lvs = river_levels(&seg);
        // 2) 깊이별 강 폭 = 절·JOIN·ON·조건 줄 머리 예약어 폭의 최댓값.
        let mut width: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
        for (idx, l) in seg.iter().enumerate() {
            if matches!(l.role, Role::Clause | Role::Join | Role::Cond) {
                let (n, w, _) = keyword_head(&l.parts, tw);
                if n > 0 {
                    let lv = lvs[idx];
                    let cur = width.entry(lv).or_insert(0);
                    *cur = (*cur).max(w);
                }
            }
        }
        // 3) 줄마다 열 계산 · 절 줄에 첫 항목 잇기 · ★ 열 프레임(괄호·CASE).
        let mut frames: Vec<Frame> = Vec::new();
        let mut k = 0;
        while k < seg.len() {
            let mut l = seg[k].clone();
            // 프레임 닫기: 깊이가 열림 이하로 돌아왔다 — 닫는 줄(`)` · CASE `END`)이면 그 열에.
            let mut closer: Option<usize> = None;
            while frames.last().is_some_and(|f| l.indent <= f.open_indent) {
                let Some(f) = frames.pop() else {
                    break;
                };
                let is_closer = l.indent == f.open_indent
                    && (l.role == Role::Close || (l.role == Role::Case && first_word(&l) == "END"));
                if is_closer {
                    closer = Some(f.close_col);
                    break;
                }
            }
            // 프레임 열기: 앞 줄보다 깊어졌고 앞 줄이 `(`로 끝났거나 이 줄이 CASE 가지.
            if let Some(prev) = out.last() {
                if l.indent > prev.indent && prev.role != Role::Raw {
                    if let Some(f) = open_frame(prev, &l, opts, unit) {
                        frames.push(f);
                    }
                }
            }
            let lv = lvs[k];
            let base = base_of(lv, &frames, unit);
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
                                let next_lv = lvs[k + 1];
                                // 첫 조건은 한 단계 안(기본) 또는 같은 깊이(`cond_indent = same` · ON) 어느 쪽이든 잇는다.
                                let same_col_cond =
                                    next.role == Role::Cond && next.indent == l.indent;
                                let joinable = matches!(next.role, Role::Item | Role::Cond)
                                    && next_lv == lv
                                    && (next.indent == l.indent + 1 || same_col_cond)
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
                // 닫는 괄호 · CASE · 주석 · 집합 연산자 · 그 밖 = 자기 깊이 열(프레임 안이면 그 기준 · 닫는 줄 = 연 줄의 `(`/`CASE` 열).
                _ => {
                    l.col = Some(closer.unwrap_or_else(|| base_of(l.indent, &frames, unit)));
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

    /// ★ 2차(95 §10-1): 서브쿼리 = `(` 다음 열이 안쇽 강의 기준 · 닫는 `)` = `(` 열 · EXISTS 서브쿼리도 · 여러 줄 CASE = `CASE` 열 기준(WHEN +한 단 ·
    /// END = CASE 열) · 멱등 · 토큰 보존.
    #[test]
    fn river_subquery_and_case_frames() {
        let mut o = river_opts(Comma::Leading);
        o.case_inline_max = 40;
        let src = "select a.k, (select max(b.d) from t2 b where b.k = a.k) as last_d, case when a.q > 100 then 'H' when a.q > 0 then 'L' else 'N' end grp from t1 a where exists (select 1 from t3 c where c.k = a.k)";
        let out = format_basic(src, &o);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "SELECT a.k", "{out}");
        assert_eq!(lines[1], "     , (", "{out}");
        assert_eq!(
            lines[2], "        SELECT max(b.d)",
            "안쪽 강 = `(` 다음 열\n{out}"
        );
        assert_eq!(lines[3], "          FROM t2 b", "{out}");
        assert_eq!(lines[4], "         WHERE b.k = a.k", "{out}");
        assert_eq!(lines[5], "       ) AS last_d", "닫는 괄호 = `(` 열\n{out}");
        assert_eq!(lines[6], "     , CASE", "{out}");
        assert_eq!(
            lines[7], "           WHEN a.q > 100 THEN 'H'",
            "WHEN = CASE 열 + 한 단\n{out}"
        );
        assert_eq!(lines[8], "           WHEN a.q > 0 THEN 'L'", "{out}");
        assert_eq!(lines[9], "           ELSE 'N'", "{out}");
        assert_eq!(lines[10], "       END grp", "END = CASE 열\n{out}");
        assert_eq!(lines[11], "  FROM t1 a", "{out}");
        assert_eq!(lines[12], " WHERE EXISTS (", "{out}");
        assert_eq!(lines[13], "               SELECT 1", "{out}");
        assert_eq!(lines[14], "                 FROM t3 c", "{out}");
        assert_eq!(lines[15], "                WHERE c.k = a.k", "{out}");
        assert_eq!(lines[16], "              )", "{out}");
        // 괄호는 띄어 비교(표준 레이아웃이 `(`를 줄로 분리).
        let paren = |t: &str| t.replace('(', " ( ").replace(')', " ) ");
        assert_eq!(
            words(&paren(src))
                .iter()
                .map(|w| w.to_ascii_uppercase())
                .collect::<Vec<_>>(),
            words(&paren(&out))
                .iter()
                .map(|w| w.to_ascii_uppercase())
                .collect::<Vec<_>>()
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
    }

    /// ★ 결함 수정(10-31 협업 발견): `cond_indent = same`이면 WHERE 첫 조건이 0열로 떨어졌다 — 조건 줄의 강 깊이 = 가장 가까운 앞 절 ·
    /// 첫 조건은 같은 깊이여도 절 줄에 잇는다 · 서브쿼리 안 WHERE도 · 멱등.
    #[test]
    fn river_cond_indent_same_joins_first_condition() {
        let mut o = river_opts(Comma::Leading);
        o.cond_indent = false;
        let src = "select a, (select max(b.d) from t2 b where b.k = a.k and b.x = 1) m from t where x = 1 and y = 2 order by a";
        let out = format_basic(src, &o);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "  SELECT a", "{out}");
        assert_eq!(lines[1], "       , (", "{out}");
        assert_eq!(lines[2], "          SELECT max(b.d)", "{out}");
        assert_eq!(lines[3], "            FROM t2 b", "{out}");
        assert_eq!(
            lines[4], "           WHERE b.k = a.k",
            "안쪽 WHERE 첫 조건도 같은 줄\n{out}"
        );
        assert_eq!(lines[5], "             AND b.x = 1", "{out}");
        assert_eq!(lines[6], "         ) m", "{out}");
        assert_eq!(lines[7], "    FROM t", "{out}");
        assert_eq!(lines[8], "   WHERE x = 1", "첫 조건은 WHERE 줄에\n{out}");
        assert_eq!(lines[9], "     AND y = 2", "{out}");
        assert_eq!(lines[10], "ORDER BY a", "{out}");
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
