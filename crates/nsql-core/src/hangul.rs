//! 한글 **자모열 검색**(사용자 09-23 "입력이 검색에 쓰이는 모든 경우는 자모 완성과 상관없이 한글 검색") — 의존 0.
//!
//! 음절을 **입력 순서의 자모열**로 펴서 비교한다: "ㄱ" ⊂ 가 · "기" ⊂ 긴 · "달" ⊂ 닭(겹받침 = 타자 순서 ㄹ,ㄱ) · "고" ⊂ 과(겹모음).
//! 일치는 **글자 경계에서 시작**해야 하고, 마지막 질의 글자는 조합 중일 수 있어 하이 글자 중간에서 끝나도 된다
//! (그래서 "긴"은 "기나"의 앞에도 걸린다 — 조합 중 입력의 본래 모호함). 조합기 자체는 nexa-ctl `hangul`(타입어헤드)에 있고,
//! 여기는 GUI 필터·편집기 찾기·파일 검색 엔진(`nsql-search`)이 함께 쓰는 순수 함수만 둔다.

/// 초성 19자(유니코드 순).
const CHO: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];
/// 중성 21자(유니코드 순).
const JUNG: [char; 21] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ',
    'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];
/// 종성 27자(인덱스 1..=27 — 0은 받침 없음).
const JONG: [char; 27] = [
    'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ', 'ㅁ',
    'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];
/// 겹모음(결합 → 타자 순서 둘).
const JUNG_SPLIT: [(char, char, char); 7] = [
    ('ㅘ', 'ㅗ', 'ㅏ'),
    ('ㅙ', 'ㅗ', 'ㅐ'),
    ('ㅚ', 'ㅗ', 'ㅣ'),
    ('ㅝ', 'ㅜ', 'ㅓ'),
    ('ㅞ', 'ㅜ', 'ㅔ'),
    ('ㅟ', 'ㅜ', 'ㅣ'),
    ('ㅢ', 'ㅡ', 'ㅣ'),
];
/// 겹받침(결합 → 타자 순서 둘).
const JONG_SPLIT: [(char, char, char); 11] = [
    ('ㄳ', 'ㄱ', 'ㅅ'),
    ('ㄵ', 'ㄴ', 'ㅈ'),
    ('ㄶ', 'ㄴ', 'ㅎ'),
    ('ㄺ', 'ㄹ', 'ㄱ'),
    ('ㄻ', 'ㄹ', 'ㅁ'),
    ('ㄼ', 'ㄹ', 'ㅂ'),
    ('ㄽ', 'ㄹ', 'ㅅ'),
    ('ㄾ', 'ㄹ', 'ㅌ'),
    ('ㄿ', 'ㄹ', 'ㅍ'),
    ('ㅀ', 'ㄹ', 'ㅎ'),
    ('ㅄ', 'ㅂ', 'ㅅ'),
];

fn split2(table: &[(char, char, char)], c: char) -> Option<(char, char)> {
    table
        .iter()
        .find(|&&(z, _, _)| z == c)
        .map(|&(_, a, b)| (a, b))
}

/// 완성 음절(가~힣)인가.
#[must_use]
pub fn is_syllable(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

/// 호환 자모(ㄱ~ㅣ)인가.
#[must_use]
pub fn is_compat_jamo(c: char) -> bool {
    ('\u{3131}'..='\u{318E}').contains(&c)
}

/// 한글(음절·자모)이 들어 있는가 — 자모열 검색을 쓸지 정하는 기준.
#[must_use]
pub fn has_hangul(s: &str) -> bool {
    s.chars().any(|c| is_syllable(c) || is_compat_jamo(c))
}

/// [`has_hangul`]의 글자 배열 판.
#[must_use]
pub fn has_hangul_chars(s: &[char]) -> bool {
    s.iter().any(|&c| is_syllable(c) || is_compat_jamo(c))
}

/// 대소문자 접기(글자 수가 변하지 않는 소문자화만 — 인덱스 대응을 지키려고).
#[must_use]
pub fn fold_char(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// 글자 하나를 입력 순서의 자모열로 편다 — 음절 = 초·중·종(겹모음·겹받침은 둘로) · 호환 겹자모도 둘로 · 그 밖은 그대로
/// (`fold`면 한글 아닌 글자는 [`fold_char`]).
pub fn decompose_into(c: char, fold: bool, out: &mut Vec<char>) {
    if is_syllable(c) {
        let s = c as u32 - 0xAC00;
        let (ci, vi, ti) = (
            (s / 588) as usize,
            ((s % 588) / 28) as usize,
            (s % 28) as usize,
        );
        out.push(CHO[ci]);
        match split2(&JUNG_SPLIT, JUNG[vi]) {
            Some((a, b)) => out.extend([a, b]),
            None => out.push(JUNG[vi]),
        }
        if ti > 0 {
            match split2(&JONG_SPLIT, JONG[ti - 1]) {
                Some((a, b)) => out.extend([a, b]),
                None => out.push(JONG[ti - 1]),
            }
        }
    } else if let Some((a, b)) = split2(&JUNG_SPLIT, c).or_else(|| split2(&JONG_SPLIT, c)) {
        out.extend([a, b]);
    } else {
        out.push(if fold { fold_char(c) } else { c });
    }
}

/// 문자열 → 자모열(질의 준비용).
#[must_use]
pub fn decompose(s: &str, fold: bool) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        decompose_into(c, fold, &mut out);
    }
    out
}

/// [`decompose`]의 글자 배열 판.
#[must_use]
pub fn decompose_chars(s: &[char], fold: bool) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for &c in s {
        decompose_into(c, fold, &mut out);
    }
    out
}

/// 자모열 검색 — `hay`(글자 배열)에서 자모열 `q`(이미 [`decompose`]한 질의 · `fold`도 같게)가 **글자 경계에서 시작**해
/// 부분열로 들어가는 구간을 전부 `(시작 글자, 끝 글자 배타)`로 `out`에 더한다(겹치지 않게 · 앞에서부터).
pub fn find_jamo(hay: &[char], q: &[char], fold: bool, out: &mut Vec<(usize, usize)>) {
    if q.is_empty() || hay.is_empty() {
        return;
    }
    let mut jam: Vec<char> = Vec::with_capacity(hay.len() * 2);
    let mut origin: Vec<usize> = Vec::with_capacity(hay.len() * 2);
    for (i, &c) in hay.iter().enumerate() {
        let n0 = jam.len();
        decompose_into(c, fold, &mut jam);
        origin.extend(std::iter::repeat_n(i, jam.len() - n0));
    }
    let n = q.len();
    let mut j = 0usize;
    while j + n <= jam.len() {
        let at_boundary = j == 0 || origin[j] != origin[j - 1];
        if at_boundary && jam[j..j + n] == *q {
            let (s, e) = (origin[j], origin[j + n - 1] + 1);
            out.push((s, e));
            // 다음 후보 = 끝난 글자의 첫 자모.
            j += (n..)
                .find(|&k| j + k >= jam.len() || origin[j + k] >= e)
                .unwrap_or(n);
        } else {
            j += 1;
        }
    }
}

/// `hay`에 자모열 `q`가 들어 있는가.
#[must_use]
pub fn contains_jamo(hay: &str, q: &[char], fold: bool) -> bool {
    let h: Vec<char> = hay.chars().collect();
    let mut out = Vec::with_capacity(1);
    find_jamo(&h, q, fold, &mut out);
    !out.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_syllables_and_compounds() {
        assert_eq!(decompose("가", false), vec!['ㄱ', 'ㅏ']);
        assert_eq!(decompose("긴", false), vec!['ㄱ', 'ㅣ', 'ㄴ']);
        assert_eq!(decompose("닭", false), vec!['ㄷ', 'ㅏ', 'ㄹ', 'ㄱ']);
        assert_eq!(decompose("과A", false), vec!['ㄱ', 'ㅗ', 'ㅏ', 'A']);
        assert_eq!(decompose("과A", true), vec!['ㄱ', 'ㅗ', 'ㅏ', 'a']);
        assert_eq!(decompose("ㄳ", false), vec!['ㄱ', 'ㅅ']);
        assert!(has_hangul("x기"));
        assert!(has_hangul("ㄱ"));
        assert!(!has_hangul("abc"));
    }

    #[test]
    fn find_jamo_partial_last_syllable() {
        let hay: Vec<char> = "가나 기타 긴급 닭".chars().collect();
        let f = |q: &str| {
            let mut out = Vec::new();
            find_jamo(&hay, &decompose(q, false), false, &mut out);
            out
        };
        assert_eq!(
            f("ㄱ"),
            vec![(0, 1), (3, 4), (6, 7), (7, 8)],
            "급도 ㄱ으로 시작"
        );
        assert_eq!(f("기"), vec![(3, 4), (6, 7)]);
        assert_eq!(f("긴"), vec![(6, 7)]);
        assert_eq!(f("긴급"), vec![(6, 8)]);
        assert_eq!(f("달"), vec![(9, 10)], "달 ⊂ 닭(겹받침 타자 순서)");
        assert_eq!(f("가나"), vec![(0, 2)]);
        assert!(f("ㅏ").is_empty(), "글자 중간에서 시작하지 않는다");
        assert!(f("각").is_empty());
    }

    #[test]
    fn find_jamo_last_syllable_may_cross_boundary_and_folds_latin() {
        let hay: Vec<char> = "기나 Sql기".chars().collect();
        let mut out = Vec::new();
        find_jamo(&hay, &decompose("긴", false), false, &mut out);
        assert_eq!(out, vec![(0, 2)], "조합 중 긴 = 기+ㄴ");
        let mut out = Vec::new();
        find_jamo(&hay, &decompose("sqlㄱ", true), true, &mut out);
        assert_eq!(out, vec![(3, 7)]);
        assert!(contains_jamo(
            "이름_기타.sql",
            &decompose("기", false),
            false
        ));
        assert!(!contains_jamo("abc", &decompose("기", false), false));
    }
}
