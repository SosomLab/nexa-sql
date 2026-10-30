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
    /// ★ T-367(10-17): `PRINT '글'`·`PRINT @v`·`PRINT (식)`이 **서버 문장**인가(T-SQL PRINT = SQL Server만). 거짓이면 엔진이 서버에 보내지
    ///   않고 "PRINT = 바인드 변수 표시 · 글은 PROMPT" 안내를 돌려준다(SQLite에서 syntax error · Oracle ORA-00900으로 끝나던 것).
    pub tsql_print: bool,
    /// ★ 스칼라 식 조회에 붙일 `FROM`(10-24 · D-359): `EXEC :B := :A + 10`을 `SELECT … AS "B"`로 보낼 때 FROM 없는 SELECT를
    ///   거부하는 방언(Db2 = `SYSIBM.SYSDUMMY1`)만 `Some`.
    pub scalar_from: Option<&'static str>,
    /// ★ 서버가 `OFFSET n ROWS FETCH FIRST m ROWS ONLY` 페이징을 받는가(157차 · Db2 ≤ 10.5 = 거짓 → 러너가 `ROW_NUMBER()` 폴백 ·
    ///   [108 §9 T-5](../../docs/108-dbms-syntax-and-objects-survey.md)). 세션이 서버 버전으로 정한다(Db2 = ACCRDBRM PRDID) · 그 밖 = 참.
    pub offset_fetch: bool,
    /// ★ T-149 ④(10-30): 문법·관용의 능력표 — 종전에 엔진·러너·분할기·완성기에 흩어져 있던 방언 `match` 19곳을 여기로 모았다.
    pub syntax: Syntax,
}

/// 식별자 인용 꼴(`${v:id}` · 이름 인용).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdentQuote {
    /// `"이름"`(ANSI · Oracle · PG · SQLite).
    DoubleQuote,
    /// `[이름]`(T-SQL).
    Bracket,
    /// `` `이름` ``(MySQL).
    Backtick,
}

/// 실행 계획 스크립트 틀(`explain_script` · GUI Explain · CLI `nsql explain`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExplainForm {
    /// `EXPLAIN PLAN FOR …; SELECT * FROM TABLE(DBMS_XPLAN.DISPLAY())` — Oracle.
    OraclePlanTable,
    /// `SET SHOWPLAN_TEXT ON; GO; DECLARE …; 문장; GO; SET SHOWPLAN_TEXT OFF` — SQL Server(배치여야 결과 집합이 온다).
    TsqlShowplan,
    /// `EXPLAIN (VERBOSE, COSTS) …` — PostgreSQL.
    PgVerbose,
    /// `EXPLAIN QUERY PLAN …` — SQLite.
    SqliteQueryPlan,
    /// EXPLAIN 표에 쓰고 우리 행만 읽는다 — Db2.
    Db2ExplainTables,
    /// `EXPLAIN …` 한 줄 — MySQL · ODBC · DuckDB · ClickHouse.
    Explain,
    /// 실행 계획 없음 → 문장 그대로(결과 = 데이터) — Access(Jet).
    Passthrough,
}

/// ★ T-149 ④(10-30): 방언 **문법·관용**의 능력표 — "어느 DBMS인가" 대신 "이 문법을 받는가"를 묻는다. 새 DBMS는 한 줄만 채운다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Syntax {
    /// 다중 행 INSERT = `VALUES (…), (…)` · 거짓 = `INSERT ALL … INTO …`(Oracle).
    pub multirow_values: bool,
    /// `DB.schema.table` 3부 이름을 그 DB의 카탈로그로 풀 수 있는가(SQL Server `OBJECT_ID(N'[DB].[s].[t]')`).
    pub three_part_names: bool,
    /// 세이브포인트 문장(설정 · 되돌리기 · 해제(없으면 None)) · `None` = 미지원(DuckDB 1.x · Access · ODBC 일반 · ClickHouse).
    pub savepoint: Option<(&'static str, &'static str, Option<&'static str>)>,
    /// 실행 계획 결과 집합을 고르는 열 이름(SQL Server `StmtText`) · None = 마지막 집합.
    pub plan_text_column: Option<&'static str>,
    /// 접속 뒤 "현재 DB" 전환 단위가 있는가(`USE db` · SQL Server · MySQL) → 탭 작업 단위 기본값에 DB를 쓴다.
    pub database_switch: bool,
    /// `&이름`이 미정의일 때 묻는가(SQL*Plus 계열 = Oracle · D-272) · 거짓 = 정의된 이름만 치환하고 미정의는 글자 그대로.
    pub define_prompt_undefined: bool,
    /// `@x` · `$x`도 이름 바인드인가(SQLite) → `:x`로 통일해 한 변수로.
    pub bind_marker_aliases: bool,
    /// 글자 상수를 `N'…'`로 쓰는가(SQL Server 유니코드).
    pub nchar_literal: bool,
    /// 글자 상수 안 역슬래시가 이스케이프인가(MySQL) — 상수 생성은 `\` → `\\` · 렉서는 `'it\'s'`를 한 글자로.
    pub backslash_escape: bool,
    pub ident_quote: IdentQuote,
    /// `#` 줄 주석(MySQL · Oracle `#`은 식별자 글자 · T-SQL `#temp`).
    pub hash_comment: bool,
    /// `--#SET TERMINATOR x` 지시자(Db2 CLP).
    pub clp_terminator: bool,
    /// `DELIMITER x` 지시자(MySQL · Db2).
    pub delimiter_directive: bool,
    /// 달러 인용 `$$ … $$`·`$tag$ … $tag$`를 문자열로 보는가(PostgreSQL 본문 · 다른 뜻이 없는 방언은 참) · MySQL = 거짓(`DELIMITER $$`가 구분자).
    pub dollar_quoting: bool,
    /// 문장은 늘 `;`로 끝난다(`/` 줄 블록 없음 · 본문의 `BEGIN … END`만 짝) — SQLite · MySQL · PG. 거짓 = `/`로 끝나는 블록(Oracle 등).
    pub semicolon_only: bool,
    /// `SHOW …`가 서버 문장인가(MySQL = STATUS·TABLES·CREATE TABLE …) · 거짓 = 클라이언트 명령.
    pub server_show: bool,
    /// T-SQL 관계 자리 규칙(`DELETE a FROM t a` · `UPDATE a SET … FROM`).
    pub tsql_from: bool,
    /// `표.열%TYPE` 앵커 자료형(Oracle · PG).
    pub anchored_types: bool,
    /// `USE |`가 고르는 것 = 스키마(MySQL · 스키마 = DB) · 거짓 = 데이터베이스.
    pub use_targets_schema: bool,
    /// `OUTPUT inserted.`/`deleted.` 가상 표(SQL Server).
    pub output_pseudo_tables: bool,
    pub explain: ExplainForm,
}

impl Syntax {
    /// ANSI 기준 표(ABI 코덱이 `Caps`를 풀 때 자리 채움 — 문법 표는 전송하지 않고 호스트가 방언 표로 덮는다 · ABI v2 유지).
    #[must_use]
    pub const fn ansi() -> Syntax {
        SYN_ANSI
    }
}

/// 표준 세이브포인트 3문장.
const SP_STD: Option<(&str, &str, Option<&str>)> = Some((
    "SAVEPOINT nsql_edit",
    "ROLLBACK TO SAVEPOINT nsql_edit",
    Some("RELEASE SAVEPOINT nsql_edit"),
));

/// ANSI 기준 문법(`?` 방언들의 바탕).
const SYN_ANSI: Syntax = Syntax {
    multirow_values: true,
    three_part_names: false,
    savepoint: SP_STD,
    plan_text_column: None,
    database_switch: false,
    define_prompt_undefined: false,
    bind_marker_aliases: false,
    nchar_literal: false,
    backslash_escape: false,
    ident_quote: IdentQuote::DoubleQuote,
    hash_comment: false,
    clp_terminator: false,
    delimiter_directive: false,
    dollar_quoting: true,
    semicolon_only: false,
    server_show: false,
    tsql_from: false,
    anchored_types: false,
    use_targets_schema: false,
    output_pseudo_tables: false,
    explain: ExplainForm::Explain,
};

const SYN_ORACLE: Syntax = Syntax {
    multirow_values: false,
    savepoint: Some(("SAVEPOINT nsql_edit", "ROLLBACK TO nsql_edit", None)),
    define_prompt_undefined: true,
    anchored_types: true,
    explain: ExplainForm::OraclePlanTable,
    ..SYN_ANSI
};

const SYN_MSSQL: Syntax = Syntax {
    three_part_names: true,
    savepoint: Some((
        "SAVE TRANSACTION nsql_edit",
        "ROLLBACK TRANSACTION nsql_edit",
        None,
    )),
    plan_text_column: Some("StmtText"),
    database_switch: true,
    nchar_literal: true,
    ident_quote: IdentQuote::Bracket,
    tsql_from: true,
    output_pseudo_tables: true,
    explain: ExplainForm::TsqlShowplan,
    ..SYN_ANSI
};

const SYN_POSTGRES: Syntax = Syntax {
    semicolon_only: true,
    anchored_types: true,
    explain: ExplainForm::PgVerbose,
    ..SYN_ANSI
};

const SYN_MYSQL: Syntax = Syntax {
    database_switch: true,
    backslash_escape: true,
    ident_quote: IdentQuote::Backtick,
    hash_comment: true,
    delimiter_directive: true,
    dollar_quoting: false,
    semicolon_only: true,
    server_show: true,
    use_targets_schema: true,
    ..SYN_ANSI
};

const SYN_SQLITE: Syntax = Syntax {
    bind_marker_aliases: true,
    semicolon_only: true,
    explain: ExplainForm::SqliteQueryPlan,
    ..SYN_ANSI
};

/// 세이브포인트 없는 `?` 방언(DuckDB 1.x · ODBC 일반 · ClickHouse).
const SYN_NO_SAVEPOINT: Syntax = Syntax {
    savepoint: None,
    ..SYN_ANSI
};

/// Db2(10-24): 세이브포인트 = `ON ROLLBACK RETAIN CURSORS` 필수 어절 · CLP 지시자 둘 · EXPLAIN 표.
const SYN_DB2: Syntax = Syntax {
    savepoint: Some((
        "SAVEPOINT nsql_edit ON ROLLBACK RETAIN CURSORS",
        "ROLLBACK TO SAVEPOINT nsql_edit",
        Some("RELEASE SAVEPOINT nsql_edit"),
    )),
    clp_terminator: true,
    delimiter_directive: true,
    explain: ExplainForm::Db2ExplainTables,
    ..SYN_ANSI
};

const SYN_ACCESS: Syntax = Syntax {
    savepoint: None,
    explain: ExplainForm::Passthrough,
    ..SYN_ANSI
};

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
    tsql_print: false,
    scalar_from: None,
    offset_fetch: true,
    syntax: SYN_ORACLE,
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
    // TDS BULK INSERT(89 §1-3 · B-4) · 다중 행 폴백은 sp_executesql 매개변수 둘을 뺀 2000(2100 = 실패 · 09-26).
    bulk_load: crate::bulk::BulkLoad::TdsBulk,
    tsql_print: true,
    scalar_from: None,
    offset_fetch: true,
    syntax: SYN_MSSQL,
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
    tsql_print: false,
    scalar_from: None,
    offset_fetch: true,
    syntax: SYN_POSTGRES,
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
    tsql_print: false,
    scalar_from: None,
    offset_fetch: true,
    syntax: SYN_ANSI,
};

/// MySQL/MariaDB(135차 · 드라이버 `nsql-driver-mysql`): `CALL p(?, ?)` 준비문의 OUT/INOUT 값은 **마지막 결과 집합 한 행**으로 온다
/// → 받을 자리는 서명(`information_schema.parameters`)이 정한다(PG와 같은 `Captures`).
const MYSQL: Caps = Caps {
    tx_begin: Some("START TRANSACTION"),
    server_autocommit: true,
    call_signature: CallSignature::Captures,
    bulk_load: crate::bulk::BulkLoad::LocalInfile,
    syntax: SYN_MYSQL,
    ..QUESTION
};

const SQLITE: Caps = Caps {
    tx_begin: Some("BEGIN"),
    server_autocommit: true,
    syntax: SYN_SQLITE,
    ..QUESTION
};

/// DuckDB(D-350) = SQLite처럼 `?` 바인드 · 파일/메모리 · 자동 커밋(문장마다) · 수동 커밋 = `BEGIN TRANSACTION`.
const DUCKDB: Caps = Caps {
    tx_begin: Some("BEGIN TRANSACTION"),
    server_autocommit: true,
    syntax: SYN_NO_SAVEPOINT,
    ..QUESTION
};

/// ODBC = 뒤의 DBMS를 모른다 → 트랜잭션은 드라이버의 자동 커밋 속성 몫(시작문 없음 · `COMMIT`은 늘 보낸다).
/// ★ ODBC(10-24): 드라이버 자동 커밋 ON이 기본(어댑터가 접속 때 되돌림) → 서버가 문장마다 커밋(`server_autocommit`) · 수동 모드는
///   러너의 `BEGIN TRANSACTION`을 어댑터가 가로채 `SQL_ATTR_AUTOCOMMIT=OFF`(COMMIT/ROLLBACK = SQLEndTran 뒤 ON 복귀).
const ODBC: Caps = Caps {
    tx_begin: Some("BEGIN TRANSACTION"),
    server_autocommit: true,
    syntax: SYN_NO_SAVEPOINT,
    ..QUESTION
};

/// ★ ClickHouse(10-24 · 138차): 트랜잭션 없음(문장마다 확정 = `server_autocommit` · 시작문 없음 · COMMIT/ROLLBACK = 드라이버 no-op) ·
///   `?` 자리는 드라이버가 리터럴로 치환(HTTP 인터페이스에 위치 바인드 없음) · 식별자는 대소문자 구분(Keep) · 다중 행 INSERT 적재.
const CLICKHOUSE: Caps = Caps {
    tx_begin: None,
    server_autocommit: true,
    bulk_load: crate::bulk::BulkLoad::MultiRow {
        max_rows: 1000,
        max_params: 32000,
    },
    syntax: SYN_NO_SAVEPOINT,
    ..QUESTION
};

/// ★ IBM Db2(10-24 · D-359 · ODBC 어댑터 위 전용 방언): `?` 위치 바인드 · 트랜잭션 = ODBC 자동 커밋 속성(ODBC와 같은 꼴) · 식별자 = 대문자 접힘 ·
///   스칼라 식 = `FROM SYSIBM.SYSDUMMY1` · 다중 행 INSERT.
const DB2: Caps = Caps {
    tx_begin: Some("BEGIN TRANSACTION"),
    server_autocommit: true,
    ident_fold: IdentFold::Upper,
    scalar_from: Some("SYSIBM.SYSDUMMY1"),
    syntax: SYN_DB2,
    ..QUESTION
};

/// ★ MS Access(10-24 · D-360 · ODBC 어댑터 위 Jet SQL): `?` 바인드 · 트랜잭션 = 드라이버 best-effort(ODBC와 같음) · 다중 행 VALUES 없음(한 행씩).
const ACCESS: Caps = Caps {
    tx_begin: Some("BEGIN TRANSACTION"),
    server_autocommit: true,
    bulk_load: crate::bulk::BulkLoad::MultiRow {
        max_rows: 1,
        max_params: 255,
    },
    syntax: SYN_ACCESS,
    ..QUESTION
};

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
            Dialect::Duckdb => DUCKDB,
            Dialect::Clickhouse => CLICKHOUSE,
            Dialect::Db2 => DB2,
            Dialect::Access => ACCESS,
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
        use Dialect::{
            Access, Clickhouse, Db2, Duckdb, Mssql, Mysql, Odbc, Oracle, Postgres, Sqlite,
        };
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
        // ODBC(10-24) = 어댑터가 가로채는 `BEGIN TRANSACTION`(자동 커밋 OFF 전환) · Oracle = 암묵 트랜잭션.
        assert_eq!((b(Oracle), b(Odbc)), (None, Some("BEGIN TRANSACTION")));
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
            // ODBC(10-24) = 드라이버 자동 커밋 ON → 서버가 문장마다 커밋(Oracle만 암묵 트랜잭션).
            assert_eq!(c.server_autocommit, d != Oracle, "{d:?}");
        }
        assert_eq!(Caps::of(Oracle).call_signature, CallSignature::BindTypes);
        assert_eq!(Caps::of(Postgres).call_signature, CallSignature::Captures);
        // T-149 ③ MySQL 서명 추론 = `information_schema.PARAMETERS`(PG와 같은 Captures 방식 · 10-30 단언 추가).
        assert_eq!(Caps::of(Mysql).call_signature, CallSignature::Captures);
        // ★ T-149 ④(10-30) 문법 표 — 종전 방언 `match`와 같은 답을 내는지 표본으로 고정.
        assert!(
            !Caps::of(Oracle).syntax.multirow_values && Caps::of(Postgres).syntax.multirow_values
        );
        assert!(
            Caps::of(Oracle).syntax.define_prompt_undefined
                && !Caps::of(Mssql).syntax.define_prompt_undefined
        );
        assert_eq!(Caps::of(Mssql).syntax.ident_quote, IdentQuote::Bracket);
        assert_eq!(Caps::of(Mysql).syntax.ident_quote, IdentQuote::Backtick);
        assert!(
            Caps::of(Mysql).syntax.hash_comment
                && Caps::of(Mysql).syntax.backslash_escape
                && !Caps::of(Mysql).syntax.dollar_quoting
        );
        assert!(
            Caps::of(Sqlite).syntax.bind_marker_aliases
                && !Caps::of(Postgres).syntax.bind_marker_aliases
        );
        assert!(
            Caps::of(Db2).syntax.clp_terminator
                && Caps::of(Db2).syntax.delimiter_directive
                && !Caps::of(Postgres).syntax.delimiter_directive
        );
        assert!(
            Caps::of(Duckdb).syntax.savepoint.is_none()
                && Caps::of(Access).syntax.savepoint.is_none()
                && Caps::of(Sqlite).syntax.savepoint.is_some()
        );
        assert_eq!(Caps::of(Mssql).syntax.plan_text_column, Some("StmtText"));
        assert_eq!(Caps::of(Access).syntax.explain, ExplainForm::Passthrough);
        assert_eq!(Caps::of(Clickhouse).syntax.explain, ExplainForm::Explain);
        assert!(
            Caps::of(Mssql).syntax.database_switch
                && Caps::of(Mysql).syntax.database_switch
                && !Caps::of(Oracle).syntax.database_switch
        );
        assert!(
            Caps::of(Mysql).syntax.use_targets_schema && !Caps::of(Mssql).syntax.use_targets_schema
        );
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
        // ODBC(10-24) = 드라이버 자동 커밋 ON → 열린 트랜잭션 없으면 클라이언트 COMMIT 불필요.
        assert!(!Caps::of(Dialect::Odbc).autocommit_needs_commit(false, false));
        assert!(Caps::of(Dialect::Odbc).autocommit_needs_commit(true, false));
    }
}
