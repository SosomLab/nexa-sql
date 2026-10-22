//! ★ ABI v2 적합성(D-350): `Session`의 **모든 메서드**를 구현한 모의 드라이버를 `export_driver!`로 함수표에 꽂고(인프로세스 ·
//! `DynDriver::from_vtable`) `DynSession`으로 되불러 값이 그대로 오가는지 — Caps · 이름 · 커서(id/페이지/스트리밍) · 메시지 싱크 ·
//! 호출 타임아웃 · 벌크 싱크(push/flush/commit/rollback/finish/drop) · 카탈로그 · COPY OUT · 오류 · 패닉 격리.
use nsql_core::{
    bulk, BulkOpts, BulkSink, Caps, CatalogColumn, CatalogTable, Column, CursorHandle, CursorId,
    DbError, Dialect, ExecRequest, ExecResult, MessageSink, ResultSet, Session, Value,
};
use nsql_driver_dyn::DynDriver;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// 모의 세션 — 받은 인자를 되돌려 주거나 셈한다.
struct Mock {
    sink: Option<MessageSink>,
    timeout_ms: Option<u64>,
    cursors: Vec<(u32, usize)>,
    pushed: Arc<Mutex<Vec<Vec<Value>>>>,
}

fn rs(n: i64) -> ResultSet {
    ResultSet {
        columns: vec![Column {
            name: "n".into(),
            type_name: "INT".into(),
        }],
        rows: vec![vec![Value::Int(n)]],
    }
}

impl Session for Mock {
    fn dialect(&self) -> Dialect {
        Dialect::Postgres
    }
    fn caps(&self) -> Caps {
        let mut c = Caps::of(Dialect::Postgres);
        c.tx_begin = Some("BEGIN WORK");
        c.bulk_load = bulk::BulkLoad::CopyIn;
        c.compile_errors = !c.compile_errors;
        c
    }
    fn driver_name(&self) -> String {
        "mock-pg 9.9".into()
    }
    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        if req.sql == "boom" {
            panic!("driver bug");
        }
        if req.sql == "fail" {
            return Err(DbError {
                code: Some(42),
                message: "nope".into(),
                position: Some(3),
            });
        }
        if let Some(s) = &self.sink {
            s(format!("notice: {}", req.sql));
        }
        Ok(ExecResult {
            rows_affected: Some(req.params.len() as u64),
            result_sets: vec![rs(1)],
            ..ExecResult::default()
        })
    }
    fn fetch_cursor(&mut self, cursor: CursorId) -> Result<ResultSet, DbError> {
        Ok(rs(cursor.0 as i64))
    }
    fn fetch_cursor_page(
        &mut self,
        cursor: CursorId,
        max: usize,
    ) -> Result<(ResultSet, Option<CursorHandle>), DbError> {
        let h = 100 + cursor.0 as u32;
        self.cursors.push((h, max));
        Ok((rs(max as i64), Some(CursorHandle(h))))
    }
    fn commit(&mut self) -> Result<(), DbError> {
        Ok(())
    }
    fn rollback(&mut self) -> Result<(), DbError> {
        Ok(())
    }
    fn set_message_sink(&mut self, sink: Option<MessageSink>) {
        self.sink = sink;
    }
    fn cursor_supported(&self) -> bool {
        true
    }
    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool), DbError> {
        if !self.cursors.iter().any(|(x, _)| *x == h.0) {
            return Err(DbError {
                code: None,
                message: format!("cursor #{} is not open", h.0),
                position: None,
            });
        }
        Ok((rs(max as i64 * 10), max < 5))
    }
    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> {
        self.cursors.retain(|(x, _)| *x != h.0);
        Ok(())
    }
    fn set_call_timeout(&mut self, timeout: Option<std::time::Duration>) {
        self.timeout_ms = timeout.map(|d| d.as_millis() as u64);
    }
    fn bulk_begin<'a>(
        &'a mut self,
        table: &str,
        cols: &[String],
        _types: &[String],
        opts: &BulkOpts,
    ) -> Result<Box<dyn BulkSink + 'a>, DbError> {
        if table == "nope" {
            return Err(bulk::unsupported());
        }
        self.pushed.lock().expect("lock").clear();
        Ok(Box::new(MockSink {
            pushed: Arc::clone(&self.pushed),
            cols: cols.len(),
            batch: opts.batch_rows,
            count: 0,
        }))
    }
    fn catalog_tables(
        &mut self,
        schema: Option<&str>,
    ) -> Result<Option<Vec<CatalogTable>>, DbError> {
        Ok(schema.map(|s| {
            vec![CatalogTable {
                catalog: "db".into(),
                schema: s.into(),
                name: "T".into(),
                kind: "TABLE".into(),
            }]
        }))
    }
    fn catalog_columns(
        &mut self,
        _schema: Option<&str>,
        table: &str,
    ) -> Result<Option<Vec<CatalogColumn>>, DbError> {
        Ok(Some(vec![CatalogColumn {
            name: format!("{table}_id"),
            type_name: "INT".into(),
            size: Some(4),
            decimal_digits: None,
            nullable: Some(false),
            position: 1,
            default: String::new(),
        }]))
    }
    fn copy_out(
        &mut self,
        sql: &str,
        csv: bool,
        out: &mut dyn std::io::Write,
    ) -> Result<u64, DbError> {
        for i in 0..3 {
            let line = if csv {
                format!("{i},{sql}\n")
            } else {
                format!("{i}\t{sql}\n")
            };
            out.write_all(line.as_bytes()).map_err(|e| DbError {
                code: None,
                message: e.to_string(),
                position: None,
            })?;
        }
        Ok(3)
    }
}

struct MockSink {
    pushed: Arc<Mutex<Vec<Vec<Value>>>>,
    cols: usize,
    batch: usize,
    count: u64,
}

impl BulkSink for MockSink {
    fn push(&mut self, row: &[Value]) -> Result<(), DbError> {
        if row.len() != self.cols {
            return Err(DbError {
                code: None,
                message: "column count".into(),
                position: None,
            });
        }
        self.pushed.lock().expect("lock").push(row.to_vec());
        self.count += 1;
        Ok(())
    }
    fn flush(&mut self) -> Result<u64, DbError> {
        Ok(self.count)
    }
    fn commit(&mut self) -> Result<bool, DbError> {
        Ok(self.batch == 7)
    }
    fn rollback(&mut self) -> Result<(), DbError> {
        self.pushed.lock().expect("lock").clear();
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<u64, DbError> {
        Ok(self.count * 100)
    }
}

thread_local! {
    static PUSHED: Arc<Mutex<Vec<Vec<Value>>>> = Arc::new(Mutex::new(Vec::new()));
}

fn open_mock(_spec: &nsql_script::ConnectSpec) -> Result<Mock, DbError> {
    Ok(Mock {
        sink: None,
        timeout_ms: None,
        cursors: Vec::new(),
        pushed: PUSHED.with(Arc::clone),
    })
}

nsql_abi::export_driver!("mock", "pg", open_mock);

#[test]
fn every_session_method_round_trips_over_abi_v2() {
    let vt = nsql_abi_vtable();
    let drv = DynDriver::from_vtable(vt, Path::new("in-process")).expect("vtable");
    assert_eq!(drv.id(), "mock");
    assert_eq!(drv.dialect(), Dialect::Postgres);
    let spec = nsql_script::ConnectSpec::parse("postgres://u@h/db").expect("spec");
    let mut s = drv.open(&spec).expect("open");
    // Caps · 이름(드라이버가 말한 값).
    let c = s.caps();
    assert_eq!(c.tx_begin, Some("BEGIN WORK"));
    assert_eq!(c.bulk_load, bulk::BulkLoad::CopyIn);
    assert_ne!(c.compile_errors, Caps::of(Dialect::Postgres).compile_errors);
    assert_eq!(s.driver_name(), "ext:mock · mock-pg 9.9");
    // 메시지 싱크 → 실행 중 콜백.
    let got: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let g2 = Arc::clone(&got);
    s.set_message_sink(Some(Arc::new(move |t| g2.lock().expect("lock").push(t))));
    let r = s
        .execute(&ExecRequest {
            sql: "SELECT 1".into(),
            params: Vec::new(),
        })
        .expect("exec");
    assert_eq!(r.rows_affected, Some(0));
    assert_eq!(got.lock().expect("lock").as_slice(), ["notice: SELECT 1"]);
    s.set_message_sink(None);
    s.execute(&ExecRequest {
        sql: "SELECT 2".into(),
        params: Vec::new(),
    })
    .expect("exec");
    assert_eq!(got.lock().expect("lock").len(), 1, "싱크 해제 뒤엔 안 온다");
    // 오류 · 패닉 격리.
    let e = s
        .execute(&ExecRequest {
            sql: "fail".into(),
            params: Vec::new(),
        })
        .expect_err("fail");
    assert_eq!((e.code, e.position), (Some(42), Some(3)));
    let e = s
        .execute(&ExecRequest {
            sql: "boom".into(),
            params: Vec::new(),
        })
        .expect_err("panic");
    assert!(
        e.message.contains("panicked") && e.message.contains("driver bug"),
        "{e:?}"
    );
    // 커서.
    assert_eq!(
        s.fetch_cursor(CursorId(7)).expect("fc").rows[0][0],
        Value::Int(7)
    );
    let (page, h) = s.fetch_cursor_page(CursorId(2), 50).expect("page");
    assert_eq!(page.rows[0][0], Value::Int(50));
    assert_eq!(h, Some(CursorHandle(102)));
    assert!(s.cursor_supported());
    let (next, more) = s.fetch_next(CursorHandle(102), 3).expect("next");
    assert_eq!((next.rows[0][0].clone(), more), (Value::Int(30), true));
    s.close_cursor(CursorHandle(102)).expect("close");
    assert!(s.fetch_next(CursorHandle(102), 3).is_err(), "닫힌 커서");
    // 호출 타임아웃(드라이버가 받았는지는 다음 copy_out 등으로 간접 확인 불가 → 오류 없이 지나가면 됨).
    s.set_call_timeout(Some(std::time::Duration::from_millis(1500)));
    s.set_call_timeout(None);
    // 벌크 싱크.
    {
        let mut sink = s
            .bulk_begin(
                "T",
                &["a".into(), "b".into()],
                &["INT".into(), "TEXT".into()],
                &BulkOpts {
                    batch_rows: 7,
                    ..BulkOpts::default()
                },
            )
            .expect("bulk");
        sink.push(&[Value::Int(1), Value::Str("x".into())])
            .expect("push");
        sink.push(&[Value::Int(2), Value::Null]).expect("push");
        assert!(
            sink.push(&[Value::Int(3)]).is_err(),
            "열 수 불일치 = 드라이버 오류 그대로"
        );
        assert_eq!(sink.flush().expect("flush"), 2);
        assert!(sink.commit().expect("commit"), "batch_rows 7이 전달됐다");
        assert_eq!(sink.finish().expect("finish"), 200);
    }
    assert_eq!(PUSHED.with(|p| p.lock().expect("lock").len()), 2);
    // 싱크를 finish 없이 버리면 drop → 드라이버 쪽 싱크도 버려진다 → 다시 열 수 있다.
    {
        let mut sink = s
            .bulk_begin("T", &["a".into()], &["INT".into()], &BulkOpts::default())
            .expect("bulk2");
        sink.push(&[Value::Int(9)]).expect("push");
        sink.rollback().expect("rollback");
    }
    let mut sink = s
        .bulk_begin("T", &["a".into()], &["INT".into()], &BulkOpts::default())
        .expect("bulk3");
    assert_eq!(sink.flush().expect("flush"), 0);
    drop(sink);
    assert!(s
        .bulk_begin("nope", &[], &[], &BulkOpts::default())
        .is_err());
    // 카탈로그.
    assert_eq!(s.catalog_tables(None).expect("ct"), None);
    let t = s.catalog_tables(Some("public")).expect("ct").expect("some");
    assert_eq!((t[0].schema.as_str(), t[0].name.as_str()), ("public", "T"));
    let cols = s.catalog_columns(None, "X").expect("cc").expect("some");
    assert_eq!(cols[0].name, "X_id");
    assert_eq!(cols[0].nullable, Some(false));
    // COPY OUT.
    let mut out = Vec::new();
    assert_eq!(s.copy_out("q", true, &mut out).expect("copy"), 3);
    assert_eq!(String::from_utf8_lossy(&out), "0,q\n1,q\n2,q\n");
    struct Failing;
    impl std::io::Write for Failing {
        fn write(&mut self, _b: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let e = s
        .copy_out("q", false, &mut Failing)
        .expect_err("write fail");
    assert!(e.message.contains("disk full"), "{e:?}");
    assert!(s.is_alive());
    assert!(s.commit().is_ok() && s.rollback().is_ok());
}
