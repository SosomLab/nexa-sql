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
    extract_binds_with(sql, &classify(sql))
}

/// [`extract_binds`] — 분류를 이미 가진 호출자용(같은 글을 두 번 토큰화하지 않게 · docs/63 §4 계측).
pub fn extract_binds_with(sql: &str, classes: &[Class]) -> Vec<BindRef> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // 대괄호 깊이 — PostgreSQL 배열 조각 `a[1:3]` · `a[:n]`의 `:`는 바인드가 아니다(Oracle에는 대괄호 문법이 없고,
    // SQL Server의 `[이름]`은 인용 식별자로 분류돼 여기까지 오지 않는다 · 09-21 조사에서 나온 오탐).
    let mut bracket = 0i32;
    while i < b.len() {
        if classes[i] == Class::Code {
            match b[i] {
                b'[' => bracket += 1,
                b']' => bracket = (bracket - 1).max(0),
                _ => {}
            }
        }
        if b[i] == b':' && classes[i] == Class::Code && bracket == 0 {
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

/// SQLite의 이름 매개변수 표기 `@이름` · `$이름`을 `:이름`으로 맞춘다(같은 길이 — 오프셋·분류가 그대로다).
/// SQLite에서는 세 표기가 모두 이름 매개변수라 **한 변수**로 다룬다(docs/63 §3-1 · T-162 ①) — 종전엔 `:이름`만 변수로
/// 묶여 `@x`·`$x`가 그대로 서버에 가 `Wrong number of parameters`가 났다. 문자열·주석·인용 식별자 안과
/// 식별자 가운데의 `$`(`a$b`)는 건드리지 않는다. 바꿀 것이 없으면 빌리기만 한다.
pub fn unify_sqlite_markers(sql: &str) -> std::borrow::Cow<'_, str> {
    if !sql.as_bytes().iter().any(|c| matches!(c, b'@' | b'$')) {
        return std::borrow::Cow::Borrowed(sql);
    }
    let classes = classify(sql);
    let mut b = sql.as_bytes().to_vec();
    let mut changed = false;
    for i in 0..b.len() {
        if matches!(b[i], b'@' | b'$')
            && classes[i] == Class::Code
            && (i == 0 || !is_ident_char(b[i - 1]) && b[i - 1] != b'@' && b[i - 1] != b':')
            && b.get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            b[i] = b':';
            changed = true;
        }
    }
    if changed {
        // ASCII 한 글자를 ASCII 한 글자로만 바꿨다 — UTF-8이 그대로다.
        std::borrow::Cow::Owned(String::from_utf8(b).expect("ascii-for-ascii swap"))
    } else {
        std::borrow::Cow::Borrowed(sql)
    }
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
    fn sqlite_markers_become_one_variable() {
        let s = "SELECT @a, $b, :c, a$b, '@x $y', \"@q\", x@y -- @z
 FROM t WHERE k = @a";
        let u = unify_sqlite_markers(s);
        assert_eq!(u.len(), s.len());
        assert_eq!(
            &*u,
            "SELECT :a, :b, :c, a$b, '@x $y', \"@q\", x@y -- @z
 FROM t WHERE k = :a"
        );
        assert_eq!(unique_names(&extract_binds(&u)), ["A", "B", "C"]);
        assert!(matches!(
            unify_sqlite_markers("SELECT 1, $1, @@x"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

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

    /// PG 배열 조각의 `:`는 바인드가 아니다 — 대괄호 밖의 바인드는 그대로.
    #[test]
    fn array_slices_are_not_binds() {
        let r = extract_binds("SELECT arr[1:3], arr[:2], arr[2:], :x FROM t WHERE y = :y");
        assert_eq!(unique_names(&r), vec!["X".to_string(), "Y".to_string()]);
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
