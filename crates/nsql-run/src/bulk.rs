//! ★ 대량 적재 러너(docs/89 §3-1 · T-236 · D-220 INSERT 전용): 원료(문자열 행) → 타입 변환 → 드라이버 싱크(`Caps.bulk_load`) 또는
//! 다중 행 `INSERT` 폴백(Oracle = `INSERT ALL`) → 배치 경계 왕복 · 커밋 간격(D-223) · 실패 배치 = 단건 재실행으로 문제 행 지목(89 §2 7).
//!
//! 데이터 보호(87 §14)와의 관계: 여기는 INSERT뿐이라 "1행 = 1문장" 규칙의 대상이 아니다. 실패 시 이미 커밋된 배치는 남는다(보고에
//! `rows`로 알린다 · 끝에 한 번 커밋하려면 `commit_every = 0`).

use crate::Runner;
use nsql_core::{
    BindParam, BulkLoad, BulkOpts, Dialect, Direction, ExecRequest, Marker, Stage, Timeline, Value,
    VarType,
};
use std::time::{Duration, Instant};

/// 적재 경로 선택.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BulkMode {
    /// 드라이버 싱크가 있으면 그것 · 없으면 다중 행.
    #[default]
    Auto,
    /// 드라이버 싱크만(없으면 오류).
    Driver,
    /// 다중 행 `INSERT`(문장당 `batch_rows` · 파라미터 상한 안).
    MultiRow,
    /// 행마다 문장 하나(진단용).
    Single,
}

impl BulkMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "auto" => BulkMode::Auto,
            "driver" => BulkMode::Driver,
            "multirow" | "multi" => BulkMode::MultiRow,
            "single" => BulkMode::Single,
            _ => return None,
        })
    }
}

/// 적재 사양.
#[derive(Clone, Debug)]
pub struct BulkParams {
    /// 대상 표(사용자 표기 그대로).
    pub table: String,
    /// 대상 열(순서 = 원료 열 순서).
    pub cols: Vec<String>,
    /// 열 타입 이름(결과 메타 · 빈 문자열 = 모름 → 글로 바인드).
    pub types: Vec<String>,
    /// 배치(문장)당 행 수.
    pub batch_rows: usize,
    /// 커밋 간격(행 · 0 = 끝에 한 번).
    pub commit_every: usize,
    pub mode: BulkMode,
    /// 빈 문자열 = NULL.
    pub empty_null: bool,
    pub opts: BulkOpts,
}

/// 실패 지목.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkFailure {
    /// 원료 행 번호(1부터 · 헤더 제외).
    pub row: u64,
    /// 원료 줄 번호(파일 · 1부터).
    pub line: u64,
    pub message: String,
}

/// 보고.
#[derive(Clone, Debug, Default)]
pub struct BulkReport {
    /// 커밋된 행 수.
    pub rows: u64,
    /// 보낸 배치 수.
    pub batches: u32,
    pub failure: Option<BulkFailure>,
    /// 쓴 경로(`driver:copy` · `multirow` · `single`).
    pub path: String,
    pub timeline: Timeline,
    pub elapsed: Duration,
}

/// 원료 한 행 = (필드들, 원료 줄 번호).
pub type SourceRow = (Vec<String>, u64);

/// 변환된 행 = (값·타입들, 원료 행 번호, 원료 줄 번호).
type Item = (Vec<(Value, VarType)>, u64, u64);

/// 원료 공급자 · 변환기(형 별칭).
type Next<'a> = &'a mut dyn FnMut() -> Result<Option<SourceRow>, String>;
type Convert<'a> = &'a mut dyn FnMut(Next<'_>) -> Result<Option<Item>, BulkFailure>;

/// 드라이버 싱크 경로의 끝.
enum SinkEnd {
    /// 세션에 싱크가 없다(폴백).
    Unsupported,
    Done,
    /// 실패한 배치의 행들(비어 있으면 읽기/커밋 오류) + 오류.
    Failed(Vec<Item>, Option<BulkFailure>),
}

/// 문자열 → 값·바인드 타입(열 타입 이름 기준 · 89 §2 6). 정수/소수는 숫자 열일 때만 · 시각은 문자열 그대로 + 타입(드라이버가 풀이).
pub fn coerce(type_name: &str, text: &str, empty_null: bool) -> (Value, VarType) {
    let up = type_name.trim().to_ascii_uppercase();
    let has = |s: &str| up.contains(s);
    if text.is_empty() && (empty_null || !(has("CHAR") || has("TEXT") || up.is_empty())) {
        return (Value::Null, VarType::Auto);
    }
    if has("INT")
        || has("NUMBER")
        || has("NUMERIC")
        || has("DECIMAL")
        || has("SERIAL")
        || has("MONEY")
    {
        let t = text.trim();
        if let Ok(i) = t.parse::<i64>() {
            return (Value::Int(i), VarType::Number);
        }
        if t.parse::<f64>().is_ok() {
            return (Value::Decimal(t.to_string()), VarType::Number);
        }
        return (Value::Str(text.to_string()), VarType::Auto);
    }
    if has("FLOAT") || has("DOUBLE") || has("REAL") {
        if let Ok(f) = text.trim().parse::<f64>() {
            return (Value::Float(f), VarType::BinaryDouble);
        }
        return (Value::Str(text.to_string()), VarType::Auto);
    }
    if has("BOOL") || up == "BIT" {
        let b = matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "t" | "y" | "yes" | "on"
        );
        return (Value::Bool(b), VarType::Boolean);
    }
    if has("TIMESTAMP") || has("DATETIME") {
        return (Value::Str(text.to_string()), VarType::Timestamp);
    }
    if up == "DATE" {
        return (Value::Str(text.to_string()), VarType::Date);
    }
    (Value::Str(text.to_string()), VarType::Auto)
}

/// 다중 행 INSERT 문장(방언 규칙 · 자리 표시는 능력표 `marker`) — `n`행. Oracle은 `INSERT ALL … SELECT 1 FROM DUAL`.
pub fn multirow_sql(
    dialect: Dialect,
    marker: Marker,
    table: &str,
    cols: &[String],
    n: usize,
) -> String {
    let q = |c: &String| nsql_catalog::quote_ident(dialect, c);
    let col_list = cols.iter().map(q).collect::<Vec<_>>().join(", ");
    // 이름은 `B{k}`(SQL Server 드라이버가 `DECLARE @이름 … = @P{n}`으로 감싸므로 `P{n}`이면 충돌 · 그리드 편집 `GE{n}`과 같은 이유).
    let ph = |k: usize| match marker {
        Marker::Named => format!(":B{k}"),
        Marker::AtName => format!("@B{k}"),
        Marker::DollarN => format!("${k}"),
        Marker::Question => "?".into(),
    };
    let mut k = 0usize;
    let mut row_tuple = |out: &mut String| {
        out.push('(');
        for i in 0..cols.len() {
            if i > 0 {
                out.push_str(", ");
            }
            k += 1;
            out.push_str(&ph(k));
        }
        out.push(')');
    };
    let mut out = String::new();
    if dialect == Dialect::Oracle {
        out.push_str("INSERT ALL");
        for _ in 0..n {
            out.push_str(&format!("\nINTO {table} ({col_list}) VALUES "));
            row_tuple(&mut out);
        }
        out.push_str("\nSELECT 1 FROM DUAL");
    } else {
        out.push_str(&format!("INSERT INTO {table} ({col_list}) VALUES "));
        for r in 0..n {
            if r > 0 {
                out.push_str(", ");
            }
            row_tuple(&mut out);
        }
    }
    out
}

/// 행 묶음 → 바인드 요청(파라미터 이름 `P1..` 순서 = 행 우선).
fn multirow_request(
    dialect: Dialect,
    marker: Marker,
    table: &str,
    cols: &[String],
    rows: &[Vec<(Value, VarType)>],
) -> ExecRequest {
    let sql = multirow_sql(dialect, marker, table, cols, rows.len());
    let mut params = Vec::with_capacity(rows.len() * cols.len());
    let mut k = 0usize;
    for row in rows {
        for (v, ty) in row {
            k += 1;
            params.push(BindParam {
                name: format!("B{k}"),
                value: v.clone(),
                ty: ty.clone(),
                direction: Direction::In,
            });
        }
    }
    ExecRequest { sql, params }
}

impl Runner {
    /// ★ 대량 적재. `next` = 원료 행(끝 = `Ok(None)` · 읽기 오류 = `Err`) · `progress(누적 커밋 행, 경과)`는 배치마다.
    pub fn bulk_load(
        &mut self,
        next: &mut dyn FnMut() -> Result<Option<SourceRow>, String>,
        p: &BulkParams,
        progress: &mut dyn FnMut(u64, Duration),
    ) -> BulkReport {
        let t0 = Instant::now();
        let mut rep = BulkReport::default();
        let dialect = self.engine.dialect;
        let caps = self.engine.caps;
        if self.session.is_none() {
            rep.failure = Some(BulkFailure {
                row: 0,
                line: 0,
                message: "no session".into(),
            });
            return rep;
        }
        let ncols = p.cols.len().max(1);
        let (max_rows, max_params) = match caps.bulk_load {
            BulkLoad::MultiRow {
                max_rows,
                max_params,
            } => (max_rows, max_params),
            _ => (p.batch_rows, usize::MAX),
        };
        let rows_per_stmt = match p.mode {
            BulkMode::Single => 1,
            _ => p
                .batch_rows
                .min(max_rows)
                .min((max_params / ncols).max(1))
                .max(1),
        };
        let driver_kind = matches!(
            caps.bulk_load,
            BulkLoad::ArrayDml | BulkLoad::CopyIn | BulkLoad::TdsBulk
        );
        let want_driver = match p.mode {
            BulkMode::Auto => driver_kind,
            BulkMode::Driver => true,
            _ => false,
        };
        // 원료 → 값 행(타입 변환) — 열 수가 다르면 오류 행.
        let mut row_no: u64 = 0;
        let mut convert = |next: &mut dyn FnMut() -> Result<Option<SourceRow>, String>|
         -> Result<Option<Item>, BulkFailure> {
            match next() {
                Ok(None) => Ok(None),
                Err(m) => Err(BulkFailure {
                    row: row_no + 1,
                    line: 0,
                    message: m,
                }),
                Ok(Some((fields, line))) => {
                    row_no += 1;
                    if fields.len() != p.cols.len() {
                        return Err(BulkFailure {
                            row: row_no,
                            line,
                            message: format!(
                                "column count {} != {}",
                                fields.len(),
                                p.cols.len()
                            ),
                        });
                    }
                    let vals = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            coerce(p.types.get(i).map_or("", String::as_str), f, p.empty_null)
                        })
                        .collect();
                    Ok(Some((vals, row_no, line)))
                }
            }
        };
        let t_send = Instant::now();
        let mut exec_total = Duration::ZERO;
        // ── 드라이버 싱크(세션 차용은 `drive_sink` 안에서 끝난다 → 실패 배치 재실행은 밖에서)
        if want_driver {
            match self.drive_sink(next, p, rows_per_stmt, &mut convert, &mut rep, progress, t0) {
                SinkEnd::Unsupported if p.mode != BulkMode::Driver => {}
                SinkEnd::Unsupported => {
                    rep.failure = Some(BulkFailure {
                        row: 0,
                        line: 0,
                        message: "bulk load: driver sink not supported".into(),
                    });
                    rep.elapsed = t0.elapsed();
                    return rep;
                }
                SinkEnd::Done => {
                    rep.elapsed = t0.elapsed();
                    return rep;
                }
                SinkEnd::Failed(rows, err) => {
                    if !rows.is_empty() {
                        let (n_ok, f) =
                            self.replay_single(&rows, p, dialect, caps.marker, caps.tx_begin);
                        rep.rows += n_ok;
                        rep.failure = f;
                    }
                    if rep.failure.is_none() {
                        rep.failure = err;
                    }
                    rep.elapsed = t0.elapsed();
                    return rep;
                }
            }
        }
        // ── 다중 행 INSERT 폴백(단건 모드 포함)
        rep.path = if rows_per_stmt == 1 {
            "single".into()
        } else {
            "multirow".into()
        };
        let mut batch: Vec<Item> = Vec::new();
        let mut since_commit: u64 = 0;
        let mut in_tx = false;
        let begin = |s: &mut Box<dyn nsql_core::Session>| -> Result<(), String> {
            match caps.tx_begin {
                Some(b) => s
                    .execute(&ExecRequest {
                        sql: b.to_string(),
                        params: Vec::new(),
                    })
                    .map(|_| ())
                    .map_err(|e| e.message),
                None => Ok(()),
            }
        };
        loop {
            let item = match convert(next) {
                Ok(v) => v,
                Err(f) => {
                    rep.failure = Some(f);
                    None
                }
            };
            let end = item.is_none();
            if let Some(it) = item {
                batch.push(it);
            }
            if rep.failure.is_some() {
                break;
            }
            let flush_now = batch.len() >= rows_per_stmt || (end && !batch.is_empty());
            if flush_now {
                let Some(session) = self.session.as_mut() else {
                    break;
                };
                if !in_tx {
                    if let Err(m) = begin(session) {
                        rep.failure = Some(BulkFailure {
                            row: batch[0].1,
                            line: batch[0].2,
                            message: m,
                        });
                        break;
                    }
                    in_tx = true;
                }
                let rows: Vec<Vec<(Value, VarType)>> =
                    batch.iter().map(|(v, _, _)| v.clone()).collect();
                let req = multirow_request(dialect, caps.marker, &p.table, &p.cols, &rows);
                let te = Instant::now();
                match session.execute(&req) {
                    Ok(_) => {
                        exec_total += te.elapsed();
                        rep.batches += 1;
                        since_commit += batch.len() as u64;
                        batch.clear();
                        if p.commit_every > 0 && since_commit >= p.commit_every as u64 {
                            if let Err(e) = session.commit() {
                                rep.failure = Some(BulkFailure {
                                    row: row_no,
                                    line: 0,
                                    message: e.message,
                                });
                                break;
                            }
                            in_tx = false;
                            rep.rows += since_commit;
                            since_commit = 0;
                            progress(rep.rows, t0.elapsed());
                        }
                    }
                    Err(e) => {
                        // 되돌리면 이 트랜잭션에서 커밋 안 된 앞 배치도 함께 잃는다(`rows`에 안 센다 · D-223 커밋 간격을 작게 하면
                        // 손실도 작다) — 실패 배치만 단건으로 다시 넣어 문제 행을 지목한다.
                        let batch_err = e.message;
                        let _ = session.rollback();
                        in_tx = false;
                        since_commit = 0;
                        let failed = std::mem::take(&mut batch);
                        let (n_ok, f) =
                            self.replay_single(&failed, p, dialect, caps.marker, caps.tx_begin);
                        rep.rows += n_ok;
                        rep.failure = f.or(Some(BulkFailure {
                            row: 0,
                            line: 0,
                            message: format!(
                                "batch failed ({batch_err}) but every row succeeded alone"
                            ),
                        }));
                        break;
                    }
                }
            }
            if end {
                break;
            }
        }
        if in_tx && rep.failure.is_none() {
            if let Some(session) = self.session.as_mut() {
                match session.commit() {
                    Ok(()) => {
                        rep.rows += since_commit;
                        progress(rep.rows, t0.elapsed());
                    }
                    Err(e) => {
                        let _ = session.rollback();
                        rep.failure = Some(BulkFailure {
                            row: row_no,
                            line: 0,
                            message: e.message,
                        });
                    }
                }
            }
        }
        rep.timeline
            .push(Stage::Send, t_send.elapsed().saturating_sub(exec_total));
        rep.timeline.push(Stage::Execute, exec_total);
        rep.elapsed = t0.elapsed();
        rep
    }

    /// 드라이버 싱크 경로 — 세션 차용을 이 안에 가둔다. 배치 실패 = 그 배치 행들을 돌려준다(밖에서 단건 재실행).
    #[allow(clippy::too_many_arguments)]
    fn drive_sink(
        &mut self,
        next: &mut dyn FnMut() -> Result<Option<SourceRow>, String>,
        p: &BulkParams,
        rows_per_stmt: usize,
        convert: Convert<'_>,
        rep: &mut BulkReport,
        progress: &mut dyn FnMut(u64, Duration),
        t0: Instant,
    ) -> SinkEnd {
        let caps = self.engine.caps;
        let Some(session) = self.session.as_mut() else {
            return SinkEnd::Unsupported;
        };
        let Ok(mut sink) = session.bulk_begin(&p.table, &p.cols, &p.types, &p.opts) else {
            return SinkEnd::Unsupported;
        };
        rep.path = format!("driver:{:?}", caps.bulk_load).to_ascii_lowercase();
        let t_send = Instant::now();
        let mut exec_total = Duration::ZERO;
        let mut batch: Vec<Item> = Vec::new();
        let mut since_commit: u64 = 0;
        let mut end = SinkEnd::Done;
        loop {
            let item = match convert(next) {
                Ok(v) => v,
                Err(f) => {
                    end = SinkEnd::Failed(Vec::new(), Some(f));
                    break;
                }
            };
            if let Some(it) = item {
                let vals_only: Vec<Value> = it.0.iter().map(|(v, _)| v.clone()).collect();
                let pushed = sink.push(&vals_only);
                batch.push(it);
                if let Err(e) = pushed {
                    end = SinkEnd::Failed(
                        std::mem::take(&mut batch),
                        Some(BulkFailure {
                            row: 0,
                            line: 0,
                            message: format!(
                                "batch failed ({}) but every row succeeded alone",
                                e.message
                            ),
                        }),
                    );
                    break;
                }
                if batch.len() < rows_per_stmt {
                    continue;
                }
            } else if batch.is_empty() {
                break;
            }
            let te = Instant::now();
            if let Err(e) = sink.flush() {
                end = SinkEnd::Failed(
                    std::mem::take(&mut batch),
                    Some(BulkFailure {
                        row: 0,
                        line: 0,
                        message: format!(
                            "batch failed ({}) but every row succeeded alone",
                            e.message
                        ),
                    }),
                );
                break;
            }
            exec_total += te.elapsed();
            rep.batches += 1;
            since_commit += batch.len() as u64;
            let was_end = batch.is_empty();
            batch.clear();
            if p.commit_every > 0 && since_commit >= p.commit_every as u64 {
                match sink.commit() {
                    Err(e) => {
                        end = SinkEnd::Failed(
                            Vec::new(),
                            Some(BulkFailure {
                                row: 0,
                                line: 0,
                                message: e.message,
                            }),
                        );
                        break;
                    }
                    Ok(true) => {
                        rep.rows += since_commit;
                        since_commit = 0;
                        progress(rep.rows, t0.elapsed());
                    }
                    // 중간 커밋이 없는 경로(COPY) — 끝에서 한 번에 센다.
                    Ok(false) => progress(rep.rows, t0.elapsed()),
                }
            }
            if was_end {
                break;
            }
        }
        if matches!(end, SinkEnd::Done) {
            match sink.finish() {
                Ok(_) => {
                    rep.rows += since_commit;
                    progress(rep.rows, t0.elapsed());
                }
                Err(e) => {
                    end = SinkEnd::Failed(
                        Vec::new(),
                        Some(BulkFailure {
                            row: 0,
                            line: 0,
                            message: e.message,
                        }),
                    )
                }
            }
        } else {
            let _ = sink.rollback();
        }
        rep.timeline
            .push(Stage::Send, t_send.elapsed().saturating_sub(exec_total));
        rep.timeline.push(Stage::Execute, exec_total);
        end
    }

    /// 실패 배치를 행마다 문장 하나로 다시 넣어 문제 행을 지목 — 앞 행은 커밋 · 문제 행에서 롤백·중단. (커밋 행 수, 실패).
    fn replay_single(
        &mut self,
        rows: &[Item],
        p: &BulkParams,
        dialect: Dialect,
        marker: Marker,
        tx_begin: Option<&'static str>,
    ) -> (u64, Option<BulkFailure>) {
        let Some(session) = self.session.as_mut() else {
            return (0, None);
        };
        if let Some(b) = tx_begin {
            let _ = session.execute(&ExecRequest {
                sql: b.to_string(),
                params: Vec::new(),
            });
        }
        let mut ok = 0u64;
        let mut failure: Option<BulkFailure> = None;
        for (vals, r, l) in rows {
            let req = multirow_request(
                dialect,
                marker,
                &p.table,
                &p.cols,
                std::slice::from_ref(vals),
            );
            match session.execute(&req) {
                Ok(_) => ok += 1,
                Err(e) => {
                    failure = Some(BulkFailure {
                        row: *r,
                        line: *l,
                        message: e.message,
                    });
                    break;
                }
            }
        }
        if let Some(f) = failure {
            // 문제 행에서 멈춘다. PG는 실패한 문장이 트랜잭션을 깨므로 되돌린 뒤 **성공했던 앞부분만 다시 넣어 커밋**(다른 DBMS도 같은
            // 길 = 결과가 방언과 무관하게 같다 · 비용은 실패 배치 하나 안에서만).
            let _ = session.rollback();
            let prefix = ok as usize;
            ok = 0;
            if prefix > 0 {
                if let Some(b) = tx_begin {
                    let _ = session.execute(&ExecRequest {
                        sql: b.to_string(),
                        params: Vec::new(),
                    });
                }
                for (vals, _, _) in &rows[..prefix] {
                    let req = multirow_request(
                        dialect,
                        marker,
                        &p.table,
                        &p.cols,
                        std::slice::from_ref(vals),
                    );
                    if session.execute(&req).is_err() {
                        let _ = session.rollback();
                        return (0, Some(f));
                    }
                }
                match session.commit() {
                    Ok(()) => ok = prefix as u64,
                    Err(_) => {
                        let _ = session.rollback();
                        ok = 0;
                    }
                }
            }
            return (ok, Some(f));
        }
        match session.commit() {
            Ok(()) => (ok, None),
            Err(e) => (
                0,
                Some(BulkFailure {
                    row: 0,
                    line: 0,
                    message: e.message,
                }),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{DbError, ExecResult, ResultSet, Session};
    use nsql_script::ConnectSpec;
    use std::sync::{Arc, Mutex};

    /// 기록 세션: 문장·파라미터 수를 남기고 `fail_on`(값) 이 든 행의 문장은 실패.
    struct Rec {
        log: Arc<Mutex<Vec<String>>>,
        fail_on: Option<String>,
        dialect: Dialect,
    }
    impl Session for Rec {
        fn dialect(&self) -> Dialect {
            self.dialect
        }
        fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
            self.log.lock().unwrap().push(format!(
                "{}#{}",
                req.sql.lines().next().unwrap_or(""),
                req.params.len()
            ));
            if let Some(bad) = &self.fail_on {
                if req
                    .params
                    .iter()
                    .any(|p| p.value == Value::Str(bad.clone()))
                {
                    return Err(DbError {
                        code: None,
                        message: format!("bad value {bad}"),
                        position: None,
                    });
                }
            }
            Ok(ExecResult::default())
        }
        fn fetch_cursor(&mut self, _c: nsql_core::CursorId) -> Result<ResultSet, DbError> {
            Ok(ResultSet::default())
        }
        fn commit(&mut self) -> Result<(), DbError> {
            self.log.lock().unwrap().push("COMMIT".into());
            Ok(())
        }
        fn rollback(&mut self) -> Result<(), DbError> {
            self.log.lock().unwrap().push("ROLLBACK".into());
            Ok(())
        }
    }

    fn runner(dialect: Dialect, fail_on: Option<&str>) -> (Runner, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let opener: crate::Opener = Box::new(|_: &ConnectSpec| {
            Err(DbError {
                code: None,
                message: "no".into(),
                position: None,
            })
        });
        let r = Runner::new(dialect, opener).with_session(
            Box::new(Rec {
                log: Arc::clone(&log),
                fail_on: fail_on.map(str::to_string),
                dialect,
            }),
            "fake",
        );
        (r, log)
    }

    fn params(_dialect: Dialect, batch: usize, commit: usize) -> BulkParams {
        BulkParams {
            table: "t".into(),
            cols: vec!["a".into(), "b".into()],
            types: vec!["INTEGER".into(), "TEXT".into()],
            batch_rows: batch,
            commit_every: commit,
            mode: BulkMode::Auto,
            empty_null: true,
            opts: BulkOpts::default(),
        }
    }

    fn source(rows: Vec<(&str, &str)>) -> impl FnMut() -> Result<Option<SourceRow>, String> {
        let owned: Vec<(String, String)> = rows
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let mut it = owned.into_iter().enumerate();
        move || Ok(it.next().map(|(i, (a, b))| (vec![a, b], i as u64 + 2)))
    }

    #[test]
    fn coerce_types() {
        assert_eq!(
            coerce("INTEGER", "42", true),
            (Value::Int(42), VarType::Number)
        );
        assert_eq!(
            coerce("NUMBER", "1.5", true),
            (Value::Decimal("1.5".into()), VarType::Number)
        );
        assert_eq!(coerce("TEXT", "", true), (Value::Null, VarType::Auto));
        assert_eq!(
            coerce("TEXT", "", false),
            (Value::Str(String::new()), VarType::Auto)
        );
        assert_eq!(coerce("INTEGER", "", false), (Value::Null, VarType::Auto));
        assert_eq!(
            coerce("DATE", "2026-09-26", true),
            (Value::Str("2026-09-26".into()), VarType::Date)
        );
        assert_eq!(
            coerce("datetime2", "2026-09-26 10:11:12", true).1,
            VarType::Timestamp
        );
        assert_eq!(
            coerce("bool", "yes", true),
            (Value::Bool(true), VarType::Boolean)
        );
        assert_eq!(
            coerce("", "x", true),
            (Value::Str("x".into()), VarType::Auto)
        );
    }

    #[test]
    fn multirow_sql_by_dialect() {
        let cols = vec!["a".into(), "b".into()];
        assert_eq!(
            multirow_sql(Dialect::Sqlite, Marker::Question, "t", &cols, 2),
            "INSERT INTO t (\"a\", \"b\") VALUES (?, ?), (?, ?)"
        );
        assert_eq!(
            multirow_sql(Dialect::Postgres, Marker::DollarN, "t", &cols, 2),
            "INSERT INTO t (\"a\", \"b\") VALUES ($1, $2), ($3, $4)"
        );
        assert_eq!(
            multirow_sql(Dialect::Mssql, Marker::AtName, "t", &cols, 1),
            "INSERT INTO t ([a], [b]) VALUES (@B1, @B2)"
        );
        assert_eq!(
            multirow_sql(Dialect::Oracle, Marker::Named, "T", &cols, 2),
            "INSERT ALL\nINTO T (\"a\", \"b\") VALUES (:B1, :B2)\nINTO T (\"a\", \"b\") VALUES (:B3, :B4)\nSELECT 1 FROM DUAL"
        );
    }

    /// 배치 3행 · 커밋 4행: 7행 → 문장 3(3+3+1) · BEGIN/COMMIT 흐름 · 전부 커밋.
    #[test]
    fn multirow_batches_and_commits() {
        let (mut r, log) = runner(Dialect::Sqlite, None);
        let nums: Vec<String> = (1..=7).map(|i| i.to_string()).collect();
        let rows: Vec<(&str, &str)> = nums.iter().map(|n| (n.as_str(), "x")).collect();
        let mut src = source(rows);
        let mut prog = Vec::new();
        let rep = r.bulk_load(&mut src, &params(Dialect::Sqlite, 3, 4), &mut |n, _| {
            prog.push(n)
        });
        assert!(rep.failure.is_none(), "{:?}", rep.failure);
        assert_eq!(rep.rows, 7);
        assert_eq!(rep.batches, 3);
        assert_eq!(rep.path, "multirow");
        let l = log.lock().unwrap().clone();
        // BEGIN · 3행(6 params) · 3행 → 커밋(6 ≥ 4) · BEGIN · 1행 → 끝 커밋.
        assert_eq!(
            l,
            vec![
                "BEGIN#0",
                "INSERT INTO t (\"a\", \"b\") VALUES (?, ?), (?, ?), (?, ?)#6",
                "INSERT INTO t (\"a\", \"b\") VALUES (?, ?), (?, ?), (?, ?)#6",
                "COMMIT",
                "BEGIN#0",
                "INSERT INTO t (\"a\", \"b\") VALUES (?, ?)#2",
                "COMMIT",
            ]
        );
        assert_eq!(prog, vec![6, 7]);
    }

    /// 실패 배치 = 롤백 → 단건 재실행으로 문제 행 지목(앞 행은 커밋) · 원료 줄 번호 보고.
    #[test]
    fn failed_batch_is_replayed_to_pinpoint_row() {
        let (mut r, log) = runner(Dialect::Postgres, Some("bad"));
        let mut src = source(vec![("1", "x"), ("2", "bad"), ("3", "y")]);
        let rep = r.bulk_load(&mut src, &params(Dialect::Postgres, 10, 0), &mut |_, _| {});
        let f = rep.failure.expect("failure");
        assert_eq!(f.row, 2);
        assert_eq!(f.line, 3, "헤더 뒤 두 번째 원료 행 = 3번째 줄");
        assert!(f.message.contains("bad value"));
        assert_eq!(rep.rows, 1, "문제 행 앞의 1행은 다시 넣어 커밋");
        let l = log.lock().unwrap().clone();
        assert!(
            l.iter()
                .any(|s| s.starts_with("INSERT INTO t") && s.ends_with("#6")),
            "배치 3행"
        );
        assert!(l.contains(&"ROLLBACK".to_string()));
        let singles = l.iter().filter(|s| s.ends_with("#2")).count();
        assert_eq!(
            singles, 3,
            "단건 재실행 1행 ✓ · 2행 ✗ → 되돌린 뒤 앞부분 1행 다시 → 커밋"
        );
    }

    /// 열 수 불일치 = 읽기 오류 행 지목 · 단건 모드 = 행마다 문장.
    #[test]
    fn column_count_mismatch_and_single_mode() {
        let (mut r, _) = runner(Dialect::Sqlite, None);
        let mut it = vec![
            Ok(Some((vec!["1".to_string(), "x".to_string()], 2u64))),
            Ok(Some((vec!["2".to_string()], 3u64))),
        ]
        .into_iter();
        let mut src = move || it.next().unwrap_or(Ok(None));
        let rep = r.bulk_load(&mut src, &params(Dialect::Sqlite, 10, 0), &mut |_, _| {});
        let f = rep.failure.expect("failure");
        assert_eq!((f.row, f.line), (2, 3));
        assert!(f.message.contains("column count"));
        let (mut r2, log2) = runner(Dialect::Sqlite, None);
        let mut src2 = source(vec![("1", "a"), ("2", "b")]);
        let mut p = params(Dialect::Sqlite, 10, 0);
        p.mode = BulkMode::Single;
        let rep2 = r2.bulk_load(&mut src2, &p, &mut |_, _| {});
        assert_eq!(rep2.rows, 2);
        assert_eq!(rep2.path, "single");
        assert_eq!(
            log2.lock()
                .unwrap()
                .iter()
                .filter(|s| s.ends_with("#2"))
                .count(),
            2
        );
    }
}
