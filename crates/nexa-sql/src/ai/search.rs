//! ★ 객체 검색 하네스(docs/110 §15-17 · 사용자 11-11 "이미 읽힌 메타로 더 빠르게 대상을 찾게"): 탐색기 메타 스냅샷(메모리)만 본다 —
//! DBMS 질의 0. 이름 등급 점수(정확 > 접두 > 낱말 머리 > 부분 > 퍼지) · 낱말 AND · `schema.name` 질의 · 현재 스키마 가산 · 읽힌
//! 컬럼 이름 검색. 순수 함수(시험) — 스냅샷 순회·출력은 `explorer::ai_search_objects`.

/// 질의 한 낱말 → 이름 점수(둘 다 소문자). 없으면 None. 빈 낱말 = 0.
pub(crate) fn name_score(q: &str, name: &str) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    if name == q {
        return Some(1000);
    }
    let extra = |n: usize| n.min(100) as i32;
    if name.starts_with(q) {
        return Some(800 - extra(name.len() - q.len()));
    }
    // 낱말 머리: `_` `$` `#` 숫자 경계 뒤에서 시작(`cust_order`에서 `order`).
    let mut word_head = false;
    for w in name.split(|c: char| !c.is_alphanumeric()) {
        if w.starts_with(q) {
            word_head = true;
            break;
        }
    }
    if word_head {
        return Some(600 - extra(name.len() - q.len()));
    }
    if let Some(i) = name.find(q) {
        return Some(400 - extra(i));
    }
    let qc: Vec<char> = q.chars().collect();
    crate::palette::fuzzy_score_lower(&qc, name).map(|s| s.clamp(0, 199))
}

/// 질의 전체 → (스키마 한정 · 이름 낱말들 · 소문자). `SCHEMA.NAME` 꼴이면 스키마를 떼고, 띄어쓰기는 AND 낱말.
pub(crate) fn parse_query(query: &str) -> (Option<String>, Vec<String>) {
    let q = query.trim().to_lowercase();
    let (schema, rest) = match q.split_once('.') {
        Some((s, r)) if !s.contains(' ') && !s.is_empty() => (Some(s.to_string()), r.to_string()),
        _ => (None, q),
    };
    let words: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
    (schema, words)
}

/// 모든 낱말이 맞으면 점수 합(낱말이 없으면 0).
pub(crate) fn words_score(words: &[String], name: &str) -> Option<i32> {
    let mut total = 0;
    for w in words {
        total += name_score(w, name)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn score_order_exact_prefix_word_sub_fuzzy() {
        let exact = name_score("order", "order").unwrap();
        let prefix = name_score("order", "orders").unwrap();
        let word = name_score("order", "cust_order_line").unwrap();
        let sub = name_score("order", "reorder_log").unwrap();
        let fuzzy = name_score("odr", "order").unwrap();
        assert!(
            exact > prefix && prefix > word && word > sub && sub > fuzzy,
            "{exact} {prefix} {word} {sub} {fuzzy}"
        );
        assert_eq!(name_score("zzz", "order"), None);
        assert_eq!(name_score("", "order"), Some(0));
    }

    #[test]
    fn query_parse_and_and_words() {
        assert_eq!(
            parse_query("HR.emp"),
            (Some("hr".into()), vec!["emp".to_string()])
        );
        assert_eq!(
            parse_query("cust order"),
            (None, vec!["cust".to_string(), "order".to_string()])
        );
        assert!(words_score(&["cust".into(), "order".into()], "cust_order").is_some());
        assert_eq!(
            words_score(&["cust".into(), "zzz".into()], "cust_order"),
            None
        );
    }
}
