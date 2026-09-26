//! ★ 행 식별 등급 판정(docs/87 §13 · T-231 · D-214~D-218) — **순수 함수**만. 그리드는 결과(`Tier`)를 받아 움직인다.
//!
//! 등급: 1급 = PK/UK 열이 결과에 전부 있다 → 2급-보완 = 키는 있는데 열이 빠졌다(숨은 키 열 주입 · 재조회 1회) →
//! 2급 = 키가 없고 DBMS가 물리 행 식별자를 준다(Oracle `ROWID` · SQLite `rowid` · PG `ctid`+`xmin` · 재조회 1회) →
//! 3급 = 비교 가능한 전 열(LOB·실수·긴 문자·xml/json·공간형 제외 · D-216) → 읽기 전용.
//! 재조회는 한 번만(`injected` = 이미 주입했거나 실패했다) — 다시 주입하려 들지 않고 3급/읽기 전용으로 내려간다.

use crate::gridedit_sql::ColMeta;
use nexa_ctl::gridedit::CellKind;
use nsql_core::{Dialect, KeyInfo};

/// 설정에서 오는 허용 정책(`grid.edit_hidden_keys` · `grid.edit_rowid` · `grid.edit_all_cols`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Policy {
    pub hidden_keys: bool,
    pub rowid: bool,
    pub all_cols: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            hidden_keys: true,
            rowid: true,
            all_cols: true,
        }
    }
}

/// 판정 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    /// 1급 — 키 열(결과 열 index).
    Constraint(Vec<usize>),
    /// 1급-보완 — 재조회로 가져올 키 열 이름(카탈로그 표기 · 결과에 없는 것만).
    NeedHiddenKeys(Vec<String>),
    /// 2급 — 물리 행 식별자를 주입해 재조회.
    NeedPhysical,
    /// 2급 — 주입된 물리 식별자 열(결과 열 index).
    Physical(Vec<usize>),
    /// 3급 — 비교 가능한 전 열.
    AllColumns(Vec<usize>),
    /// 식별 불가 → 읽기 전용.
    None,
}

/// DBMS가 단일 테이블 문장에 주입할 물리 행 식별자를 주는가(87 §13-3 · SQL Server `%%physloc%%`는 D-218로 미사용).
pub(crate) fn physical_supported(d: Dialect) -> bool {
    matches!(d, Dialect::Oracle | Dialect::Sqlite | Dialect::Postgres)
}

/// 전 열 비교(3급)에 쓸 수 있는 열인가(D-216) — LOB · 실수 · 긴 문자(> 4000) · xml/json · 공간형 · 이진 · 숨은 열은 제외.
/// 값은 원본 `Value` 그대로 바인드하므로 정수·문자·날짜·불·NUMERIC은 비교 가능.
pub(crate) fn comparable(d: Dialect, c: &ColMeta) -> bool {
    if c.hidden || c.spec.kind == CellKind::Binary {
        return false;
    }
    let ty = c.type_name.trim().to_ascii_uppercase();
    let head: String = ty
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == ' ' || *ch == '_')
        .collect();
    let h = head.trim();
    let has = |s: &str| h.contains(s);
    // 실수(표시 반올림 ≠ 저장값).
    if has("FLOAT") || has("REAL") || has("DOUBLE") || has("BINARY_FLOAT") || has("BINARY_DOUBLE") {
        return false;
    }
    // LOB · LONG · xml/json · 공간형 · 기타 비교 불가(방언 공통 이름).
    if has("CLOB")
        || has("BLOB")
        || h == "LONG"
        || h == "LONG RAW"
        || has("XML")
        || has("JSON") && h != "JSONB"
        || has("GEOMETRY")
        || has("GEOGRAPHY")
        || has("SDO_")
        || has("BFILE")
        || has("ANYDATA")
        || has("HIERARCHYID")
        || has("SQL_VARIANT")
        || has("IMAGE")
        || has("LONGTEXT")
        || has("MEDIUMTEXT")
    {
        return false;
    }
    // SQL Server `text`/`ntext`(비교 불가) · `varchar(max)`(긴 문자). SQLite·PG의 TEXT는 보통 문자열이라 허용.
    if d == Dialect::Mssql && (h == "TEXT" || h == "NTEXT" || ty.contains("MAX")) {
        return false;
    }
    // 긴 문자열(> 4000).
    if c.spec.kind == CellKind::Text && c.spec.max_len.is_some_and(|n| n > 4000) {
        return false;
    }
    true
}

/// 등급 판정(87 §13-1 순서). `keys` = 카탈로그 PK/UK(없으면 None) · `injected` = 이미 주입 재조회를 했다(또는 실패했다).
pub(crate) fn classify(
    d: Dialect,
    cols: &[ColMeta],
    keys: Option<&KeyInfo>,
    policy: &Policy,
    injected: bool,
) -> Tier {
    let find = |names: &[String]| -> Option<Vec<usize>> {
        if names.is_empty() {
            return None;
        }
        names
            .iter()
            .map(|k| cols.iter().position(|c| c.name.eq_ignore_ascii_case(k)))
            .collect()
    };
    if let Some(k) = keys {
        if let Some(v) = find(&k.pk) {
            return Tier::Constraint(v);
        }
        for (_, u) in &k.unique {
            if let Some(v) = find(u) {
                return Tier::Constraint(v);
            }
        }
        // 1급-보완: 키는 있는데 열이 빠졌다 → 빠진 키 열만 주입(PK 우선 · 없으면 첫 UK).
        if !injected && policy.hidden_keys {
            let want: &[String] = if !k.pk.is_empty() {
                &k.pk
            } else if let Some((_, u)) = k.unique.first() {
                u
            } else {
                &[]
            };
            let missing: Vec<String> = want
                .iter()
                .filter(|n| !cols.iter().any(|c| c.name.eq_ignore_ascii_case(n)))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Tier::NeedHiddenKeys(missing);
            }
        }
    }
    // 2급: 이미 주입된 물리 식별자 열이 있으면 그것.
    let phys: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter(|(_, c)| c.hidden && c.where_expr.is_some())
        .map(|(i, _)| i)
        .collect();
    if !phys.is_empty() {
        return Tier::Physical(phys);
    }
    if !injected && policy.rowid && physical_supported(d) {
        return Tier::NeedPhysical;
    }
    // 3급: 비교 가능한 전 열.
    if policy.all_cols {
        let v: Vec<usize> = cols
            .iter()
            .enumerate()
            .filter(|(_, c)| comparable(d, c))
            .map(|(i, _)| i)
            .collect();
        if !v.is_empty() {
            return Tier::AllColumns(v);
        }
    }
    Tier::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_ctl::gridedit::CellSpec;

    fn col(name: &str, ty: &str) -> ColMeta {
        let mut kind = CellKind::from_type_name(ty);
        if ty.is_empty() {
            kind = CellKind::Text;
        }
        let mut spec = CellSpec::new(name, kind);
        spec.max_len = CellSpec::max_len_from_type_name(ty);
        ColMeta::new(name, spec, ty)
    }

    fn keys(pk: &[&str], uk: &[&[&str]]) -> KeyInfo {
        KeyInfo {
            pk: pk.iter().map(|s| s.to_string()).collect(),
            unique: uk
                .iter()
                .enumerate()
                .map(|(i, u)| (format!("U{i}"), u.iter().map(|s| s.to_string()).collect()))
                .collect(),
        }
    }

    fn cols3() -> Vec<ColMeta> {
        vec![
            col("ID", "NUMBER"),
            col("NAME", "VARCHAR2(30)"),
            col("MEMO", "CLOB"),
        ]
    }

    // ── MC/DC: 1급 조건 = (키 있음) ∧ (열 전부 있음) — 각 조건이 독립적으로 결과를 바꾼다.
    #[test]
    fn tier1_pk_present() {
        let k = keys(&["ID"], &[]);
        assert_eq!(
            classify(
                Dialect::Oracle,
                &cols3(),
                Some(&k),
                &Policy::default(),
                false
            ),
            Tier::Constraint(vec![0])
        );
        // 대소문자 무시.
        let k = keys(&["id"], &[]);
        assert_eq!(
            classify(
                Dialect::Oracle,
                &cols3(),
                Some(&k),
                &Policy::default(),
                false
            ),
            Tier::Constraint(vec![0])
        );
    }

    #[test]
    fn tier1_unique_when_pk_missing_cols() {
        // PK 열은 빠졌고 UK 열은 있다 → UK가 1급(주입보다 먼저).
        let k = keys(&["SEQ"], &[&["NAME"]]);
        assert_eq!(
            classify(
                Dialect::Oracle,
                &cols3(),
                Some(&k),
                &Policy::default(),
                false
            ),
            Tier::Constraint(vec![1])
        );
    }

    #[test]
    fn tier1b_hidden_keys_only_missing_and_only_once() {
        let k = keys(&["ID", "SEQ"], &[]);
        // ID는 있고 SEQ만 빠졌다 → 빠진 것만.
        assert_eq!(
            classify(
                Dialect::Oracle,
                &cols3(),
                Some(&k),
                &Policy::default(),
                false
            ),
            Tier::NeedHiddenKeys(vec!["SEQ".into()])
        );
        // 이미 주입(실패)했다 → 다시 주입하지 않고 3급으로(Oracle이라도 물리 식별자 재시도 없음).
        assert_eq!(
            classify(
                Dialect::Oracle,
                &cols3(),
                Some(&k),
                &Policy::default(),
                true
            ),
            Tier::AllColumns(vec![0, 1])
        );
        // 정책 끔 → 물리 식별자로.
        let p = Policy {
            hidden_keys: false,
            ..Policy::default()
        };
        assert_eq!(
            classify(Dialect::Oracle, &cols3(), Some(&k), &p, false),
            Tier::NeedPhysical
        );
    }

    #[test]
    fn tier2_physical_by_dialect_and_policy() {
        // 키 없음 · Oracle/SQLite/PG = 주입 · SQL Server/MySQL = 3급.
        for d in [Dialect::Oracle, Dialect::Sqlite, Dialect::Postgres] {
            assert_eq!(
                classify(d, &cols3(), None, &Policy::default(), false),
                Tier::NeedPhysical,
                "{d:?}"
            );
        }
        for d in [Dialect::Mssql, Dialect::Mysql, Dialect::Odbc] {
            assert_eq!(
                classify(d, &cols3(), None, &Policy::default(), false),
                Tier::AllColumns(vec![0, 1]),
                "{d:?}"
            );
        }
        let p = Policy {
            rowid: false,
            ..Policy::default()
        };
        assert_eq!(
            classify(Dialect::Oracle, &cols3(), None, &p, false),
            Tier::AllColumns(vec![0, 1])
        );
        // 주입된 물리 열이 있으면 그것(키 없음).
        let mut cs = cols3();
        let mut rid = col("ROWID", "ROWID");
        rid.hidden = true;
        rid.where_expr = Some("ROWID".into());
        cs.push(rid);
        assert_eq!(
            classify(Dialect::Oracle, &cs, None, &Policy::default(), true),
            Tier::Physical(vec![3])
        );
        // 키가 있으면 물리 열보다 키.
        let k = keys(&["ID"], &[]);
        assert_eq!(
            classify(Dialect::Oracle, &cs, Some(&k), &Policy::default(), true),
            Tier::Constraint(vec![0])
        );
    }

    #[test]
    fn tier3_exclusions_and_none() {
        // 비교 불가 열만 남으면 읽기 전용.
        let only_lob = vec![col("MEMO", "CLOB"), col("F", "BINARY_DOUBLE")];
        assert_eq!(
            classify(Dialect::Mssql, &only_lob, None, &Policy::default(), false),
            Tier::None
        );
        let p = Policy {
            all_cols: false,
            ..Policy::default()
        };
        assert_eq!(
            classify(Dialect::Mssql, &cols3(), None, &p, false),
            Tier::None
        );
        // 제외 규칙(D-216).
        let d = Dialect::Mssql;
        assert!(!comparable(d, &col("X", "text")));
        assert!(!comparable(d, &col("X", "nvarchar(max)")));
        assert!(!comparable(d, &col("X", "xml")));
        assert!(!comparable(d, &col("X", "float")));
        assert!(!comparable(d, &col("X", "varbinary(8)")));
        assert!(comparable(d, &col("X", "nvarchar(100)")));
        assert!(comparable(d, &col("X", "int")));
        assert!(comparable(d, &col("X", "datetime2")));
        assert!(comparable(Dialect::Sqlite, &col("X", "TEXT")));
        assert!(comparable(Dialect::Postgres, &col("X", "text")));
        assert!(!comparable(Dialect::Postgres, &col("X", "json")));
        assert!(comparable(Dialect::Postgres, &col("X", "jsonb")));
        assert!(!comparable(Dialect::Postgres, &col("X", "float8")));
        assert!(!comparable(Dialect::Oracle, &col("X", "VARCHAR2(4001)")));
        assert!(comparable(Dialect::Oracle, &col("X", "VARCHAR2(4000)")));
        assert!(!comparable(Dialect::Oracle, &col("X", "SDO_GEOMETRY")));
        assert!(!comparable(Dialect::Mysql, &col("X", "longtext")));
        assert!(comparable(Dialect::Mysql, &col("X", "decimal(10,2)")));
    }
}
