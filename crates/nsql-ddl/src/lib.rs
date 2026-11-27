//! 테이블 구조 편집의 핵심(docs/119 · 의존 0 · 순수) — 모델 [`Table`] → 차이 [`diff`] → 방언별 문장 계획 [`plan`].
//!
//! 흐름 = 원본 모델(서버에서 읽은 것)과 편집 모델(화면이 고친 것)을 비교해 [`Change`] 목록을 만들고, 방언 규칙에 따라
//! 실행 순서가 정해진 [`Stmt`] 목록으로 푼다. 방언이 못 하는 변경(SQLite의 타입 변경 · MySQL 밖의 컬럼 순서)은 문장을 만들지
//! 않고 [`Blocked`]로 돌려준다 — **테이블 재생성(drop-recreate)은 하지 않는다**(D-318 · 데이터 보호 불변식의 DDL 판).
//! 편집 모델의 각 항목은 `origin`(원본에서의 이름)으로 원본과 이어진다: `None` = 새 항목 · 이름이 달라졌으면 이름 바꾸기.

mod render;
#[cfg(test)]
mod tests;

pub use render::{create_table, fold_new_name, ident, qualified, script, type_names};

/// 이 크레이트가 문장을 만들 수 있는 방언(그 밖 = 호스트가 편집기를 읽기 전용으로 연다).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Oracle,
    Postgres,
    Mssql,
    Mysql,
    Sqlite,
}

/// 컬럼 하나. `data_type` = 방언 글 그대로(`VARCHAR2(20)` · `numeric(10,2)`). `default` = 식(빈 값 = 없음).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: String,
    pub comment: String,
    /// 원본에서의 이름(`None` = 새 컬럼).
    pub origin: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyKind {
    #[default]
    Primary,
    Unique,
    Foreign,
    Check,
}

/// 제약 하나. 외래 키 = `ref_table`(스키마 포함 가능 · `스키마.이름`)·`ref_cols`·`on_delete`(빈 값 = 기본) · 체크 = `check` 식.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub kind: KeyKind,
    pub cols: Vec<String>,
    pub ref_table: String,
    pub ref_cols: Vec<String>,
    pub on_delete: String,
    /// 외래 키의 갱신 규칙(빈 값 = 기본 · Oracle에는 없다).
    pub on_update: String,
    pub check: String,
    pub origin: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub name: String,
    pub unique: bool,
    pub cols: Vec<String>,
    pub origin: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub comment: String,
    pub columns: Vec<Column>,
    pub keys: Vec<Key>,
    pub indexes: Vec<Index>,
}

impl Table {
    /// 서버에서 읽은 모델을 편집 시작점으로: 모든 항목의 `origin`을 자기 이름으로 채운 사본.
    #[must_use]
    pub fn editable(&self) -> Table {
        let mut t = self.clone();
        for c in &mut t.columns {
            c.origin = Some(c.name.clone());
        }
        for k in &mut t.keys {
            k.origin = Some(k.name.clone());
        }
        for i in &mut t.indexes {
            i.origin = Some(i.name.clone());
        }
        t
    }
}

/// 변경 하나. 컬럼 변경은 **편집 뒤 컬럼 전체**(`col`)와 원본 이름(`old_name` · 위험 검사는 적용 전 이름으로 묻는다)을 함께 든다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    RenameTable {
        to: String,
    },
    /// `at_end` = 맨 끝에 더한다(자리 지정이 필요 없다).
    AddColumn {
        col: Column,
        after: Option<String>,
        at_end: bool,
    },
    DropColumn {
        name: String,
        had_default: bool,
    },
    RenameColumn {
        from: String,
        to: String,
    },
    AlterType {
        col: Column,
        old_name: String,
        from: String,
    },
    AlterNull {
        col: Column,
        old_name: String,
    },
    AlterDefault {
        col: Column,
        old_name: String,
        from: String,
    },
    MoveColumn {
        col: Column,
        after: Option<String>,
    },
    ColumnComment {
        col: Column,
        from: String,
    },
    TableComment {
        to: String,
        from: String,
    },
    AddKey(Key),
    DropKey(Key),
    RenameKey {
        key: Key,
        from: String,
    },
    AddIndex(Index),
    DropIndex(Index),
    RenameIndex {
        index: Index,
        from: String,
    },
}

/// 위험 등급 — 화면의 경고 띠와 2단 확인이 쓴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// 데이터에 닿지 않는다.
    Safe,
    /// 기존 데이터 때문에 실패하거나 오래 걸릴 수 있다(NOT NULL 추가 · 타입 변경 · 제약 추가).
    Warn,
    /// 데이터가 사라진다(컬럼 삭제 · 타입 축소).
    Danger,
}

/// 실행할 문장 하나(끝의 `;` 없음 · 한 번에 하나씩 실행).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stmt {
    pub sql: String,
    pub risk: Risk,
}

/// 방언이 못 하는 까닭.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// 테이블을 다시 만들어야 한다(이 프로그램은 재생성하지 않는다).
    NeedsRebuild,
    /// 그 방언에 없는 기능(SQLite 코멘트).
    NotSupported,
}

/// 문장으로 풀지 못한 변경(`what` = 대상 이름 · 화면이 사유와 함께 보인다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked {
    pub what: String,
    pub reason: Reason,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub stmts: Vec<Stmt>,
    pub blocked: Vec<Blocked>,
}

impl Plan {
    /// 가장 높은 위험 등급(문장이 없으면 Safe).
    #[must_use]
    pub fn risk(&self) -> Risk {
        self.stmts
            .iter()
            .map(|s| s.risk)
            .max()
            .unwrap_or(Risk::Safe)
    }
}

/// 적용 전 위험 검사 질의의 종류(결과 = 행 수 하나 · 0이 아니면 경고).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckKind {
    /// NOT NULL로 바꾸려는 컬럼의 NULL 행 수.
    NullRows,
    /// 지우려는 컬럼에 값이 있는 행 수.
    ValueRows,
    /// 기본값 없이 NOT NULL 컬럼을 더하려는 테이블의 행 수.
    TableRows,
    /// PK/유니크로 묶으려는 컬럼 조합의 중복 묶음 수.
    DuplicateKeys,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskCheck {
    pub kind: CheckKind,
    /// 대상(컬럼 이름 또는 제약 이름).
    pub target: String,
    pub sql: String,
}

/// 타입 글 비교용 정규화(대소문자 · 공백 무시 · **같은 타입의 다른 이름**은 하나로): 서버는 정규 이름(`character varying`)으로
/// 돌려주고 사람은 줄임(`varchar`)으로 친다 — 글자만 견주면 적용이 끝난 컬럼이 "바뀜"으로 남는다(11-27 PostgreSQL 실서버).
#[must_use]
pub fn norm_type(t: &str) -> String {
    let n: String = t
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    let (base, rest) = match n.find('(') {
        Some(i) => n.split_at(i),
        None => (n.as_str(), ""),
    };
    let base = match base {
        "VARCHAR" => "CHARACTERVARYING",
        "CHAR" | "BPCHAR" => "CHARACTER",
        "INT" | "INT4" => "INTEGER",
        "INT8" => "BIGINT",
        "INT2" => "SMALLINT",
        "BOOL" => "BOOLEAN",
        "DECIMAL" | "DEC" => "NUMERIC",
        "FLOAT8" => "DOUBLEPRECISION",
        "FLOAT4" => "REAL",
        "TIMESTAMPTZ" => "TIMESTAMPWITHTIMEZONE",
        "TIMESTAMPWITHOUTTIMEZONE" => "TIMESTAMP",
        "TIMETZ" => "TIMEWITHTIMEZONE",
        "TIMEWITHOUTTIMEZONE" => "TIME",
        other => other,
    };
    format!("{base}{rest}")
}

/// `이름(숫자[,숫자])` 꼴에서 (이름, 첫 숫자) — 길이 축소 판정용.
fn type_len(t: &str) -> (String, Option<u64>) {
    let n = norm_type(t);
    match n.split_once('(') {
        Some((base, rest)) => {
            let first: String = rest.chars().take_while(char::is_ascii_digit).collect();
            (base.to_string(), first.parse().ok())
        }
        None => (n, None),
    }
}

/// 타입 변경의 위험: 같은 타입에서 길이가 줄면 Danger · 그 밖의 변경은 Warn.
#[must_use]
pub fn type_change_risk(from: &str, to: &str) -> Risk {
    let (fb, fl) = type_len(from);
    let (tb, tl) = type_len(to);
    match (fl, tl) {
        (Some(a), Some(b)) if fb == tb && b < a => Risk::Danger,
        _ => Risk::Warn,
    }
}

/// 가장 긴 증가 부분열의 원소 자리(순서가 유지된 컬럼 = 옮길 필요 없는 컬럼).
fn lis_keep(seq: &[usize]) -> Vec<bool> {
    let n = seq.len();
    let mut len = vec![1usize; n];
    let mut prev = vec![usize::MAX; n];
    for i in 0..n {
        for j in 0..i {
            if seq[j] < seq[i] && len[j] + 1 > len[i] {
                len[i] = len[j] + 1;
                prev[i] = j;
            }
        }
    }
    let mut keep = vec![false; n];
    if let Some(mut i) = (0..n).max_by_key(|&i| (len[i], std::cmp::Reverse(i))) {
        loop {
            keep[i] = true;
            if prev[i] == usize::MAX {
                break;
            }
            i = prev[i];
        }
    }
    keep
}

/// 원본 ↔ 편집 모델의 차이. 순서는 뜻이 없다(실행 순서는 [`plan`]이 정한다).
#[must_use]
pub fn diff(orig: &Table, edit: &Table) -> Vec<Change> {
    let mut out = Vec::new();
    // ── 컬럼
    let ocol = |name: &str| orig.columns.iter().position(|c| c.name == name);
    for o in &orig.columns {
        if !edit
            .columns
            .iter()
            .any(|c| c.origin.as_deref() == Some(o.name.as_str()))
        {
            out.push(Change::DropColumn {
                name: o.name.clone(),
                had_default: !o.default.trim().is_empty(),
            });
        }
    }
    let mut survivors: Vec<(usize, usize)> = Vec::new(); // (편집 자리, 원본 자리)
    for (ei, c) in edit.columns.iter().enumerate() {
        let after = ei.checked_sub(1).map(|p| edit.columns[p].name.clone());
        let Some(oi) = c.origin.as_deref().and_then(ocol) else {
            out.push(Change::AddColumn {
                col: c.clone(),
                after,
                at_end: ei + 1 == edit.columns.len(),
            });
            continue;
        };
        survivors.push((ei, oi));
        let o = &orig.columns[oi];
        if o.name != c.name {
            out.push(Change::RenameColumn {
                from: o.name.clone(),
                to: c.name.clone(),
            });
        }
        if norm_type(&o.data_type) != norm_type(&c.data_type) {
            out.push(Change::AlterType {
                col: c.clone(),
                old_name: o.name.clone(),
                from: o.data_type.clone(),
            });
        }
        if o.nullable != c.nullable {
            out.push(Change::AlterNull {
                col: c.clone(),
                old_name: o.name.clone(),
            });
        }
        if o.default.trim() != c.default.trim() {
            out.push(Change::AlterDefault {
                col: c.clone(),
                old_name: o.name.clone(),
                from: o.default.clone(),
            });
        }
        if o.comment != c.comment {
            out.push(Change::ColumnComment {
                col: c.clone(),
                from: o.comment.clone(),
            });
        }
    }
    // 순서: 남은 컬럼의 원본 자리 수열에서 증가 부분열 밖의 것만 옮긴다.
    let seq: Vec<usize> = survivors.iter().map(|&(_, oi)| oi).collect();
    for (k, keep) in lis_keep(&seq).into_iter().enumerate() {
        if !keep {
            let ei = survivors[k].0;
            out.push(Change::MoveColumn {
                col: edit.columns[ei].clone(),
                after: ei.checked_sub(1).map(|p| edit.columns[p].name.clone()),
            });
        }
    }
    // ── 제약
    for o in &orig.keys {
        if !edit
            .keys
            .iter()
            .any(|k| k.origin.as_deref() == Some(o.name.as_str()))
        {
            out.push(Change::DropKey(o.clone()));
        }
    }
    for k in &edit.keys {
        let o = k
            .origin
            .as_deref()
            .and_then(|n| orig.keys.iter().find(|o| o.name == n));
        match o {
            None => out.push(Change::AddKey(k.clone())),
            Some(o) => {
                // 원본의 컬럼 이름을 편집 뒤 이름으로 옮겨 비교한다(컬럼 이름만 바뀐 제약은 그대로다).
                let same = o.kind == k.kind
                    && renamed(&o.cols, orig, edit) == k.cols
                    && o.ref_table == k.ref_table
                    && o.ref_cols == k.ref_cols
                    && o.on_delete.eq_ignore_ascii_case(&k.on_delete)
                    && o.on_update.eq_ignore_ascii_case(&k.on_update)
                    && o.check.trim() == k.check.trim();
                if !same {
                    out.push(Change::DropKey(o.clone()));
                    out.push(Change::AddKey(k.clone()));
                } else if o.name != k.name {
                    out.push(Change::RenameKey {
                        key: k.clone(),
                        from: o.name.clone(),
                    });
                }
            }
        }
    }
    // ── 인덱스
    for o in &orig.indexes {
        if !edit
            .indexes
            .iter()
            .any(|i| i.origin.as_deref() == Some(o.name.as_str()))
        {
            out.push(Change::DropIndex(o.clone()));
        }
    }
    for i in &edit.indexes {
        let o = i
            .origin
            .as_deref()
            .and_then(|n| orig.indexes.iter().find(|o| o.name == n));
        match o {
            None => out.push(Change::AddIndex(i.clone())),
            Some(o) => {
                if o.unique != i.unique || renamed(&o.cols, orig, edit) != i.cols {
                    out.push(Change::DropIndex(o.clone()));
                    out.push(Change::AddIndex(i.clone()));
                } else if o.name != i.name {
                    out.push(Change::RenameIndex {
                        index: i.clone(),
                        from: o.name.clone(),
                    });
                }
            }
        }
    }
    if orig.comment != edit.comment {
        out.push(Change::TableComment {
            to: edit.comment.clone(),
            from: orig.comment.clone(),
        });
    }
    if orig.name != edit.name {
        out.push(Change::RenameTable {
            to: edit.name.clone(),
        });
    }
    out
}

/// 원본 컬럼 이름 목록을 편집 뒤 이름으로(지워진 컬럼은 원래 이름 그대로 남긴다 = 달라진 것으로 판정된다).
fn renamed(cols: &[String], orig: &Table, edit: &Table) -> Vec<String> {
    let _ = orig;
    cols.iter()
        .map(|c| {
            edit.columns
                .iter()
                .find(|e| e.origin.as_deref() == Some(c.as_str()))
                .map_or_else(|| c.clone(), |e| e.name.clone())
        })
        .collect()
}

/// 변경 목록 → 실행 순서가 정해진 문장 계획. `orig` = 적용 전 테이블(스키마·이름 · 원본 코멘트 유무 판정).
///
/// 순서 = 제약·인덱스 삭제(외래 키 먼저) → 컬럼 이름 → 컬럼 추가 → 타입·NULL·기본값 → 컬럼 삭제 → 순서 → 제약 이름·추가(외래 키
/// 마지막) → 인덱스 → 코멘트 → 테이블 이름. 이름 바꾸기 뒤의 문장은 새 컬럼 이름을 쓴다.
#[must_use]
pub fn plan(d: Dialect, orig: &Table, changes: &[Change]) -> Plan {
    render::plan(d, orig, changes)
}

/// 적용 전에 물어볼 위험 검사 질의(전부 `SELECT COUNT(*)` 한 행 · 적용 전 이름 기준).
#[must_use]
pub fn risk_checks(d: Dialect, orig: &Table, changes: &[Change]) -> Vec<RiskCheck> {
    let t = qualified(d, &orig.schema, &orig.name);
    let mut out = Vec::new();
    for c in changes {
        match c {
            Change::AlterNull { col, old_name } if !col.nullable => out.push(RiskCheck {
                kind: CheckKind::NullRows,
                target: old_name.clone(),
                sql: format!(
                    "SELECT COUNT(*) FROM {t} WHERE {} IS NULL",
                    ident(d, old_name)
                ),
            }),
            Change::DropColumn { name, .. } => out.push(RiskCheck {
                kind: CheckKind::ValueRows,
                target: name.clone(),
                sql: format!(
                    "SELECT COUNT(*) FROM {t} WHERE {} IS NOT NULL",
                    ident(d, name)
                ),
            }),
            Change::AddColumn { col, .. } if !col.nullable && col.default.trim().is_empty() => {
                out.push(RiskCheck {
                    kind: CheckKind::TableRows,
                    target: col.name.clone(),
                    sql: format!("SELECT COUNT(*) FROM {t}"),
                });
            }
            Change::AddKey(k) if matches!(k.kind, KeyKind::Primary | KeyKind::Unique) => {
                // 적용 전 이름으로: 이 계획에서 이름이 바뀌는 컬럼은 옛 이름으로 되돌려 묻는다 · 새 컬럼이 끼면 물을 수 없다.
                let mut cols = Vec::new();
                for kc in &k.cols {
                    let old = changes.iter().find_map(|x| match x {
                        Change::RenameColumn { from, to } if to == kc => Some(from.clone()),
                        _ => None,
                    });
                    let name = old.unwrap_or_else(|| kc.clone());
                    if orig.columns.iter().any(|o| o.name == name) {
                        cols.push(ident(d, &name));
                    }
                }
                if cols.len() == k.cols.len() && !cols.is_empty() {
                    let list = cols.join(", ");
                    out.push(RiskCheck {
                        kind: CheckKind::DuplicateKeys,
                        target: k.name.clone(),
                        sql: format!("SELECT COUNT(*) FROM (SELECT {list} FROM {t} GROUP BY {list} HAVING COUNT(*) > 1) nsql_dup"),
                    });
                }
            }
            _ => {}
        }
    }
    out
}
