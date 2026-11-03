//! ★ 형 추론(docs/127 §4 · 11-04 · T-8 가져오기 ③ · 순수 · 의존 0): 표본 셀 글에서 열마다 **가장 좁은 형**을 고르고, 방언별 DDL 형 이름과
//! 새 표 `CREATE TABLE` 문을 만든다. 좁은 → 넓은 승격 = Bool ⊂ Int ⊂ Decimal ⊂ Float · Date ⊂ DateTime · 그 밖은 Text(최대 글자 수) ·
//! 빈 셀 = NULL(형 판정에서 제외 · nullable 표시) · 선행 0이 있는 숫자(`007` · 우편번호)는 글.

use nsql_core::Dialect;

/// 추론 형.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    /// (정밀도, 소수 자리) — 표본에서 본 최대.
    Decimal(u8, u8),
    Float,
    Date,
    DateTime,
    Time,
    /// 최대 글자 수(≥ 1).
    Text(u32),
}

impl Kind {
    /// 표시 이름(미리보기 헤더 · CLI dry-run).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Kind::Bool => "BOOL".into(),
            Kind::Int => "INT".into(),
            Kind::Decimal(p, s) => format!("DECIMAL({p},{s})"),
            Kind::Float => "FLOAT".into(),
            Kind::Date => "DATE".into(),
            Kind::DateTime => "DATETIME".into(),
            Kind::Time => "TIME".into(),
            Kind::Text(n) => format!("TEXT({n})"),
        }
    }

    /// 두 형의 **공통 상위**(승격).
    #[must_use]
    pub fn join(self, other: Kind) -> Kind {
        use Kind::*;
        if self == other {
            return self;
        }
        match (self, other) {
            (Text(a), Text(b)) => Text(a.max(b)),
            (Text(n), k) | (k, Text(n)) => Text(n.max(k.min_text_len())),
            (Bool, Int) | (Int, Bool) => Int,
            (Bool, Decimal(p, s)) | (Decimal(p, s), Bool) => Decimal(p.max(1), s),
            (Int, Decimal(p, s)) | (Decimal(p, s), Int) => Decimal(p, s),
            (Decimal(p1, s1), Decimal(p2, s2)) => {
                let int1 = p1.saturating_sub(s1);
                let int2 = p2.saturating_sub(s2);
                let s = s1.max(s2);
                Decimal(int1.max(int2).saturating_add(s).clamp(1, 38), s)
            }
            (Float, Bool | Int | Decimal(..)) | (Bool | Int | Decimal(..), Float) => Float,
            (Date, DateTime) | (DateTime, Date) => DateTime,
            // 그 밖(날짜 × 숫자 · 시각 × 날짜 …) = 글.
            (a, b) => Text(a.min_text_len().max(b.min_text_len())),
        }
    }

    /// 글로 떨어질 때 필요한 최소 길이(표본 값이 그 형으로 표기될 때의 길이 상한).
    fn min_text_len(self) -> u32 {
        match self {
            Kind::Bool => 5,
            Kind::Int => 20,
            Kind::Decimal(p, s) => u32::from(p) + u32::from(s > 0) + 2,
            Kind::Float => 24,
            Kind::Date => 10,
            Kind::DateTime => 29,
            Kind::Time => 16,
            Kind::Text(n) => n,
        }
    }
}

/// 열 하나의 추론 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inferred {
    pub kind: Kind,
    /// 표본에 NULL/빈 셀이 있었다.
    pub nullable: bool,
    /// 본 표본 수(NULL 제외).
    pub seen: u64,
}

/// 열별 누적기 — 레코드를 하나씩 `feed`하고 끝에 `finish`.
#[derive(Clone, Debug)]
pub struct Inferer {
    cols: Vec<Option<Kind>>,
    nullable: Vec<bool>,
    seen: Vec<u64>,
}

impl Inferer {
    #[must_use]
    pub fn new(ncols: usize) -> Self {
        Inferer {
            cols: vec![None; ncols],
            nullable: vec![false; ncols],
            seen: vec![0; ncols],
        }
    }

    /// 레코드 하나(셀 글 · `None` = NULL · 짧으면 나머지는 NULL).
    pub fn feed(&mut self, cells: &[Option<String>]) {
        for c in 0..self.cols.len() {
            match cells.get(c).and_then(|v| v.as_deref()) {
                None | Some("") => self.nullable[c] = true,
                Some(s) => {
                    let k = classify(s);
                    self.seen[c] += 1;
                    self.cols[c] = Some(match self.cols[c] {
                        None => k,
                        Some(prev) => prev.join(k),
                    });
                }
            }
        }
    }

    /// 열마다 결과(전부 NULL인 열 = `Text(1)` · nullable).
    #[must_use]
    pub fn finish(&self) -> Vec<Inferred> {
        (0..self.cols.len())
            .map(|c| Inferred {
                kind: self.cols[c].unwrap_or(Kind::Text(1)),
                nullable: self.nullable[c] || self.seen[c] == 0,
                seen: self.seen[c],
            })
            .collect()
    }
}

/// 셀 글 하나의 형(가장 좁은 것).
#[must_use]
pub fn classify(s: &str) -> Kind {
    let t = s.trim();
    if t.is_empty() {
        return Kind::Text(1);
    }
    let chars = t.chars().count() as u32;
    if is_bool(t) {
        return Kind::Bool;
    }
    if let Some(k) = classify_number(t) {
        return k;
    }
    if is_date(t) {
        return Kind::Date;
    }
    if is_datetime(t) {
        return Kind::DateTime;
    }
    if is_time(t) {
        return Kind::Time;
    }
    Kind::Text(chars.max(1))
}

fn is_bool(t: &str) -> bool {
    matches!(
        t.to_ascii_lowercase().as_str(),
        "true" | "false" | "t" | "f" | "y" | "n" | "yes" | "no"
    )
}

/// 숫자: 정수(i64 범위 · 선행 0 금지 · `+`/`-` 허용) · 소수(`123.45` · 자리수) · 지수 표기 = Float. 천 단위 쉼표는 숫자로 보지 않는다(글).
fn classify_number(t: &str) -> Option<Kind> {
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let _ = neg;
    if body.is_empty() {
        return None;
    }
    let lower = body.to_ascii_lowercase();
    if lower.contains('e') {
        return if body
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
            && body.parse::<f64>().is_ok()
        {
            Some(Kind::Float)
        } else {
            None
        };
    }
    let (int_part, frac_part) = match body.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (body, None),
    };
    if !int_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if let Some(f) = frac_part {
        if f.is_empty() || !f.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    // 선행 0(`007` · `0123`)은 글(우편번호 · 코드) — `0` · `0.5`는 숫자.
    if int_part.len() > 1 && int_part.starts_with('0') {
        return None;
    }
    match frac_part {
        None => {
            if int_part.is_empty() {
                return None;
            }
            if t.parse::<i64>().is_ok() {
                Some(Kind::Int)
            } else {
                // i64를 넘는 정수 = Decimal(자리수, 0).
                Some(Kind::Decimal(int_part.len().min(38) as u8, 0))
            }
        }
        Some(f) => {
            let ip = if int_part.is_empty() || int_part == "0" {
                0
            } else {
                int_part.len()
            };
            let scale = f.len().min(38);
            Some(Kind::Decimal(
                ((ip + scale).clamp(1, 38)) as u8,
                scale as u8,
            ))
        }
    }
}

fn digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

/// `yyyy-MM-dd` · `yyyy/MM/dd` · `yyyy.MM.dd`(월·일 범위 검사) — 8자리 `yyyyMMdd`는 숫자와 구별이 안 돼 쓰지 않는다.
fn is_date(t: &str) -> bool {
    let sep = t.chars().nth(4);
    let Some(sep) = sep else { return false };
    if !matches!(sep, '-' | '/' | '.') {
        return false;
    }
    let parts: Vec<&str> = t.split(sep).collect();
    parts.len() == 3 && digits(parts[0], 4) && valid_md(parts[1], parts[2])
}

fn valid_md(m: &str, d: &str) -> bool {
    let ok = |s: &str, lo: u32, hi: u32| {
        (s.len() == 1 || s.len() == 2)
            && s.bytes().all(|b| b.is_ascii_digit())
            && s.parse::<u32>().is_ok_and(|v| (lo..=hi).contains(&v))
    };
    ok(m, 1, 12) && ok(d, 1, 31)
}

/// `HH:mm[:ss[.fff]]`.
fn is_time(t: &str) -> bool {
    let (hms, frac) = match t.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (t, None),
    };
    if frac.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    let p: Vec<&str> = hms.split(':').collect();
    if p.len() < 2 || p.len() > 3 {
        return false;
    }
    let ok = |s: &str, hi: u32| {
        (s.len() == 1 || s.len() == 2)
            && s.bytes().all(|b| b.is_ascii_digit())
            && s.parse::<u32>().is_ok_and(|v| v <= hi)
    };
    ok(p[0], 23) && ok(p[1], 59) && p.get(2).is_none_or(|s| ok(s, 60))
}

/// `<날짜>[T ]<시각>[Z|±hh:mm]`.
fn is_datetime(t: &str) -> bool {
    let (d, rest) = match t.split_once(['T', ' ']) {
        Some(x) => x,
        None => return false,
    };
    if !is_date(d) {
        return false;
    }
    let rest = rest.trim_end_matches('Z');
    let time = match rest.rfind(['+', '-']) {
        Some(i) if i > 0 && rest[i + 1..].len() <= 6 && rest[i + 1..].contains(':') => &rest[..i],
        _ => rest,
    };
    is_time(time)
}

/// 방언별 DDL 형 이름(127 §4 표) — `text_margin_pct` = 문자열 길이 여유(%) · 한글 CHAR 의미(Oracle = `n CHAR`).
#[must_use]
pub fn ddl_type(dialect: Dialect, kind: Kind, text_margin_pct: u32) -> String {
    use Kind::*;
    let grow = |n: u32| -> u32 {
        let g = (u64::from(n) * (100 + u64::from(text_margin_pct))).div_ceil(100);
        (g as u32).max(1)
    };
    match dialect {
        Dialect::Oracle => match kind {
            Bool => "NUMBER(1)".into(),
            Int => "NUMBER(19)".into(),
            Decimal(p, s) => format!("NUMBER({p},{s})"),
            Float => "BINARY_DOUBLE".into(),
            Date => "DATE".into(),
            DateTime => "TIMESTAMP".into(),
            Time => "VARCHAR2(16 CHAR)".into(),
            Text(n) => {
                let n = grow(n);
                if n > 4000 {
                    "CLOB".into()
                } else {
                    format!("VARCHAR2({n} CHAR)")
                }
            }
        },
        Dialect::Postgres | Dialect::Duckdb => match kind {
            Bool => "boolean".into(),
            Int => "bigint".into(),
            Decimal(p, s) => format!("numeric({p},{s})"),
            Float => "double precision".into(),
            Date => "date".into(),
            DateTime => "timestamp".into(),
            Time => "time".into(),
            Text(n) => {
                let n = grow(n);
                if n > 10_485_760 {
                    "text".into()
                } else {
                    format!("varchar({n})")
                }
            }
        },
        Dialect::Mssql => match kind {
            Bool => "bit".into(),
            Int => "bigint".into(),
            Decimal(p, s) => format!("decimal({p},{s})"),
            Float => "float".into(),
            Date => "date".into(),
            DateTime => "datetime2".into(),
            Time => "time".into(),
            Text(n) => {
                let n = grow(n);
                if n > 4000 {
                    "nvarchar(max)".into()
                } else {
                    format!("nvarchar({n})")
                }
            }
        },
        Dialect::Sqlite => match kind {
            Bool | Int => "INTEGER".into(),
            Decimal(..) => "NUMERIC".into(),
            Float => "REAL".into(),
            Date | DateTime | Time | Text(_) => "TEXT".into(),
        },
        Dialect::Mysql => match kind {
            Bool => "TINYINT(1)".into(),
            Int => "BIGINT".into(),
            Decimal(p, s) => format!("DECIMAL({p},{s})"),
            Float => "DOUBLE".into(),
            Date => "DATE".into(),
            DateTime => "DATETIME".into(),
            Time => "TIME".into(),
            Text(n) => {
                let n = grow(n);
                if n > 16_383 {
                    "TEXT".into()
                } else {
                    format!("VARCHAR({n})")
                }
            }
        },
        _ => match kind {
            Bool => "BOOLEAN".into(),
            Int => "BIGINT".into(),
            Decimal(p, s) => format!("DECIMAL({p},{s})"),
            Float => "DOUBLE".into(),
            Date => "DATE".into(),
            DateTime => "TIMESTAMP".into(),
            Time => "TIME".into(),
            Text(n) => {
                let n = grow(n);
                if n > 32_672 {
                    "CLOB".into()
                } else {
                    format!("VARCHAR({n})")
                }
            }
        },
    }
}

/// 방언별 형 후보 목록(GUI 콤보 · 사용자가 추론값을 바꿀 때) — 표시 이름 그대로 DDL에 들어간다.
#[must_use]
pub fn type_choices(dialect: Dialect) -> Vec<&'static str> {
    match dialect {
        Dialect::Oracle => vec![
            "NUMBER(1)",
            "NUMBER(19)",
            "NUMBER(38,10)",
            "BINARY_DOUBLE",
            "DATE",
            "TIMESTAMP",
            "VARCHAR2(50 CHAR)",
            "VARCHAR2(200 CHAR)",
            "VARCHAR2(4000 CHAR)",
            "CLOB",
        ],
        Dialect::Postgres | Dialect::Duckdb => vec![
            "boolean",
            "bigint",
            "numeric(38,10)",
            "double precision",
            "date",
            "timestamp",
            "time",
            "varchar(50)",
            "varchar(200)",
            "text",
        ],
        Dialect::Mssql => vec![
            "bit",
            "bigint",
            "decimal(38,10)",
            "float",
            "date",
            "datetime2",
            "time",
            "nvarchar(50)",
            "nvarchar(200)",
            "nvarchar(max)",
        ],
        Dialect::Sqlite => vec!["INTEGER", "NUMERIC", "REAL", "TEXT", "BLOB"],
        Dialect::Mysql => vec![
            "TINYINT(1)",
            "BIGINT",
            "DECIMAL(38,10)",
            "DOUBLE",
            "DATE",
            "DATETIME",
            "TIME",
            "VARCHAR(50)",
            "VARCHAR(200)",
            "TEXT",
        ],
        _ => vec![
            "BOOLEAN",
            "BIGINT",
            "DECIMAL(38,10)",
            "DOUBLE",
            "DATE",
            "TIMESTAMP",
            "TIME",
            "VARCHAR(50)",
            "VARCHAR(200)",
            "CLOB",
        ],
    }
}

/// 새 표 열 이름 정리: 공백·특수문자 → `_` · 빈 이름 → `col{i}` · 숫자로 시작하면 `c` 접두 · 중복 `_2` · 방언 예약어/대소문자 섞임은
/// 호스트가 `identq`로 인용한다(여기서는 글자만 정리).
#[must_use]
pub fn column_ident(raw: &str, index: usize) -> String {
    let mut s: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        return format!("col{}", index + 1);
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("c{s}");
    }
    s
}

/// `CREATE TABLE` 문(열 = (인용된 이름, DDL 형, nullable) · nullable 아님 = `NOT NULL`).
#[must_use]
pub fn create_table_sql(table_quoted: &str, cols: &[(String, String, bool)]) -> String {
    let mut out = format!("CREATE TABLE {table_quoted} (\n");
    for (i, (name, ty, nullable)) in cols.iter().enumerate() {
        out.push_str("  ");
        out.push_str(name);
        out.push(' ');
        out.push_str(ty);
        if !nullable {
            out.push_str(" NOT NULL");
        }
        if i + 1 < cols.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push(')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<Option<String>> {
        v.iter()
            .map(|x| {
                if x.is_empty() {
                    None
                } else {
                    Some((*x).to_string())
                }
            })
            .collect()
    }

    #[test]
    fn classify_each_kind() {
        assert_eq!(classify("true"), Kind::Bool);
        assert_eq!(classify("Y"), Kind::Bool);
        assert_eq!(classify("42"), Kind::Int);
        assert_eq!(classify("-7"), Kind::Int);
        assert_eq!(classify("007"), Kind::Text(3), "선행 0 = 글");
        assert_eq!(classify("0"), Kind::Int);
        assert_eq!(classify("123.45"), Kind::Decimal(5, 2));
        assert_eq!(classify("0.5"), Kind::Decimal(1, 1));
        assert_eq!(classify("1e5"), Kind::Float);
        assert_eq!(classify("99999999999999999999"), Kind::Decimal(20, 0));
        assert_eq!(classify("1,234"), Kind::Text(5), "천 단위 쉼표 = 글");
        assert_eq!(classify("2026-11-04"), Kind::Date);
        assert_eq!(classify("2026/1/4"), Kind::Date);
        assert_eq!(classify("2026-13-04"), Kind::Text(10));
        assert_eq!(classify("20261104"), Kind::Int, "8자리는 숫자로");
        assert_eq!(classify("2026-11-04 12:30:00"), Kind::DateTime);
        assert_eq!(classify("2026-11-04T12:30:00.123+09:00"), Kind::DateTime);
        assert_eq!(classify("12:30"), Kind::Time);
        assert_eq!(classify("25:00"), Kind::Text(5));
        assert_eq!(classify("홍길동"), Kind::Text(3));
    }

    #[test]
    fn join_promotes_narrow_to_wide() {
        assert_eq!(Kind::Bool.join(Kind::Int), Kind::Int);
        assert_eq!(Kind::Int.join(Kind::Decimal(5, 2)), Kind::Decimal(5, 2));
        assert_eq!(
            Kind::Decimal(7, 0).join(Kind::Decimal(3, 2)),
            Kind::Decimal(9, 2),
            "정수부 7 + 소수 2"
        );
        assert_eq!(Kind::Decimal(5, 2).join(Kind::Float), Kind::Float);
        assert_eq!(Kind::Date.join(Kind::DateTime), Kind::DateTime);
        assert_eq!(Kind::Date.join(Kind::Int), Kind::Text(20));
        assert_eq!(Kind::Text(3).join(Kind::Int), Kind::Text(20));
        assert_eq!(Kind::Text(30).join(Kind::Bool), Kind::Text(30));
    }

    #[test]
    fn inferer_tracks_nullable_and_all_null_column() {
        let mut inf = Inferer::new(4);
        inf.feed(&s(&["1", "Kim", "2026-01-01", ""]));
        inf.feed(&s(&["2", "", "2026-01-02 10:00", ""]));
        inf.feed(&s(&["x", "Lee"]));
        let r = inf.finish();
        assert_eq!(r[0].kind, Kind::Text(20));
        assert!(!r[0].nullable);
        assert_eq!(r[1].kind, Kind::Text(3));
        assert!(r[1].nullable);
        assert_eq!(r[2].kind, Kind::DateTime);
        assert!(r[2].nullable, "짧은 레코드의 빠진 셀 = NULL");
        assert_eq!(r[3].kind, Kind::Text(1));
        assert!(r[3].nullable);
        assert_eq!(r[3].seen, 0);
    }

    #[test]
    fn ddl_types_per_dialect_and_margin() {
        assert_eq!(
            ddl_type(Dialect::Oracle, Kind::Text(10), 50),
            "VARCHAR2(15 CHAR)"
        );
        assert_eq!(ddl_type(Dialect::Oracle, Kind::Text(4000), 50), "CLOB");
        assert_eq!(
            ddl_type(Dialect::Postgres, Kind::Decimal(9, 2), 0),
            "numeric(9,2)"
        );
        assert_eq!(
            ddl_type(Dialect::Mssql, Kind::Text(5000), 0),
            "nvarchar(max)"
        );
        assert_eq!(ddl_type(Dialect::Sqlite, Kind::DateTime, 0), "TEXT");
        assert_eq!(ddl_type(Dialect::Mysql, Kind::Bool, 0), "TINYINT(1)");
        assert_eq!(ddl_type(Dialect::Db2, Kind::Int, 0), "BIGINT");
        assert_eq!(ddl_type(Dialect::Postgres, Kind::Text(1), 50), "varchar(2)");
    }

    #[test]
    fn idents_and_create_sql() {
        assert_eq!(column_ident("Emp No.", 0), "Emp_No");
        assert_eq!(column_ident("2024 매출", 1), "c2024_매출");
        assert_eq!(column_ident("  ", 2), "col3");
        let sql = create_table_sql(
            "\"SALES\"",
            &[
                ("id".into(), "bigint".into(), false),
                ("\"name\"".into(), "varchar(20)".into(), true),
            ],
        );
        assert_eq!(
            sql,
            "CREATE TABLE \"SALES\" (\n  id bigint NOT NULL,\n  \"name\" varchar(20)\n)"
        );
        assert!(type_choices(Dialect::Oracle).contains(&"CLOB"));
    }
}
