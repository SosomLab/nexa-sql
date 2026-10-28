//! 별칭 생성 부품(인텔리센스 완성 · Generate SQL 공용 · 10-15 · 30 §2 등재) — 같은 문제를 두 번째 만나 부품으로 올림.
//! - [`abbr`] = 단어 첫 글자 약어(`sales_customer` → `sc` · `MyTable` → `mt`) · 키워드 모양이면 `1` 꼬리 · 겹치면 `2`, `3` …(소문자로 만든다).
//! - [`letter`] = 문장 안에서 아직 안 쓰인 첫 글자 A, B, C … · Z까지 쓰였으면 `A2`, `B2` ….
//! - [`make`] = 설정 둘(`intel.alias_style` = 약어/문자 · `intel.alias_upper` = 대문자 여부)을 받아 하나로: 방식 → 대소문자.

/// 단어 첫 글자 약어(소문자). `taken`(대소문자 무시)에 있으면 숫자 꼬리(`e` → `e2`).
#[must_use]
pub fn abbr(name: &str, taken: &[&str]) -> String {
    let base = name.rsplit('.').next().unwrap_or(name);
    let mut a = String::new();
    let mut prev = '_';
    for ch in base.chars() {
        if ch.is_alphanumeric()
            && (prev == '_'
                || prev == '$'
                || prev == '#'
                || (ch.is_uppercase() && prev.is_lowercase()))
        {
            a.push(ch.to_ascii_lowercase());
        }
        prev = ch;
    }
    if a.is_empty() {
        a = base.chars().take(1).collect::<String>().to_lowercase();
    }
    if a.is_empty() {
        a.push('t');
    }
    const KW: &[&str] = &[
        "in", "as", "or", "on", "by", "is", "if", "to", "at", "do", "go", "no",
    ];
    if KW.contains(&a.as_str()) {
        a.push('1');
    }
    let mut out = a.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&out)) {
        out = format!("{a}{n}");
        n += 1;
    }
    out
}

/// A, B, C … 순서 별칭(대문자): `taken`(대소문자 무시)에 없는 첫 글자 · Z까지 다 쓰였으면 `A2`, `B2` ….
#[must_use]
pub fn letter(taken: &[&str]) -> String {
    for round in 0u32.. {
        for ch in 'A'..='Z' {
            let cand = if round == 0 {
                ch.to_string()
            } else {
                format!("{ch}{}", round + 1)
            };
            if !taken.iter().any(|t| t.eq_ignore_ascii_case(&cand)) {
                return cand;
            }
        }
    }
    "A".to_string()
}

/// 대소문자 적용(`intel.alias_upper`).
#[must_use]
pub fn cased(a: &str, upper: bool) -> String {
    if upper {
        a.to_ascii_uppercase()
    } else {
        a.to_ascii_lowercase()
    }
}

/// 설정대로 별칭 하나: `letters`(A, B, C …) 또는 약어 → `upper`면 대문자.
#[must_use]
pub fn make(name: &str, taken: &[&str], letters: bool, upper: bool) -> String {
    let a = if letters {
        letter(taken)
    } else {
        abbr(name, taken)
    };
    cased(&a, upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbr_rules() {
        assert_eq!(abbr("sales_customer", &[]), "sc");
        assert_eq!(abbr("EMP", &["e"]), "e2");
        assert_eq!(abbr("EMP", &["E"]), "e2", "겹침 판정은 대소문자 무시");
        assert_eq!(abbr("MyTable", &[]), "mt");
        assert_eq!(abbr("orders", &[]), "o");
        assert_eq!(abbr("sch.item_set", &[]), "is1", "키워드 모양이면 1");
        assert_eq!(
            abbr("DMS_I002150_261015_BAK", &[]),
            "di2b",
            "숫자로 시작하는 낱말도 첫 글자"
        );
    }

    #[test]
    fn letter_rules() {
        assert_eq!(letter(&[]), "A");
        assert_eq!(letter(&["a", "B"]), "C");
        let all: Vec<String> = ('A'..='Z').map(|c| c.to_string()).collect();
        let taken: Vec<&str> = all.iter().map(String::as_str).collect();
        assert_eq!(letter(&taken), "A2");
    }

    /// 설정 조합 넷(방식 × 대문자) — 기본 = 약어 + 대문자.
    #[test]
    fn make_follows_both_settings() {
        assert_eq!(make("sales_customer", &[], false, true), "SC");
        assert_eq!(make("sales_customer", &[], false, false), "sc");
        assert_eq!(make("sales_customer", &[], true, true), "A");
        assert_eq!(make("sales_customer", &["A"], true, false), "b");
        assert_eq!(cased("a2", true), "A2");
    }
}
