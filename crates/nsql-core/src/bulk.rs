//! ★ 대량 적재 포트(docs/89 §3-1 · T-236 · D-220 적재 = INSERT 전용) — 드라이버가 제 최속 경로(Oracle 배열 DML · PG COPY …)를
//! `BulkSink`로 내고, 러너는 능력표(`Caps.bulk_load`)를 보고 그 길 또는 다중 행 `INSERT` 폴백을 고른다.
//!
//! 규칙: 싱크는 세션을 빌려 산다(`&'a mut Session`) → 트랜잭션도 싱크가 다룬다(`commit`/`rollback`) · 배치 실패 = `Err` →
//! 러너가 싱크를 버리고 그 배치를 단건으로 다시 넣어 문제 행을 지목한다(89 §2 7).

use crate::{DbError, Value};

/// 드라이버가 제공하는 적재 경로(능력표).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BulkLoad {
    /// 드라이버 싱크 없음 → 다중 행 `INSERT … VALUES (…),(…)`(Oracle은 `INSERT ALL`) · `max_rows` 행/문장 · `max_params` 바인드 상한.
    MultiRow { max_rows: usize, max_params: usize },
    /// Oracle 배열 바인드 DML(`Connection::batch`).
    ArrayDml,
    /// PostgreSQL `COPY … FROM STDIN`(text).
    CopyIn,
    /// SQL Server TDS BULK INSERT(후속 B-4 — 지금은 MultiRow 폴백).
    TdsBulk,
}

/// 적재 옵션(설정 `bulk.*` · 89 §3-2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkOpts {
    /// 문장/배치당 행 수.
    pub batch_rows: usize,
    /// SQL Server bulk `CHECK_CONSTRAINTS`(D-221 기본 on).
    pub check_constraints: bool,
    /// SQL Server bulk `FIRE_TRIGGERS`(기본 off).
    pub fire_triggers: bool,
    /// Oracle `/*+ APPEND */`(D-224 위험 옵션 · 기본 off).
    pub append_hint: bool,
}

impl Default for BulkOpts {
    fn default() -> Self {
        BulkOpts {
            batch_rows: 1000,
            check_constraints: true,
            fire_triggers: false,
            append_hint: false,
        }
    }
}

/// 드라이버 적재 싱크 — 행을 밀어 넣고(`push`) 배치 경계에서 보내며(`flush`) 커밋 간격에 `commit` · 끝에 `finish`.
pub trait BulkSink {
    /// 행 하나(열 순서 = `bulk_begin`의 `cols`). 버퍼에만 쌓을 수 있다.
    fn push(&mut self, row: &[Value]) -> Result<(), DbError>;
    /// 쌓인 배치를 서버로(왕복). 돌려주는 값 = 이번에 보낸 행 수.
    fn flush(&mut self) -> Result<u64, DbError>;
    /// 지금까지 보낸 것을 커밋(간격 커밋 · D-223). `Ok(false)` = 이 경로는 중간 커밋이 없다(PG COPY = 문장 하나가 원자 ·
    /// `finish`에서 굳는다) → 러너는 그 행들을 아직 커밋된 것으로 세지 않는다.
    fn commit(&mut self) -> Result<bool, DbError>;
    /// 마지막 커밋 뒤 것을 되돌린다(배치 실패).
    fn rollback(&mut self) -> Result<(), DbError>;
    /// 남은 것 보내고 커밋 · 총 행 수.
    fn finish(self: Box<Self>) -> Result<u64, DbError>;
}

/// "이 세션은 드라이버 싱크가 없다"(러너가 MultiRow로 내려간다).
#[must_use]
pub fn unsupported() -> DbError {
    DbError {
        code: None,
        message: "bulk load: driver sink not supported".into(),
        position: None,
    }
}
