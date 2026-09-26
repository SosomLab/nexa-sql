//! ★ 드라이버 능력표 `Caps`(T-152 · docs/63 §3 · docs/30 확장점 규칙).
//!
//! 스크립트 엔진과 러너는 "어느 DBMS인가"를 묻지 않고 **"무엇을 할 수 있는가"**를 묻는다. 새 DBMS 어댑터는 능력표 한 줄만 채우면
//! 엔진·러너의 분기를 고치지 않고 붙는다(MySQL/MariaDB · ODBC 뒤의 DBMS · 확장 드라이버).
//!
//! 세 조각(docs/30): **포트** = [`crate::Session::caps`](드라이버가 자기 능력을 말한다 · 기본 = 방언의 표) · **레지스트리** =
//! [`Caps::of`](내장 방언 표) · **선택** = 접속이 정한다(세션이 붙으면 러너가 그 세션의 능력표를 엔진에 넣는다).
//!
//! 값은 전부 `Copy`인 작은 표 — 문장마다 읽어도 비용이 없다(할당 0).

use crate::Dialect;

/// 바인드 자리 표기 — 엔진은 늘 `:NAME`으로 쓰인 글을 받아 이 표기로 바꿔 보낸다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marker {
    /// `:NAME` 그대로(이름 바인드 · 같은 이름은 한 번만 보낸다) — Oracle.
    Named,
    /// `@NAME`(이름 바인드) + 바인드가 불가능한 배치는 `DECLARE @v … = …;` 프리펜드 — T-SQL.
    AtName,
    /// `$1` `$2` …(자리 바인드 · 같은 이름 = 같은 번호) — PostgreSQL.
    DollarN,
    /// `?`(등장 순서대로 · 같은 이름이 두 번 나오면 두 번 보낸다) — MySQL · SQLite · ODBC.
    Question,
}

/// 호출이 돌려주는 값(OUT 인자 · `:V := 식`)이 **어떻게 돌아오는가**.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutValues {
    /// 프로토콜의 OUT 바인드(드라이버가 `ExecResult::out_params`를 채운다) — Oracle · SQL Server.
    Protocol,
    /// **1행 결과**로 온다 — 요청이 받을 이름을 명시한다(`Prepared::captures`) · PostgreSQL · MySQL · SQLite · ODBC.
    ResultRow,
}

/// 커서를 값으로 돌려받는 방식.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CursorOut {
    /// 없음.
    None,
    /// 핸들(OUT 바인드 · `REFCURSOR` 변수 · `PRINT`로 소비) — Oracle.
    Handle,
    /// **이름**(글자로 온다 · 같은 트랜잭션 안에서 `FETCH … IN "이름"`) — PostgreSQL(드라이버가 풀어 결과로 돌려준다 · T-150).
    NamedInTxn,
}

/// `EXEC 본문`을 서버 문장으로 옮기는 틀.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExecForm {
    /// `BEGIN 본문; END;`(스칼라 서브쿼리 대입은 `SELECT … INTO :V FROM DUAL`) — PL/SQL.
    PlsqlBlock,
    /// `:V := 식` → `SET @V = 식` · `SELECT … INTO :A` → `SELECT @A = …` · 그 외 `EXEC 본문` — T-SQL.
    TsqlBatch,
    /// `:V := 식` → `SELECT 식 AS "V"` · `SELECT … INTO :A` → 별칭 조회 · 그 외 `CALL 본문`.
    Call,
}

/// 호출 서명(서버 카탈로그)을 **무엇에 쓰는가**(T-151).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallSignature {
    /// 쓰지 않는다(서명 조회 0).
    None,
    /// 선언 없는 바인드의 **타입**을 정한다(REF CURSOR OUT을 글자로 바인드하면 PLS-00306) — Oracle.
    BindTypes,
    /// 어느 자리가 OUT인지로 **받을 바인드**를 정한다(1행 결과의 자리 ↔ 바인드) — PostgreSQL.
    Captures,
    /// 타입 + **`OUTPUT` 표기 보충** — T-SQL은 호출 쪽에도 `OUTPUT`을 써야 값이 돌아온다(빠뜨리면 조용히 버려진다) — SQL Server.
    OutputMarks,
}

/// 인용하지 않은 이름을 서버가 저장하는 규칙(`DESCRIBE` 등 카탈로그 조회의 이름 맞춤).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdentFold {
    Upper,
    Lower,
    Keep,
}

/// 드라이버 능력표 — 엔진·러너가 방언 대신 묻는 것들.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Caps {
    pub marker: Marker,
    pub out_values: OutValues,
    pub cursor_out: CursorOut,
    pub exec_form: ExecForm,
    /// `SELECT @A = col`이 여러 행·0행을 말없이 넘긴다 → 서버에서 `@@ROWCOUNT` 검사를 덧붙인다(D-139).
    pub into_rowcount_guard: bool,
    /// 수동 커밋에서 러너가 **먼저 보내야 하는 트랜잭션 시작문**(T-146). `None` = 암묵 트랜잭션(Oracle)이거나 모른다(ODBC).
    pub tx_begin: Option<&'static str>,
    /// 서버가 **문장마다 스스로 커밋**하는가(열린 트랜잭션이 없을 때) — 참이면 자동 커밋 모드에서 클라이언트 `COMMIT`을
    /// 보내지 않는다(T-155 · 왕복 1회 절약 · PG 경고 제거). 거짓 = 암묵 트랜잭션이거나 모른다 → 늘 보낸다.
    pub server_autocommit: bool,
    pub call_signature: CallSignature,
    /// `strict` 확인 질의 — 이 트랜잭션이 쓰기를 했는가(1/0). `None` = 분류 판정 그대로(docs/56).
    pub strict_probe: Option<&'static str>,
    /// 저장 코드 컴파일 오류를 카탈로그에서 읽을 수 있는가(`SHOW ERRORS` · Oracle `ALL_ERRORS`).
    pub compile_errors: bool,
    pub ident_fold: IdentFold,
    /// ★ 대량 적재 경로(docs/89 §3-1 · T-236).
    pub bulk_load: crate::bulk::BulkLoad,
}

const ORACLE: Caps = Caps {
    marker: Marker::Named,
    out_values: OutValues::Protocol,
    cursor_out: CursorOut::Handle,
    exec_form: ExecForm::PlsqlBlock,
    into_rowcount_guard: false,
    tx_begin: None,
    server_autocommit: false,
    call_signature: CallSignature::BindTypes,
    strict_probe: Some(
        "SELECT CASE WHEN dbms_transaction.local_transaction_id IS NULL THEN 0 ELSE 1 END FROM dual",
    ),
    compile_errors: true,
    ident_fold: IdentFold::Upper,
    bulk_load: crate::bulk::BulkLoad::ArrayDml,
};

const MSSQL: Caps = Caps {
    marker: Marker::AtName,
    out_values: OutValues::Protocol,
    cursor_out: CursorOut::None,
    exec_form: ExecForm::TsqlBatch,
    into_rowcount_guard: true,
    tx_begin: Some("BEGIN TRANSACTION"),
    server_autocommit: true,
    call_signature: CallSignature::OutputMarks,
    strict_probe: Some(
        "SELECT CASE WHEN EXISTS (SELECT 1 FROM sys.dm_tran_session_transactions st JOIN sys.dm_tran_database_transactions dt ON dt.transaction_id = st.transaction_id WHERE st.session_id = @@SPID AND dt.database_transaction_log_record_count > 0) THEN 1 ELSE 0 END",
    ),
    compile_errors: false,
    ident_fold: IdentFold::Keep,
    // sp_executesql 자체 매개변수 둘을 빼야 한다(2100 한도 · 4-DBMS 벌크 시험 09-26: 2100 = 실패 · 2000 ✓).
    bulk_load: crate::bulk::BulkLoad::MultiRow { max_rows: 1000, max_params: 2000 },
};

const POSTGRES: Caps = Caps {
    marker: Marker::DollarN,
    out_values: OutValues::ResultRow,
    cursor_out: CursorOut::NamedInTxn,
    exec_form: ExecForm::Call,
    into_rowcount_guard: false,
    tx_begin: Some("BEGIN"),
    server_autocommit: true,
    call_signature: CallSignature::Captures,
    strict_probe: Some(
        "SELECT CASE WHEN pg_current_xact_id_if_assigned() IS NULL THEN 0 ELSE 1 END",
    ),
    compile_errors: false,
    ident_fold: IdentFold::Lower,
    bulk_load: crate::bulk::BulkLoad::CopyIn,
};

/// `?` 자리 바인드 · 1행 결과로 값을 받는 DBMS의 공통 틀(MySQL · SQLite · ODBC가 여기서 갈라진다).
const QUESTION: Caps = Caps {
    marker: Marker::Question,
    out_values: OutValues::ResultRow,
    cursor_out: CursorOut::None,
    exec_form: ExecForm::Call,
    into_rowcount_guard: false,
    tx_begin: None,
    server_autocommit: false,
    call_signature: CallSignature::None,
    strict_probe: None,
    compile_errors: false,
    ident_fold: IdentFold::Keep,
    bulk_load: crate::bulk::BulkLoad::MultiRow {
        max_rows: 500,
        max_params: 32000,
    },
};

const MYSQL: Caps = Caps {
    tx_begin: Some("START TRANSACTION"),
    server_autocommit: true,
    ..QUESTION
};

const SQLITE: Caps = Caps {
    tx_begin: Some("BEGIN"),
    server_autocommit: true,
    ..QUESTION
};

/// ODBC = 뒤의 DBMS를 모른다 → 트랜잭션은 드라이버의 자동 커밋 속성 몫(시작문 없음 · `COMMIT`은 늘 보낸다).
const ODBC: Caps = QUESTION;

impl Caps {
    /// 내장 방언의 능력표(레지스트리). 확장 드라이버는 [`crate::Session::caps`]를 덮어써 자기 표를 낸다.
    #[must_use]
    pub const fn of(dialect: Dialect) -> Caps {
        match dialect {
            Dialect::Oracle => ORACLE,
            Dialect::Mssql => MSSQL,
            Dialect::Postgres => POSTGRES,
            Dialect::Mysql => MYSQL,
            Dialect::Sqlite => SQLITE,
            Dialect::Odbc => ODBC,
        }
    }

    /// 돌아온 값을 **요청이 명시한 1행 결과**로 받는가(OUT 바인드가 없다).
    #[must_use]
    pub const fn captures_rows(&self) -> bool {
        matches!(self.out_values, OutValues::ResultRow)
    }

    /// 자동 커밋 모드에서 문장 뒤에 클라이언트가 `COMMIT`을 보내야 하는가(T-155): 암묵 트랜잭션이면 늘 ·
    /// 서버가 스스로 커밋하면 열린 트랜잭션(러너가 연 것 · 사용자가 연 것)이 있을 때만.
    #[must_use]
    pub const fn autocommit_needs_commit(&self, tx_open: bool, tx_user: bool) -> bool {
        !self.server_autocommit || tx_open || tx_user
    }

    /// 인용하지 않은 이름을 서버의 저장 규칙대로(인용한 이름은 호출자가 그대로 둔다).
    #[must_use]
    pub fn fold_ident(&self, name: &str) -> String {
        match self.ident_fold {
            IdentFold::Upper => name.to_ascii_uppercase(),
            IdentFold::Lower => name.to_ascii_lowercase(),
            IdentFold::Keep => name.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 능력표 = 종전 방언 `match`와 같은 값(T-152는 동작을 바꾸지 않는다) — 방언 × 능력 전수.
    #[test]
    fn table_matches_the_former_dialect_branches() {
        use Dialect::{Mssql, Mysql, Odbc, Oracle, Postgres, Sqlite};
        let m = |d| Caps::of(d).marker;
        assert_eq!(
            [
                m(Oracle),
                m(Mssql),
                m(Postgres),
                m(Mysql),
                m(Sqlite),
                m(Odbc)
            ],
            [
                Marker::Named,
                Marker::AtName,
                Marker::DollarN,
                Marker::Question,
                Marker::Question,
                Marker::Question
            ]
        );
        let f = |d| Caps::of(d).exec_form;
        assert_eq!(
            [
                f(Oracle),
                f(Mssql),
                f(Postgres),
                f(Mysql),
                f(Sqlite),
                f(Odbc)
            ],
            [
                ExecForm::PlsqlBlock,
                ExecForm::TsqlBatch,
                ExecForm::Call,
                ExecForm::Call,
                ExecForm::Call,
                ExecForm::Call
            ]
        );
        let b = |d| Caps::of(d).tx_begin;
        assert_eq!(b(Postgres), Some("BEGIN"));
        assert_eq!(b(Sqlite), Some("BEGIN"));
        assert_eq!(b(Mssql), Some("BEGIN TRANSACTION"));
        assert_eq!(b(Mysql), Some("START TRANSACTION"));
        assert_eq!((b(Oracle), b(Odbc)), (None, None));
        for d in Dialect::ALL {
            let c = Caps::of(d);
            // OUT 바인드가 있는 것 = Oracle · SQL Server뿐(그 외 = 1행 결과를 명시해 받는다).
            assert_eq!(c.captures_rows(), !matches!(d, Oracle | Mssql), "{d:?}");
            assert_eq!(
                c.strict_probe.is_some(),
                matches!(d, Oracle | Mssql | Postgres),
                "{d:?}"
            );
            assert_eq!(c.compile_errors, d == Oracle, "{d:?}");
            assert_eq!(c.into_rowcount_guard, d == Mssql, "{d:?}");
            assert_eq!(c.cursor_out == CursorOut::Handle, d == Oracle, "{d:?}");
            assert_eq!(c.server_autocommit, !matches!(d, Oracle | Odbc), "{d:?}");
        }
        assert_eq!(Caps::of(Oracle).call_signature, CallSignature::BindTypes);
        assert_eq!(Caps::of(Postgres).call_signature, CallSignature::Captures);
        assert_eq!(Caps::of(Mssql).call_signature, CallSignature::OutputMarks);
        assert_eq!(Caps::of(Sqlite).call_signature, CallSignature::None);
        assert_eq!(Caps::of(Oracle).fold_ident("Emp"), "EMP");
        assert_eq!(Caps::of(Postgres).fold_ident("Emp"), "emp");
        assert_eq!(Caps::of(Mssql).fold_ident("Emp"), "Emp");
    }

    /// T-155 MC/DC — `!server_autocommit || tx_open || tx_user`: 조건마다 혼자서 결과를 바꾸는 쌍.
    #[test]
    fn autocommit_commit_mcdc() {
        let pg = Caps::of(Dialect::Postgres);
        let ora = Caps::of(Dialect::Oracle);
        assert!(!pg.autocommit_needs_commit(false, false));
        assert!(
            ora.autocommit_needs_commit(false, false),
            "server_autocommit 단독"
        );
        assert!(pg.autocommit_needs_commit(true, false), "tx_open 단독");
        assert!(pg.autocommit_needs_commit(false, true), "tx_user 단독");
        assert!(Caps::of(Dialect::Odbc).autocommit_needs_commit(false, false));
    }
}
