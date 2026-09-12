//! `:NAME` 바인드 참조 추출 — 문자열·주석·인용 식별자 밖에서만(docs/08 §4).
//! `::`(PG 캐스트)·`:=`(대입 연산자)는 바인드가 아니다. `:1` 같은 숫자 위치 바인드도 받는다.

use crate::lexer::{classify, is_ident_char, Class};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BindRef {
    /// 정규화(대문자) 이름. 숫자 바인드는 그대로("1").
    pub name: String,
    /// `:` 포함 시작 바이트.
    pub start: usize,
    /// 이름 끝(exclusive).
    pub end: usize,
}

pub fn extract_binds(sql: &str) -> Vec<BindRef> {
    let b = sql.as_bytes();
    let classes = classify(sql);
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b':' && classes[i] == Class::Code {
            // `::` 캐스트 — 둘 다 건너뛴다.
            if i + 1 < b.len() && b[i + 1] == b':' {
                i += 2;
                continue;
            }
            // 직전이 `:`(캐스트의 두 번째)면 무시 — 위에서 처리되지만 안전망.
            if i > 0 && b[i - 1] == b':' {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            // 첫 글자: 문자·_·숫자(위치 바인드). `:=`·`: ` 등은 제외.
            if j < b.len() && (b[j].is_ascii_alphabetic() || b[j] == b'_' || b[j].is_ascii_digit())
            {
                let digits_only = b[j].is_ascii_digit();
                while j < b.len()
                    && if digits_only {
                        b[j].is_ascii_digit()
                    } else {
                        is_ident_char(b[j])
                    }
                {
                    j += 1;
                }
                out.push(BindRef {
                    name: sql[i + 1..j].to_ascii_uppercase(),
                    start: i,
                    end: j,
                });
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 첫 등장 순서를 유지한 고유 이름 목록.
pub fn unique_names(refs: &[BindRef]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for r in refs {
        if !v.iter().any(|n| n == &r.name) {
            v.push(r.name.clone());
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_named_and_skips_masked() {
        let sql = "SELECT :V_A, ':nope', a::int, \":q\" FROM t WHERE x = :v_b -- :c\n AND y = :V_A";
        let refs = extract_binds(sql);
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["V_A", "V_B", "V_A"]);
        assert_eq!(unique_names(&extract_binds(sql)), vec!["V_A", "V_B"]);
    }

    #[test]
    fn assignment_operator_is_not_a_bind() {
        let sql = ":V_PRG_NM := 'SP_M4P';";
        let r = extract_binds(sql);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "V_PRG_NM");
        assert_eq!(&sql[r[0].start..r[0].end], ":V_PRG_NM");
    }

    #[test]
    fn positional_numeric() {
        let r = extract_binds("SELECT :1, :22 FROM dual");
        assert_eq!(
            r.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            vec!["1", "22"]
        );
    }
}
