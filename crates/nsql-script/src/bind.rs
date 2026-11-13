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

/// 식 안에서 불러도 **부작용이 없다고 아는** 이름(내장 스칼라·집계 함수 + 뒤에 `(`가 올 수 있는 예약어) — [`expr_is_plain`]의 허용 표.
/// 여기 없는 이름의 호출은 사용자 루틴일 수 있다(자율 트랜잭션 함수 = DML) → "식 하나의 평가"로 보지 않는다.
const PLAIN_CALLS: &[&str] = &[
    // 예약어(뒤에 괄호가 올 수 있는 것).
    "SELECT",
    "FROM",
    "WHERE",
    "AND",
    "OR",
    "NOT",
    "IN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "BETWEEN",
    "LIKE",
    "IS",
    "AS",
    "ON",
    "ANY",
    "ALL",
    "SOME",
    "OVER",
    "BY",
    "DISTINCT",
    "UNION",
    "VALUES",
    "INTERVAL",
    "DATE",
    "TIMESTAMP",
    // 숫자.
    "ABS",
    "CEIL",
    "CEILING",
    "FLOOR",
    "ROUND",
    "TRUNC",
    "TRUNCATE",
    "MOD",
    "POWER",
    "SQRT",
    "SIGN",
    "EXP",
    "LN",
    "LOG",
    "LOG10",
    "GREATEST",
    "LEAST",
    "BITAND",
    // 문자.
    "UPPER",
    "LOWER",
    "INITCAP",
    "SUBSTR",
    "SUBSTRING",
    "SUBSTRB",
    "INSTR",
    "LENGTH",
    "LENGTHB",
    "LEN",
    "CHAR_LENGTH",
    "TRIM",
    "LTRIM",
    "RTRIM",
    "LPAD",
    "RPAD",
    "REPLACE",
    "TRANSLATE",
    "CONCAT",
    "CONCAT_WS",
    "CHR",
    "ASCII",
    "CHAR",
    "REVERSE",
    "LEFT",
    "RIGHT",
    "REPEAT",
    "REPLICATE",
    "STUFF",
    "CHARINDEX",
    "POSITION",
    "REGEXP_REPLACE",
    "REGEXP_SUBSTR",
    "REGEXP_INSTR",
    "REGEXP_LIKE",
    "REGEXP_COUNT",
    "HEX",
    "RAWTOHEX",
    "HEXTORAW",
    "PRINTF",
    "FORMAT",
    // 변환·NULL.
    "TO_CHAR",
    "TO_NUMBER",
    "TO_DATE",
    "TO_TIMESTAMP",
    "TO_CLOB",
    "CAST",
    "CONVERT",
    "TRY_CAST",
    "TRY_CONVERT",
    "NVL",
    "NVL2",
    "COALESCE",
    "NULLIF",
    "IFNULL",
    "ISNULL",
    "IIF",
    "IF",
    "DECODE",
    "TYPEOF",
    // 날짜.
    "SYSDATE",
    "GETDATE",
    "NOW",
    "CURRENT_TIMESTAMP",
    "SYSTIMESTAMP",
    "SYSDATETIME",
    "CURDATE",
    "ADD_MONTHS",
    "MONTHS_BETWEEN",
    "LAST_DAY",
    "NEXT_DAY",
    "EXTRACT",
    "DATEADD",
    "DATEDIFF",
    "DATEPART",
    "DATENAME",
    "DATE_ADD",
    "DATE_SUB",
    "DATE_FORMAT",
    "DATE_TRUNC",
    "YEAR",
    "MONTH",
    "DAY",
    "EOMONTH",
    "STRFTIME",
    "JULIANDAY",
    "NUMTODSINTERVAL",
    "NUMTOYMINTERVAL",
    // 집계·순위.
    "COUNT",
    "SUM",
    "MIN",
    "MAX",
    "AVG",
    "LISTAGG",
    "STRING_AGG",
    "GROUP_CONCAT",
    "ROW_NUMBER",
    "RANK",
    "DENSE_RANK",
    // 환경 읽기.
    "SYS_CONTEXT",
    "USERENV",
    "DB_NAME",
];

/// ★ 식이 **계산만 하는 식**으로 보이는가(사용자 11-13 "해결해줘" · 데이터 보호): 수식 대입(`EXEC :V := 식`)은 "식 하나의 평가"라
/// 운영 2단 확인·보류 커밋·승인 카드를 건너뛰는데, 식 안에 **사용자 루틴 호출**이 있으면 그 루틴이 DML을 할 수 있다(Oracle 자율
/// 트랜잭션 함수 등). 보수적 판정 = 호출(`이름(`)이 전부 [`PLAIN_CALLS`]에 있고 · 한정 이름 호출(`pkg.f(`)·인용 이름 호출이 없고 ·
/// 시퀀스 소비(`.NEXTVAL`)가 없을 때만 참. 문자열·주석 안은 보지 않는다. 모르는 이름 = 거짓(확인을 거치게 한다).
#[must_use]
pub fn expr_is_plain(expr: &str) -> bool {
    let b = expr.as_bytes();
    let mut i = 0;
    // 마지막으로 본 뜻 있는 글자가 `.`인가(한정 이름).
    let mut after_dot = false;
    let next_is_paren = |mut j: usize| {
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        j < b.len() && b[j] == b'('
    };
    while i < b.len() {
        let c = b[i];
        match c {
            b'\'' => {
                i += 1;
                while i < b.len() {
                    if b[i] == b'\'' {
                        if b.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
                after_dot = false;
            }
            b'"' | b'`' => {
                i += 1;
                while i < b.len() && b[i] != c {
                    i += 1;
                }
                i += 1;
                if next_is_paren(i) {
                    return false;
                }
                after_dot = false;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            _ if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                let s = i;
                while i < b.len()
                    && (b[i].is_ascii_alphanumeric()
                        || matches!(b[i], b'_' | b'$' | b'#')
                        || b[i] >= 0x80)
                {
                    i += 1;
                }
                let word = expr[s..i].to_ascii_uppercase();
                // 바인드(`:이름`)는 변수 = 호출이 아니다.
                let is_bind = s > 0 && b[s - 1] == b':';
                if after_dot && matches!(word.as_str(), "NEXTVAL" | "CURRVAL") {
                    return false;
                }
                if !is_bind
                    && next_is_paren(i)
                    && (after_dot || !PLAIN_CALLS.contains(&word.as_str()))
                {
                    return false;
                }
                after_dot = false;
            }
            b'.' => {
                after_dot = true;
                i += 1;
            }
            _ => {
                if !c.is_ascii_whitespace() {
                    after_dot = false;
                }
                i += 1;
            }
        }
    }
    true
}

/// ★ 문장이 **계산만 하는 변수 대입**(`EXEC :이름 := 식` · 식 = [`expr_is_plain`])인가 — 데이터를 바꾸지 않으므로 트랜잭션 영향이
/// 없다(사용자 11-13 "Select만 했는데 왜 Commit 창이 뜨는거지": 실행 끝의 수식 재계산이 운영 보류에 "1문장 · 0행 미커밋"으로
/// 잡혔다). 여러 문장(`;`) · 프로시저 호출 · 사용자 루틴 호출이 든 식은 아니다.
#[must_use]
pub fn is_plain_bind_assign(sql: &str) -> bool {
    let Ok(Some(crate::command::Command::Exec { body })) = crate::command::parse_command(sql)
    else {
        return false;
    };
    let body = body.trim().trim_end_matches(';').trim();
    if body.contains(';') {
        return false;
    }
    let Some((lhs, rhs)) = body.split_once(":=") else {
        return false;
    };
    let lhs = lhs.trim();
    lhs.starts_with(':')
        && lhs.len() > 1
        && lhs[1..].bytes().all(is_ident_char)
        && !rhs.trim().is_empty()
        && expr_is_plain(rhs)
}

/// ★ 글 안의 `:이름` 바인드 표기를 대문자로(사용자 11-13 "Formula에도 변수명은 대문자로") — 문자열·주석·인용 식별자 안과
/// 그 밖의 글자는 그대로 · 이름은 ASCII만 바뀐다(한글 이름은 그대로).
#[must_use]
pub fn upper_binds(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut at = 0;
    for b in extract_binds(sql) {
        if b.start < at || b.end > sql.len() {
            continue;
        }
        out.push_str(&sql[at..b.start]);
        out.push_str(&sql[b.start..b.end].to_ascii_uppercase());
        at = b.end;
    }
    out.push_str(&sql[at..]);
    out
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
    #[test]
    fn plain_bind_assign_is_exec_of_a_pure_expression() {
        use super::is_plain_bind_assign as p;
        assert!(p("EXEC :STR1 := '이름-' || :NAME"));
        assert!(p("exec :n := (SELECT COUNT(*) FROM t WHERE a = :x);"));
        assert!(!p("EXEC :V := pkg.audit_and_get(:A)"));
        assert!(!p("EXEC pkg.do_it(:V)"));
        assert!(!p("EXEC :V := 1; DELETE FROM t"));
        assert!(!p("UPDATE t SET a = 1"));
    }

    #[test]
    fn expr_is_plain_only_for_known_builtins() {
        use super::expr_is_plain as p;
        // 계산만 하는 식.
        assert!(p(":A + 2"));
        assert!(p(":ABC * (:D + 2) / 3"));
        assert!(p("UPPER(:NAME) || '_' || TO_CHAR(SYSDATE, 'YYYYMMDD')"));
        assert!(p("CASE WHEN :N > 10 THEN 'BIG' ELSE 'SMALL' END"));
        assert!(p("nvl ( :x , 0 ) + 1"));
        assert!(p(
            "(SELECT COUNT(*) FROM EMP WHERE DEPT IN (1, 2) AND NAME = :D)"
        ));
        assert!(p("'pkg.f(1)' || :A"), "문자열 안은 호출이 아니다");
        assert!(
            p(":A /* pkg.f(1) */ + 1 -- g(2)"),
            "주석 안은 호출이 아니다"
        );
        assert!(p("EMP.SAL + 1"), "한정 열 이름은 호출이 아니다");
        // 사용자 루틴일 수 있는 호출 = 계산만 하는 식이 아니다.
        assert!(!p("pkg.f(:A)"));
        assert!(!p("my_func(1)"));
        assert!(!p("1 + MY_FUNC (:A)"));
        assert!(!p("dbo.fn_x(1)"));
        assert!(!p("UPPER(app.audit_and_get(:A))"));
        assert!(!p(r#""Weird Fn"(1)"#));
        assert!(!p("(SELECT x FROM table_fn(1))"));
        assert!(!p("SEQ_A.NEXTVAL"));
        assert!(!p("s.seq_a . nextval + 1"));
    }

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
