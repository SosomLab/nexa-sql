# 89. 대량 I/O(벌크 적재·추출) 속도 향상 기술 검토 — DBMS별 지원 방식·지원 여부·기술 방식(사용자 09-26)

> 목적: 결과 추출(export)·대량 입력(import/bulk)·그리드 편집 적용·스크립트의 반복 DML을 **어떤 경로로 보내면 얼마나 빨라지는가**를 DBMS·드라이버(우리가 쓰는 Rust crate) 기준으로 정리하고, nexa-sql에 넣을 포트·설정·단계를 제안한다. 수치는 벤더 문서·공개 벤치의 **대략치**(⚠ = 실측 전)이며 실측은 [71](71-performance-review-process.md) C 시나리오로 한다.
> 관련: 데이터 척추 `Export` 싱크 [77 §4](77-data-workbench-architecture.md) · 페치 모델 [43](43-fetch-model-and-result-tabs.md) · 결과 한 세트 DR-33 · 능력표 `Caps` [61 §1-6](61-core-design-and-working-rules.md) · 네트워크 원장 [26 §8](26-performance-architecture.md).

## 0. 한 줄 결론

| 방향 | 가장 빠른 길(DBMS별) | 우리 드라이버 crate에서 지금 되는가 | 기대 배수(단건 INSERT/한 줄 fetch 대비 · ⚠) |
|---|---|---|---|
| **입력(적재)** | Oracle **배열 바인드 DML**(OCI array · `APPEND`/`NOLOGGING`은 옵션) · PostgreSQL **`COPY … FROM STDIN`(binary)** · SQL Server **TDS BULK INSERT(bcp 프로토콜)** · SQLite **한 트랜잭션 + 준비문 재사용** · MySQL **`LOAD DATA LOCAL INFILE`**(없으면 다중 행 VALUES) | oracle 0.6 `Batch` ✅ · postgres 0.19 `copy_in` ✅ · tiberius 0.12 `bulk_insert` ✅ · rusqlite ✅ · mysql_async `LOAD DATA LOCAL` 핸들러 ✅(드라이버 미구현) · ODBC `odbc-api` 열 단위 배열 바인드 ✅(미구현) | 10~100× |
| **추출(읽기)** | PostgreSQL **`COPY … TO STDOUT`**(서버가 CSV/binary로 직접) · 그 외 = **배열 페치 크기 + 스트리밍**(Oracle `fetch_array_size`/prefetch · SQL Server 행 스트림 · SQLite step) + 파일 쓰기 버퍼링 | Oracle `fetch_array_size` ✅(설정 `db.fetch_size`) · PG `copy_out` ✅(미사용) · 나머지 스트리밍 ✅ | 2~10× (PG COPY는 20×까지) |
| **공통** | 다중 행 `INSERT … VALUES (…),(…)` · 준비문 1회 + 실행 N · 커밋 묶기(N행마다) · 인덱스/제약 잠시 끄기 · 로깅 최소화 | 방언 독립 · 지금 없음 | 5~20× |

우리 현 상태: **적재 경로가 없다**(`nsql export`만 · import/bulk 미구현 · DR-6 범위) · 그리드 편집 적용은 행마다 문장 1개(안전 규칙상 1행 = 1문장 유지 · 87 §14) · 추출은 `RowSource` 스트리밍 + 형식 렌더러 1벌(DR-33)로 이미 복사 0.

## 1. DBMS별 지원 방식

### 1-1. Oracle

| 기법 | 방식 | 지원 | 우리 crate(`oracle` 0.6 · ODPI-C) | 비고 |
|---|---|---|---|---|
| **배열 바인드 DML**(array DML · OCI `OCIBindByName` + iters) | 한 문장에 N행의 바인드 배열 → 왕복 1회 | ✅ 모든 판 | ✅ `Connection::batch(sql, n)` → `Batch::append_row` · `execute`(행 단위 오류 수집 `with_batch_errors`) | 가장 현실적 · 10~50× ⚠ · 트리거·제약 그대로 |
| **Direct Path Load**(OCI DirectPath · `INSERT /*+ APPEND */`) | 버퍼 캐시 우회 · 블록 직접 기록 · `NOLOGGING`이면 redo 최소 | ✅ (권한·표 잠금) | OCI DirectPath API는 ODPI-C 미노출 ✗ · **`/*+ APPEND */` 힌트 + 배열 바인드**로 대체 ✅ | `APPEND`는 표를 배타 잠금 · 같은 트랜잭션에서 그 표를 읽을 수 없음(커밋 후) |
| **SQL*Loader / 외부 테이블** | 외부 프로세스·서버 파일 | ✅ | 외부 도구 호출 = DR-3 밖(프로세스 생성 0 원칙) ✗ | 문서 안내만("이만한 크기는 SQL*Loader") |
| **배열 페치**(prefetch · fetch array) | 왕복당 N행 | ✅ | ✅ `fetch_array_size(db.fetch_size)` · `prefetch_rows` 옵션 가능 | 100 → 1,000이면 왕복 10배 감소 · LOB 열이 있으면 작게 |
| 병렬 DML(`ALTER SESSION ENABLE PARALLEL DML` + `/*+ PARALLEL */`) | 서버 병렬 | ✅ EE | 문장으로 보낼 수 있음 | 사용자 옵션(기본 끔) |
| `INSERT … SELECT`(서버 안 이동) | 클라이언트 경유 0 | ✅ | 문장 | 같은 서버 내 복사는 늘 이것이 최선 |

### 1-2. PostgreSQL

| 기법 | 방식 | 지원 | 우리 crate(`postgres` 0.19 = tokio-postgres 동기 래퍼) | 비고 |
|---|---|---|---|---|
| **`COPY … FROM STDIN`**(text/CSV/**binary**) | 프로토콜 CopyIn 스트림 · 서버 파서 | ✅ 모든 판 | ✅ `Client::copy_in(sql) -> CopyInWriter` (+ `BinaryCopyInWriter` for binary) | 단건 대비 20~100× ⚠ · 트리거·제약 그대로 · 오류는 전체 롤백(행 단위 재시도 없음) |
| **`COPY … TO STDOUT`** | 서버가 형식화해 스트림 | ✅ | ✅ `Client::copy_out(sql) -> CopyOutReader` | CSV 추출 최속 · 우리 형식(Markdown·JSON·INSERT)은 클라이언트 렌더 |
| 다중 행 `INSERT … VALUES` + `UNNEST` 배열 바인드 | 문장 1 · 바인드 N | ✅ | ✅ 일반 실행 | 파라미터 상한 65,535개 → 열 수로 나눠 배치 |
| **파이프라이닝**(extended protocol · 응답 대기 없이 연속 전송) | 왕복 지연 숨김 | ✅ 14+ 클라이언트 | tokio-postgres 비동기 `Client`에서 futures 동시 진행으로 가능 · 동기 래퍼에서는 ✗ | 원격(고지연)에서 효과 · 후속 |
| 준비문 재사용 | 파싱 1회 | ✅ | ✅ `prepare` + `execute` | |
| `UNLOGGED` 표 · `synchronous_commit=off` · `maintenance_work_mem`↑ · 인덱스 뒤에 만들기 | 서버 설정 | ✅ | 문장/세션 옵션 | 사용자 옵션 · 데이터 안전성 경고 |
| 바이너리 결과 형식 | 텍스트 변환 생략 | ✅ | ✅ 이미 binary(`FromSql`) | |
| 서버 커서·포털 페치 | 메모리 상한 | ✅ | ✅(43 · `fetch_next`) | |

### 1-3. SQL Server

| 기법 | 방식 | 지원 | 우리 crate(`tiberius` 0.12) | 비고 |
|---|---|---|---|---|
| **TDS BULK INSERT(bcp 프로토콜 · `INSERT BULK`)** | 행 데이터를 TDS 스트림으로 · 최소 로깅 가능 | ✅ | ✅ `Client::bulk_insert(table) -> BulkLoadRequest`(`send(TokenRow)` · `finalize`) · 옵션 `TABLOCK`·`CHECK_CONSTRAINTS`·`FIRE_TRIGGERS`·`KEEP_NULLS` | 10~50× ⚠ · 기본은 트리거·제약 **건너뜀**(옵션으로 켬) — 데이터 보호 규칙상 우리는 `CHECK_CONSTRAINTS` 기본 on |
| **TVP(테이블 값 매개변수)** | 한 호출에 표 하나 | ✅ 2008+ | tiberius ✗(미지원) | 후속 |
| 다중 행 `INSERT … VALUES` | 1,000행 상한 | ✅ | ✅ | 2,100 파라미터 상한 → 열 수로 배치 |
| `BULK INSERT`/`OPENROWSET(BULK)` | 서버 파일 | ✅ | 문장(서버가 파일을 봐야 함) | 원격 클라이언트에는 부적합 |
| 배치 크기·`TABLOCK`·복구 모델 bulk-logged | 서버 | ✅ | 옵션 | |
| 행 스트리밍 fetch | TDS 스트림 | ✅ | ✅ `QueryStream` | 커서 없음(43) |
| `sp_executesql` 준비 재사용 | | ✅ | ✅ | |

### 1-4. SQLite

| 기법 | 방식 | 지원 | 우리 crate(`rusqlite` 0.32 bundled) | 비고 |
|---|---|---|---|---|
| **한 트랜잭션에 묶기** | fsync 1회 | ✅ | ✅ `BEGIN`/`COMMIT` | 단건 자동 커밋 대비 **100×** 이상(디스크 동기화 비용) |
| **준비문 재사용** | 파싱 1회 | ✅ | ✅ `prepare_cached` | |
| `PRAGMA journal_mode=WAL` · `synchronous=OFF/NORMAL` · `cache_size` · `temp_store=MEMORY` · `locking_mode=EXCLUSIVE` | 파일 I/O 줄이기 | ✅ | 문장 | `synchronous=OFF`는 전원 장애 시 손상 위험 → 임시 적재에만 |
| 다중 행 VALUES | 파싱 감소 | ✅ 3.7.11+ | ✅ | 999/32,766 변수 상한 |
| 인덱스는 적재 뒤 · 외래 키 검사 잠시 끔(`PRAGMA foreign_keys=OFF`) | | ✅ | 문장 | |
| `.import`(CLI 셸) | 외부 | — | ✗ | |

### 1-5. MySQL · MariaDB(확장 드라이버 · 미구현)

| 기법 | 방식 | 지원 | crate(`mysql_async`) | 비고 |
|---|---|---|---|---|
| **`LOAD DATA LOCAL INFILE`** | 클라이언트 파일 스트림 | ✅(서버 `local_infile=ON` 필요) | ✅ `LocalInfileHandler`/`infile_handler` | 최속 · 보안 설정에 자주 막힘 |
| 다중 행 VALUES(`max_allowed_packet` 안) · `INSERT … ON DUPLICATE KEY` | | ✅ | ✅ | 가장 흔한 대안 · 10~20× ⚠ |
| `SET unique_checks=0, foreign_key_checks=0` · `innodb_flush_log_at_trx_commit=2` | 서버 | ✅ | 문장 | |
| `SELECT … INTO OUTFILE` | 서버 파일 | ✅ | ✗(서버 측) | |

### 1-6. ODBC 폴백(Tibero · Altibase · CUBRID …)

| 기법 | 방식 | crate(`odbc-api`) | 비고 |
|---|---|---|---|
| **열 단위 배열 바인드**(`SQL_ATTR_PARAMSET_SIZE`) | 문장 1 · N행 | ✅ `ColumnarBulkInserter` | 드라이버가 지원해야(대부분 ✅) |
| `SQLBulkOperations` | 행 단위 추가 | ✅(드라이버 의존) | |
| 블록 커서 fetch(`SQL_ATTR_ROW_ARRAY_SIZE`) | 왕복당 N행 | ✅ `RowSetBuffer` | |

## 2. 기법별 기술 방식(공통 원리)

1. **왕복 수 줄이기** — 배열 바인드·다중 행 VALUES·COPY/BULK 스트림은 모두 "행 N개 = 왕복 1회". 원격(수 ms RTT)에서는 이것이 지배적: 10만 행 × 3 ms = 300 s → 1,000행/왕복이면 0.3 s.
2. **파싱·계획 재사용** — 준비문 1회 + 바인드 실행 N(문자열 리터럴 조립 금지 · 우리 `Marker` 바인드 그대로).
3. **디스크 동기화 줄이기** — 커밋 묶기(N행마다 · 실패 시 그 배치만 재시도) · SQLite/PG `synchronous` · Oracle `NOLOGGING`/SQL Server bulk-logged(운영 DB에는 권하지 않음 · 옵션·경고).
4. **서버 안에서 끝내기** — 같은 서버 복사는 `INSERT … SELECT`/`CREATE TABLE AS` · 추출은 서버 형식화(`COPY TO`).
5. **클라이언트 메모리 상한** — 입력은 파일을 **스트리밍 파싱**(CSV 상태 기계 · 행 버퍼 N) · 출력은 `RowSource` 스트리밍(DR-33) + 8~64 KB 쓰기 버퍼 · 압축은 후속.
6. **타입 안전** — COPY binary·TDS bulk·배열 바인드는 열 타입을 정확히 알아야 한다 → 카탈로그(`MetaStore` 컬럼 타입 · 79 단일 원천)에서 `CellSpec`/`VarType`로 미리 맞춘다(문자열 → 날짜 변환은 `gridedit::datetime` 재사용).
7. **오류 처리** — 스트림형(COPY · BULK)은 행 단위 오류 위치를 못 주는 경우가 많다 → "배치 실패 = 그 배치를 단건 재실행해 문제 행 지목"(2단계) · 배열 바인드(Oracle `with_batch_errors` · ODBC 상태 배열)는 행 단위 오류 ✅.
8. **데이터 보호 규칙과의 관계**(87 §14 · CLAUDE.md §3) — 벌크 적재는 **INSERT 전용**(사용자가 파일을 넣는 명시 동작). UPDATE/DELETE는 규칙대로 1행 = 1문장 + 사전 검사를 유지한다(배열 바인드로 묶더라도 문장마다 영향 행 수 1 검사가 가능한 경로만 — Oracle 배열 DML은 행별 영향 수를 주므로 후속 검토, COPY/BULK는 INSERT뿐이라 해당 없음).

## 3. nexa-sql 설계 제안

### 3-1. 포트(30 §1 확장점 규칙 = 포트 + 레지스트리 + 설정)

```
nsql-core
  Caps.bulk_load: BulkLoad { None | MultiRow{max_rows, max_params} | ArrayDml | CopyIn{binary: bool} | TdsBulk }
  Caps.bulk_out:  BulkOut  { None | CopyOut }
  trait Session {
      fn bulk_begin(&mut self, table: &str, cols: &[ColumnSpec], opts: &BulkOpts) -> Result<Box<dyn BulkSink>, DbError>  // 기본 = Err(Unsupported)
      fn copy_out(&mut self, sql: &str, fmt: CopyFmt, sink: &mut dyn Write) -> Result<u64, DbError>                          // PG만
  }
  trait BulkSink { fn push(&mut self, row: &[Value]) -> Result<(), DbError>; fn finish(self: Box<Self>) -> Result<BulkReport, DbError>; }
nsql-run
  Runner::bulk_load(source: &mut dyn RowSource, table, map, opts) → 배치 크기 · 커밋 간격 · 오류 2단계(배치 실패 → 단건 재실행 지목) · Timeline 단계(parse · send · commit)
  폴백 사다리: 드라이버 BulkSink → MultiRow VALUES(파라미터 상한 안) → 단건 준비문
nsql-io
  CSV/TSV/JSONL 스트리밍 파서(RowSource 구현 · 헤더 → 열 매핑 · 타입 변환 = CellSpec) · 쓰기 버퍼 · (후속) gzip
```

드라이버 구현 = Oracle `Batch`(배열 DML · `with_batch_errors` · `APPEND` 힌트 옵션) · PG `copy_in` binary(타입 = 카탈로그 OID) + `copy_out` · MSSQL `bulk_insert`(`CHECK_CONSTRAINTS` 기본 on · `FIRE_TRIGGERS` 옵션) · SQLite 트랜잭션 + `prepare_cached` + PRAGMA 옵션 · MySQL/ODBC = 확장 드라이버 때.

### 3-2. 설정(REGISTRY · 39 §3 부하원 등재)

`bulk.batch_rows`(1,000) · `bulk.commit_every`(10,000 · 0 = 끝에 한 번) · `bulk.mode`(auto/driver/multirow/single) · `bulk.check_constraints`(on) · `bulk.fire_triggers`(off) · `bulk.append_hint`(off · Oracle) · `bulk.sqlite_pragmas`(off · WAL/synchronous) · `export.copy_out`(on · PG CSV/TSV만) · `export.buffer_kb`(64) · 네트워크 원장 26 §8 행 추가(사용자 동작 때만 · 상한 = 파일 크기).

### 3-3. 사용자 면

- CLI `nsql import -c <프로필> -t <표> -f csv|tsv|jsonl [--map a=col,…] [--batch N] [--commit-every N] [--mode …] <파일>` · `nsql export … --fast`(PG COPY TO) · 진행 = 행/초·ETA(stderr) · 결과 = 적재 행 · 실패 배치·행 번호.
- GUI = 탐색기 표 우클릭 **Import Data…**(파일 → 열 매핑 미리보기 10행 → 옵션 → 진행 카드 · ■ 취소 = `CancelHandle`) · 결과 도구줄 **Export**(77 §4 싱크 · PG는 COPY TO 경로 자동).
- 세션 통제: `gate_open()` 1회 · 적재 중 그 세션은 `Sess::blocked`(DR-34) · 취소 = 배치 경계에서 롤백.

### 3-4. 단계(T-236)

| 단계 | 내용 | 검증 |
|---|---|---|
| B-1 | `Caps.bulk_load` + `BulkSink` 포트 + `Runner::bulk_load`(MultiRow 폴백 · 오류 2단계 · Timeline) + nsql-io CSV 스트리밍 파서 + CLI `nsql import` | 모의 세션 단위 시험(배치 경계·커밋 간격·실패 배치 지목) · SQLite 실측 10만 행 |
| B-2 | Oracle `Batch` · PG `copy_in`(binary) · MSSQL `bulk_insert` · SQLite 트랜잭션+준비문 | 4-DBMS 실서버 임시 표(`NSQLT_BULK`) 10만 행 · 71 C 시나리오 등재(행/초 · 메모리 상한) |
| B-3 | PG `copy_out` 추출 · GUI Import 창(열 매핑 · 진행 카드) · 취소 | E2E 기동 명령 `import.run:` · 캡처 |
| B-4 | 옵션(APPEND · PRAGMA · FIRE_TRIGGERS) · TVP/파이프라이닝 검토 · 확장 드라이버(MySQL LOAD DATA · ODBC 배열) | |

### 3-4-a. 구현 상태(09-26 · journal §238)

| 단계 | 상태 | 내용 |
|---|---|---|
| B-1 | ✅ | nsql-core `bulk.rs`(`BulkLoad` 능력표 · `BulkOpts` · `BulkSink` 포트 · `Session::bulk_begin`) · nsql-io `delim::DelimReader`(스트리밍 CSV/TSV · 따옴표 안 줄바꿈 · BOM · 레코드 시작 줄 번호) · nsql-run `bulk.rs`(`Runner::bulk_load` = 타입 변환 `coerce` · 드라이버 싱크 `drive_sink` / 다중 행 폴백 `multirow_sql`(Oracle `INSERT ALL`) · 배치·커밋 간격 · 실패 배치 단건 재실행 = 문제 행·줄 지목 + 성공 앞부분 다시 넣어 커밋 · Timeline) · CLI `nsql import`(헤더/--no-header/--cols/--map/--batch/--commit-every/--mode/--empty-null · 진행·요약·--timing · 열 메타 = 빈 조회 → 없으면 카탈로그) · 설정 `bulk.*` 7키 |
| B-2 | ✅ | PG `COPY … FROM STDIN` text 싱크(문장 하나 = 원자 · 중간 커밋 없음 = `commit → false`) · Oracle 배열 DML 싱크(`Connection::batch` · 열 타입별 명시 바인드 `Typed`: NUMBER/Timestamp(ISO)/CLOB/BLOB/Varchar2 · 중간 커밋 진짜) · SQLite = 다중 행(500행/문장 · 트랜잭션) · SQL Server 다중 행 폴백(파라미터 상한 2000 = 500행/문장) |
| B-4 (일부) | ✅ §239 | **SQL Server TDS BULK INSERT 싱크**(`Client::bulk_insert` · 배치마다 열 메타 조회 → 행 스트림 → finalize · 요청이 세션을 빌리므로 `flush` 안에서만 · 트랜잭션은 싱크가 `BEGIN/COMMIT/ROLLBACK TRAN` · 열 종류 = 카탈로그 타입 → `ColumnData` 정확 일치(`MsKind` 16종 · `datetime2` ≠ `datetime` · numeric = 자릿수 i128 · date/time/datetime/smalldatetime/uniqueidentifier) · 변환 오류 = 러너가 단건 재실행으로 지목 · 드라이버 경로 실패인데 단건이 전부 성공하면 **나머지는 다중 행 폴백으로 이어 감**) · **JSON Lines 원료**(`nsql_io::jsonl` 평평한 객체 파서 · 키 → 열 · 첫 객체 키 = 헤더 · 빠진 키/null = NULL · `-f jsonl` 또는 `.jsonl/.ndjson`) · TVP·파이프라이닝·MySQL/ODBC는 남음 |
| B-3 | ✅ §240 | **PG `COPY TO` 추출**(`Session::copy_out` 포트 · PG `COPY (<sql>) TO STDOUT WITH (FORMAT csv, HEADER)` · CLI `nsql export --fast` csv/tsv · 미지원 = 안내 뒤 일반 경로) · **GUI Import 창**(`import_win.rs` · 탐색기 표 우클릭 ▸ Import Data… → 파일 창 → 미리보기 6줄 · 헤더/매핑/배치/커밋/경로 옵션(기본 = `bulk.*`) · Start/Cancel/Close · 진행 줄 200 ms · 결과 = 성공/실패 행·줄 지목/취소 · 활성 탭 세션 · `Sess.aux` 통제) · 오케스트레이션 단일 원천 `nsql_run::bulk::ImportSpec` + `Runner::import_file`(CLI·GUI 공용) · 취소 = `BulkParams.cancel` 배치 경계(커밋 안 된 배치 롤백 · 커밋된 앞부분 유지) · 자체 시험 `import.open/start/cancel/dump` |
| 실측(Debug · 5,000행 · 4열 · 원격 192.168.x) | | Oracle 배열 DML **13~14k행/s**(0.36 s) · PG COPY **15~20k행/s**(0.25 s) · SQLite 다중 행 **57k행/s**(1,000행 18 ms) · SQL Server **TDS bulk 5.2k행/s**(배치 1,000 · 0.96 s) → **13.9k행/s**(배치 5,000 · 1배치 · 배치마다 메타 조회+finalize 왕복이 커 SQL Server는 `--batch`를 키울수록 빠름) · 종전 다중 행 1.1~1.7k행/s |
| 시험 | | nsql-run `bulk` 8(변환·문장·배치/커밋·실패 지목·열 수·**취소**·형식 판정·미리보기) · nsql-io 2 · E2E `scripts/mac-bulk-e2e.sh`(SQLite ①~④ CLI + **⑤ GUI Import 창** 17 검사 + `-P/-d` 실서버 3종 임시 표 `NSQLT_BULK` 5,000행 + PG `--fast` = **28/28**) |
| 결정 | | D-220 INSERT 전용 ✅ · D-221 `bulk.check_constraints` on/`fire_triggers` off(옵션만 · TDS bulk 때 적용) · D-222 **PG COPY = text**(binary는 후속 · text가 서버 풀이라 타입 안전) · D-223 커밋 간격 10,000(COPY는 문장 원자) · D-224 위험 옵션 기본 끔(`bulk.append_hint`) — 한 줄 고지로 진행 |

남음 = Import 창 후속(열 매핑 표 UI · 진행 카드 연동 · 결과 탭 새로 고침) · B-4 나머지(TVP · 파이프라이닝 · MySQL `LOAD DATA`/ODBC 배열 · SQL Server bulk 옵션 `CHECK_CONSTRAINTS`/`FIRE_TRIGGERS`는 tiberius 0.12에 API 없음).

### 3-5. 결정 대기

- **D-220** 벌크 적재 = INSERT 전용(UPDATE/DELETE는 87 §14 규칙 유지) — 권장 ✅.
- **D-221** SQL Server bulk 기본 `CHECK_CONSTRAINTS` on · `FIRE_TRIGGERS` off — 권장(안전 우선 · 트리거는 옵션).
- **D-222** PG COPY = binary 기본(타입은 카탈로그에서) · 카탈로그를 못 읽으면 text COPY 폴백.
- **D-223** 커밋 간격 기본 10,000행(실패 배치는 그 배치만 롤백 · 이미 커밋된 배치는 남는다는 고지) vs 끝에 한 번(전부-아니면-무).
- **D-224** 위험 옵션(NOLOGGING/APPEND · `synchronous=OFF` · 제약 끄기)은 **기본 끔 + 확인 창**.

## 4. 참고(원천)

Oracle ODPI-C/`oracle` crate `Batch`·`with_batch_errors` · Oracle Database SQL Tuning Guide "Direct-Path INSERT"/`APPEND` · PostgreSQL 문서 `COPY`·"Populating a Database"(14.4) · `postgres` crate `copy_in`/`copy_out`/`BinaryCopyInWriter` · Microsoft Learn "Bulk Import and Export"·"Minimal Logging"·TDS `INSERT BULK` · `tiberius` `bulk_insert`/`BulkLoadRequest` · SQLite FAQ "INSERT is really slow"·PRAGMA · `rusqlite` `prepare_cached` · MySQL `LOAD DATA`·"Optimizing INSERT Statements" · `odbc-api` `ColumnarBulkInserter`. 수치(⚠)는 각 문서·공개 벤치의 대략치로 실측(71)이 원장이다.
