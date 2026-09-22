# 66 · 트랜잭션 테스트 시나리오 — 자동/수동 커밋 × DBMS별 · 순서대로 진행하는 점검표

> **요청**(사용자 09-22): *"트랜잭션 자동 커밋·수동 커밋 상태에서 DBMS별로 구현된 기능을 점검하는 과정을 시나리오 문서로 만들어 순서대로 진행할 수 있게."*
> **근거 문서**: [34 트랜잭션 UX](34-transaction-ux.md)(3층 표시 · 상태 기계 · 묻는 순간) · [44 트랜잭션 로그](44-transaction-log.md)(Tx 열 문구) · [56 수동 커밋 잠금 방지](56-manual-commit-lock-prevention.md)(L1~L4 · DBMS별 잠김) · [52 세션 모드](52-session-modes.md)(공유 N · 전용 `CONNECT` · 통제) · [53 접속 생존](53-connection-liveness.md)(끊김) · [61 §1-5](61-core-design-and-working-rules.md)(**수동 커밋 = 진짜 트랜잭션** T-146).
> **쓰는 법**: 위에서 아래로. 각 단계는 `조작 → 기대(우리 앱) → 기대(다른 세션에서 본 서버) → 기록`. 한 DBMS를 끝까지 한 번 돌리고 다음 DBMS로. 결과는 §9 표에 ✅/❌/— 로.

---

## 0. 전제 · 준비

### 0-1. 대상과 도구

| DBMS | 프로필(사용자 볼트) | **두 번째 세션 도구**(서버의 진짜 상태를 보는 눈) | 비고 |
|---|---|---|---|
| Oracle | `BISCM`(192.168.0.58:1521) | `sqlplus BISCM/…@//192.168.0.58:1521/BISCM` | Instant Client 필요([64](64-dbms-clients-and-driver-packaging.md)) |
| SQL Server | `M4PLAN`(192.168.0.58:1433) | `sqlcmd -S 192.168.0.58 -U BISCM_MS -d M4PLAN_MS` | 없으면 SSMS |
| PostgreSQL | `Repository`(192.168.0.60:5433/matrixdb2) | `psql -h 192.168.0.60 -p 5433 -U postgres matrixdb2` | |
| SQLite | `Local`(파일 경로 하나 — `:memory:`는 두 번째 세션이 없으므로 **파일**로) | `sqlite3 <같은 파일>` | 격리 홈에 새 프로필 |
| MySQL/MariaDB | (드라이버 미탑재 · 카탈로그만) | — | §1 표에 기대 동작만 적어 두고 드라이버가 들어오면 같은 절차로 |

- 앱은 **격리 홈**에서 돈다(`NSQL_HOME=<임시 폴더>` · 실제 설정·볼트를 건드리지 않는다 · [61 §2-4](61-core-design-and-working-rules.md)). 프로필은 `nsql conn add …`로 격리 홈에 셋 등록.
- 시험 표 `NSQLT_TX`는 각 DBMS에 **시나리오 시작에 만들고 끝에 지운다**(§0-3). 실서버 통합 테스트의 `NSQLT_*` 관용과 같다.
- 시간을 기다리는 단계(§5)는 **설정으로 시간을 줄여서** 돈다(§0-2 두 번째 열) — 기본값으로는 10~30분이 걸린다.

### 0-2. 설정(격리 홈 `settings.conf`) — 기본값과 시험용 단축값

| 키 | 기본 | 시험용(§5) | 뜻 |
|---|---|---|---|
| `session.autocommit` | on | — | 전역 기본 모드(상태줄 팝업으로 탭/세션마다 전환) |
| `tx.smart_commit` | off | §3 A8에서 on | 자동 모드라도 첫 DML 뒤 수동으로 전환(DBeaver식) |
| `tx.read_end` | auto | strict도 한 번 | L1 읽기 트랜잭션 자동 종료(auto = 문장 분류 · strict = 서버에 물어 확인 · off) |
| `tx.stale_min` | 10 | **1** | 미커밋이 이 시간을 넘으면 배지 빨강 + 경고 카드 |
| `tx.remind_min` | 10 | **1** | 재알림 간격 |
| `tx.idle_action` | rollback | rollback · warn · commit 각 1회 | 유휴 한도 뒤 자동 처리 |
| `tx.idle_limit_min` | 30 | **2** | 자동 처리까지의 유휴 시간 |
| `tx.idle_countdown_secs` | 60 | **10** | 자동 처리 전 카운트다운 |
| `tx.block_poll_secs` | 30 | **5** | L3 막힘 감지 주기(0 = 끔 · 향상 모드 0) |
| `tx.server_idle_timeout_secs` | 0 | **20**(PG만) | L4 서버 유휴 트랜잭션 타임아웃(접속 직후 `SET`) |
| `tx.lock_wait_timeout_secs` | 0 | 5 | 내 문장의 잠금 대기 상한 |
| `tx.close_action` | ask | ask · commit · rollback 각 1회 | 미커밋 탭 닫기 |
| `tx.badge` | count | dot · off 각 1회 | 탭 배지 표시 |
| `tx.prod_stale_min` / `tx.prod_idle_limit_min` | 5 / 10 | 1 / 2 | 운영 접속의 더 엄격한 값(§6) |
| `run.prod_confirm` | on | — | 운영 접속에서 변경 문장은 3초 안에 같은 글을 다시 실행해야 진행 |
| `session.mode` | shared | per-editor 1회(§7) | 탭별 세션 |

### 0-3. 시험 표 만들기/지우기(방언별)

```sql
-- Oracle
CREATE TABLE NSQLT_TX (ID NUMBER PRIMARY KEY, V VARCHAR2(40));
-- SQL Server
CREATE TABLE NSQLT_TX (ID INT PRIMARY KEY, V NVARCHAR(40));
-- PostgreSQL
CREATE TABLE nsqlt_tx (id INT PRIMARY KEY, v VARCHAR(40));
-- SQLite
CREATE TABLE nsqlt_tx (id INTEGER PRIMARY KEY, v TEXT);
-- 끝에 전부: DROP TABLE NSQLT_TX;
```

두 번째 세션의 **관찰 질의**(각 단계의 "서버에서 본 것"):

| DBMS | 행 수 | 내 세션의 트랜잭션 상태 | 잠금·차단 |
|---|---|---|---|
| Oracle | `SELECT COUNT(*) FROM BISCM.NSQLT_TX;` | `SELECT s.sid, s.status, s.taddr FROM v$session s WHERE s.username='BISCM' AND s.program LIKE '%nexa%';`(`taddr`가 NULL이 아니면 트랜잭션 열림) | `SELECT * FROM v$lock WHERE type IN ('TM','TX') AND sid = <sid>;` · 차단 = `SELECT sid, blocking_session FROM v$session WHERE blocking_session IS NOT NULL;` |
| SQL Server | `SELECT COUNT(*) FROM NSQLT_TX;` | `SELECT session_id, open_transaction_count, status FROM sys.dm_exec_sessions WHERE login_name='BISCM_MS';` | `SELECT request_session_id, resource_type, request_mode FROM sys.dm_tran_locks WHERE resource_database_id = DB_ID();` · 차단 = `SELECT session_id, blocking_session_id FROM sys.dm_exec_requests WHERE blocking_session_id <> 0;` |
| PostgreSQL | `SELECT count(*) FROM nsqlt_tx;` | `SELECT pid, state, xact_start, left(query,40) FROM pg_stat_activity WHERE usename='postgres' AND application_name LIKE '%nexa%' OR pid <> pg_backend_pid();`(**`idle in transaction`** = 열린 트랜잭션) | `SELECT pid, locktype, mode, relation::regclass FROM pg_locks WHERE pid <> pg_backend_pid();` · 차단 = `SELECT pid, pg_blocking_pids(pid) FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0;` |
| SQLite | `SELECT count(*) FROM nsqlt_tx;`(같은 파일을 연 둘째 `sqlite3`) | 상태 뷰 없음 — 둘째 접속에서 `INSERT`를 시도해 `database is locked`가 나면 열린 쓰기 트랜잭션 | (롤백 저널) 읽기는 되고 쓰기만 막힘 · WAL이면 읽기 무관 |

---

## 1. DBMS별 트랜잭션 동작 — 우리가 구현한 것(코드가 정한 사실 · 시나리오의 기대값 원천)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL(준비) | 원천 |
|---|---|---|---|---|---|---|
| 서버가 문장마다 커밋하나(`server_autocommit`) | 아니오(암묵 트랜잭션) | 예 | 예 | 예 | 예 | `nsql-core caps.rs` |
| 수동 모드 첫 문장 앞에 러너가 여는 문장(`tx_begin`) | 없음 | `BEGIN TRANSACTION` | `BEGIN` | `BEGIN` | `START TRANSACTION` | `manual_begin_sql`(T-146) |
| 자동 모드에서 클라이언트가 `COMMIT`을 보내는가 | 늘(문장 뒤) | 러너/사용자가 연 트랜잭션이 있을 때만 | 같음 | 같음 | 같음 | `autocommit_needs_commit` |
| SELECT만 한 트랜잭션이 서버에 남는 것 | 없음(`FOR UPDATE`·DB 링크 예외) | 잠금은 문장 뒤 풀리지만 트랜잭션은 남음 | **`idle in transaction`** + 스냅샷 + `ACCESS SHARE` | SHARED 읽기 잠금 | MDL + 스냅샷 | [56 §1](56-manual-commit-lock-prevention.md) |
| L1 읽기 종료(`tx.read_end=auto`)의 효과 | 사실상 무동작 | 트랜잭션 닫힘 | `idle`로 돌아감 | 잠금 해제 | 스냅샷 해제 | 56 L1 |
| `strict` 탐침(서버에 "변경 있었나" 확인) | `dbms_transaction.local_transaction_id` | `sys.dm_tran_*` 로그 레코드 수 | `pg_current_xact_id_if_assigned()` | 없음(auto와 같음) | 없음 | `caps.strict_probe` |
| DDL이 트랜잭션 안인가(`ddl_transactional`) | **아니오 — 암묵 커밋** | 예 | 예 | 예 | 아니오 — 암묵 커밋 | `Dialect::ddl_transactional` |
| 암묵 커밋 문장(`implicit_commit`) | `CREATE ALTER DROP TRUNCATE GRANT REVOKE RENAME COMMENT` → n=0 · Tx 열 `암묵 커밋(CREATE)` | 없음 | 없음 | 없음 | Oracle과 같음 | `Dialect::implicit_commit` |
| TRUNCATE 롤백 가능(`truncate_transactional`) | 아니오 | 예 | 예 | 예 | 아니오 | |
| 오류 난 문장 | 배지 수에 넣지 않음 · 트랜잭션은 유지(서버가 문장만 롤백) | PG와 달리 트랜잭션 유지 | **트랜잭션이 `aborted`** — 다음 문장은 `current transaction is aborted` → Rollback 필요 | 유지 | 유지 | 44 §2-3 · PG 고유 |
| L3 막힘 감지 질의 | `v$session.blocking_session` | `sys.dm_exec_requests.blocking_session_id` | `pg_blocking_pids()` | 해당 없음 | `sys.innodb_lock_waits` | 56 L3 |
| L4 접속 직후 서버 안전망 | `ddl_lock_timeout` | `SET LOCK_TIMEOUT` | `idle_in_transaction_session_timeout` · `lock_timeout` | `busy_timeout`(드라이버 기본) | lock wait | 56 L4 |
| 끊김 뒤 열린 트랜잭션 | 서버가 롤백 → Tx 열 `끊김(롤백)` | 같음 | 같음 | 프로세스 종료 = 저널 롤백 | 같음 | 44 · 53 |
| CLI `nsql run` | 러너 기본 = **수동**(SQL*Plus 관용) · 스크립트 끝 `commit_at_exit`(암묵 트랜잭션이거나 열린 것이 있으면 COMMIT) · `SET AUTOCOMMIT ON/OFF`가 우선 | | | | | `Runner::commit_at_exit` |

> ★ 시나리오의 **핵심 검증점**은 표의 두 번째 줄이다: PG·SQL Server·SQLite에서 수동 모드의 DML이 **다른 세션에 보이지 않다가 Commit 뒤에 보이고 Rollback이면 사라져야** 한다(T-146 이전에는 "미커밋 ●"로 보이면서 이미 커밋돼 있었다).

---

## 2. 표시가 어디에 나오나(관찰 창) — 각 단계에서 볼 것

| 층 | 자동 커밋 | 수동 커밋 · 대기 n |
|---|---|---|
| 상태줄 세그먼트 | `자동 커밋` | `수동 커밋 ● n` · 클릭 = 팝업(전환 · Commit n · Rollback n) |
| 탭 표식(배지) | 없음 | `●n`(`tx.badge=count`) · `tx.stale_min` 넘으면 빨강 |
| 툴바 Commit/Rollback | 비활성 | n>0이면 활성 + Commit 배지 |
| 트랜잭션 로그 창(View ▸ Transaction log) | 문장 행 Tx 열 `자동 커밋` | `대기` → `커밋됨 hh:mm` / `롤백됨 hh:mm`(취소선) / `암묵 커밋(CREATE)` / `읽기만 · 종료됨` / `끊김(롤백)` · 상단 "차단 중" 띠 |
| 실행 상태 카드 · 토스트 | 완료/오류 | + L2 경고 카드(`txwarn` · [지금 롤백][커밋][연장]) · L3 위험 토스트 |
| 세션 창(View ▸ Sessions) | — | 세션별 미커밋 수 · Disconnect ▾ |

---

## 3. 시나리오 A — 자동 커밋(기본 · `session.autocommit=on`)

각 DBMS에서 순서대로. **A = 문장이 곧 커밋**이 전부 맞는지.

| # | 조작(편집기 · Ctrl+Enter = 현재 문 실행) | 기대(앱) | 기대(두 번째 세션) |
|---|---|---|---|
| A1 | 접속 → §0-3 `CREATE TABLE` | 상태줄 `자동 커밋` · 배지 없음 · 툴바 Commit 비활성 · 로그 Tx 열 `자동 커밋` | 표가 보임 |
| A2 | `INSERT INTO NSQLT_TX VALUES (1,'a');` | 실행 카드 "1행" · 배지 여전히 없음 | **즉시** `COUNT(*) = 1` · 트랜잭션 상태 = 없음(PG `idle` · Oracle `taddr` NULL · MSSQL `open_transaction_count 0`) |
| A3 | `UPDATE NSQLT_TX SET V='b' WHERE ID=1;` → `DELETE FROM NSQLT_TX WHERE ID=1;` | 각 1행 · Tx 열 `자동 커밋` | 즉시 반영 · 끝에 0행 |
| A4 | `SELECT * FROM NSQLT_TX;` 여러 번 | 결과 탭 · 로그에 Tx 열 `자동 커밋` | PG `pg_stat_activity.state = idle`(**`idle in transaction`이 아님**) |
| A5 | 틀린 문장 `INSERT INTO NSQLT_TX VALUES ('x');` | 오류 카드/토스트 · 로그 결과 = `[코드] 메시지` · 배지 없음 | 아무 변화 없음 · **PG: 다음 문장이 정상 실행**(자동 모드는 문장마다 새 트랜잭션) |
| A6 | 큰 조회 실행 뒤 ■ 중지(`SELECT` 수만 행 · Oracle `all_objects` 또는 `generate_series`) | 결과 = `중지 · Fetch …` · 세션 `busy`→`idle` · 커서 정리(자동 모드 미룬 커밋 `deferred_commit`) | 트랜잭션 남지 않음(PG `idle`) |
| A7 | DDL: `ALTER TABLE NSQLT_TX ADD (X NUMBER)`(Oracle) / `ADD X INT`(나머지) | Tx 열 `자동 커밋`(Oracle은 `암묵 커밋(ALTER)`가 아니라 자동 모드이므로 `자동 커밋`) | 즉시 반영 |
| A8 | 설정 `tx.smart_commit=on` → `INSERT … (2,'c');` | **그 세션이 수동으로 전환** · 상태줄 `수동 커밋 ● 1` · 배지 `●1` · 툴바 Commit 활성 → Commit → 다시 `수동 커밋`(자동으로 돌아가지 않음 · DBeaver와 같음) → 상태줄 팝업으로 자동 복귀 | Commit 전 = PG/MSSQL/SQLite에서 **보이지 않음** · 뒤 = 보임 |
| A9 | 설정 원복(`tx.smart_commit=off`) · `DELETE FROM NSQLT_TX;` | | 0행 |

---

## 4. 시나리오 B — 수동 커밋(상태줄 팝업 ▸ 수동 커밋)

**B = 진짜 트랜잭션**(T-146) + 3층 표시 + 묻는 순간([34 §2-4](34-transaction-ux.md)). 표 `NSQLT_TX`는 비어 있는 상태로 시작.

| # | 조작 | 기대(앱) | 기대(두 번째 세션) |
|---|---|---|---|
| B1 | 상태줄 팝업 ▸ **수동 커밋** | 상태줄 `수동 커밋`(n 없음) · 배지 없음 · Commit 비활성 | — |
| B2 | `SELECT * FROM NSQLT_TX;` 세 번 | 배지 없음 · 로그 Tx 열 **`읽기만 · 종료됨`**(L1 `tx.read_end=auto` · PG/MSSQL/SQLite) · Oracle은 `자동 커밋`이 아닌 **빈 Tx**(트랜잭션이 열리지 않음) | **PG `state = idle`**(`idle in transaction`이 아님 — 56 L1의 목적) · MSSQL `open_transaction_count 0` |
| B3 | `INSERT INTO NSQLT_TX VALUES (1,'a');` | 상태줄 `수동 커밋 ● 1` · 탭 `●1` · 툴바 Commit 1 · 로그 Tx 열 `대기` · 실행 카드 1행 | **보이지 않음**(`COUNT(*) = 0`) · PG `idle in transaction` · Oracle `taddr` 있음 · MSSQL `open_transaction_count 1` · SQLite 둘째 접속 `INSERT` = `database is locked` |
| B4 | `INSERT … (2,'b');` · `UPDATE NSQLT_TX SET V='c' WHERE ID=1;` | `● 3`(DML 문장 수) | 여전히 0 |
| B5 | `SELECT * FROM NSQLT_TX;`(같은 세션) | 결과 2행(자기 트랜잭션은 자기에게 보임) · 배지 그대로 `●3`(읽기는 트랜잭션을 끝내지 않음 — 변경이 대기 중) | 0 |
| B6 | 툴바 **Rollback** | 팝업 없이 즉시 · `수동 커밋`(n 없음) · 로그의 세 행 Tx 열 **`롤백됨 hh:mm`**(취소선) · 결과 새로고침 → 0행 | 0 · 트랜잭션 없음 ← **T-146 핵심: 종전에는 여기서 2행이 남아 있었다** |
| B7 | `INSERT … (1,'a');` → 툴바 **Commit** | `●1` → `수동 커밋` · Tx 열 `커밋됨 hh:mm` | **1행 보임** · 트랜잭션 없음 |
| B8 | 문장으로: `INSERT … (2,'b');` `COMMIT;` → `INSERT … (3,'c');` `ROLLBACK;` | `COMMIT` 문장 = 배지 0 · Tx 열 `커밋됨` / `ROLLBACK` 문장 = `롤백됨` · 통제 문장 자체는 배지에 안 셈 | 2행(1,2) |
| B9 | 오류 문장 `INSERT … VALUES ('x');` 뒤 `INSERT … (4,'d');` | 오류 행 Tx 없음(배지 미포함) · **PG: 둘째 문장이 `current transaction is aborted` 오류** → 상태줄/카드가 Rollback을 권함 · Rollback 뒤 정상 · **Oracle/MSSQL/SQLite: 둘째 문장 정상 `●1`** | PG는 Rollback 뒤 2행 · 나머지 Commit 뒤 3행 |
| B10 | DDL in 수동: `CREATE TABLE NSQLT_TX2 (A INT);` | **Oracle**: Tx 열 `암묵 커밋(CREATE)` · **앞서 대기하던 DML도 함께 커밋되어 n=0** · **PG/MSSQL/SQLite**: `●n+1`(DDL이 트랜잭션 안) → Rollback → **표가 사라짐** | Oracle 즉시 보임 · PG는 Rollback 뒤 없음 |
| B11 | `TRUNCATE TABLE NSQLT_TX;` → Rollback | Oracle = 암묵 커밋(되돌릴 수 없음 · 로그 `암묵 커밋(TRUNCATE)`) · PG/MSSQL/SQLite = 롤백되어 행 복구 | 표의 `truncate_transactional`대로 |
| B12 | `SELECT * FROM NSQLT_TX WHERE ID=1 FOR UPDATE;`(Oracle·PG · MSSQL은 `WITH (UPDLOCK)`) | L1이 **끝내지 않음**(잠금이 목적 · `TxEffect::UserHold`) · 배지는 0이지만 트랜잭션 유지 · 로그 Tx `대기` | PG `idle in transaction` · Oracle `taddr` 있음 · 둘째 세션의 같은 행 `UPDATE`가 **대기** → Rollback으로 풀림 |
| B13 | 사용자 `BEGIN;`(PG/SQLite) · `BEGIN TRANSACTION;`(MSSQL) · Oracle `SET TRANSACTION READ ONLY;` → SELECT → | 사용자가 연 트랜잭션은 L1이 건드리지 않음 · `COMMIT;`으로 끝 · 러너의 `BEGIN`이 겹쳐 "이미 트랜잭션 안" 오류가 **나지 않아야** 한다(61 §1-5 불변식 `note_tx_ended`) | |
| B14 | 스크립트 `SET AUTOCOMMIT ON` · `INSERT …` · `SET AUTOCOMMIT OFF` · `INSERT …`(전체 실행) | 첫 INSERT = `자동 커밋` · 둘째 = `대기 ●1`(스크립트 설정이 그 실행에 우선) | 첫 행만 보임 |
| B15 | `●n` 상태로 **자동 커밋으로 전환**(상태줄 팝업) | 모달 3택 **Commit and switch / Rollback and switch / Cancel** · 고른 대로 로그 Tx 열(`커밋됨` · `롤백됨`) → `자동 커밋` | 선택대로 |
| B16 | `●n` 상태로 **탭 닫기**(× · Ctrl+W) — `tx.close_action` = ask / commit / rollback 각 1회 | ask = 3택(Commit · Rollback · Cancel) · commit/rollback = 묻지 않고 처리 + 로그 | |
| B17 | `●n` 상태로 **접속 해제**(툴바 Disconnect) / 다른 서버 접속 | 같은 3택 · 해제 뒤 세션 창에서 사라짐 | 취소하지 않은 한 트랜잭션 없음 |
| B18 | `●n` 상태로 **앱 종료** | 탭별 목록 + Commit all · Rollback all · Cancel | |
| B19 | `●n` 상태로 네트워크 끊기(VPN 끊기 또는 서버에서 세션 KILL — Oracle `ALTER SYSTEM KILL SESSION` · PG `pg_terminate_backend(pid)` · MSSQL `KILL <spid>`) → 다음 실행 | 끊김 판정(53 · 플러그 빨강 · Broken) · 로그 Tx 열 **`끊김(롤백)`** · 배지 0 · 재접속은 자동으로 하지 않음(26 §8) | 서버가 롤백 → 행 없음 |
| B20 | `tx.badge = dot` / `off`로 B3 반복 | 점만 / 배지 없음(상태줄·툴바는 그대로) | |
| B21 | `tx.read_end = strict`로 B2·B12 반복(Oracle·PG·MSSQL) | 결과 뒤 서버 탐침 1회(로그 개발자 모드에 문장 보임) · 판정 같음 | |
| B22 | `tx.read_end = off`로 B2 | PG **`idle in transaction`이 남는다**(종전 동작 · 사용자가 반복 읽기를 원할 때만) → 원복 | |

---

## 5. 시나리오 C — 잠금 방지(56 L2~L4 · 시험용 단축값 §0-2)

수동 모드 · `INSERT … (9,'idle');` 한 건을 넣고 **그 세션에서는 아무것도 하지 않는다**(다른 탭에서 일해도 된다 — 시계는 그 세션의 문장 실행만).

| # | 조작/대기 | 기대 |
|---|---|---|
| C1 | `tx.stale_min=1` 경과 | 탭 배지 **빨강** · 상태줄 `pending 1 min` · 경고 카드(`txwarn` · 실행 카드 위 · 비모달) "[지금 롤백] [커밋] [연장]" · 창이 비활성이면 작업 표시줄 깜빡임 · 로그 창 1줄 |
| C2 | [연장] | 카드 사라짐 · `tx.remind_min=1` 뒤 다시 |
| C3 | 그 세션에서 아무 문장 실행 | 카드·시계 전부 되돌림(다시 1분부터) |
| C4 | `tx.idle_action=rollback` · `tx.idle_limit_min=2` 경과 | **카운트다운 카드 10초**(취소 가능) → 만료 = 자동 롤백 · 로그 Tx 열 `롤백됨(자동 · 유휴 2분)` · 배지 0 · 두 번째 세션에서 행 없음 |
| C5 | `tx.idle_action=warn` | 카드만 · 자동 처리 없음(무한히 대기 · 배지 빨강 유지) |
| C6 | `tx.idle_action=commit`(권하지 않음 · 설정 설명에 경고) | 카운트다운 → 자동 커밋 · 로그 `커밋됨(자동)` · 두 번째 세션에서 행 보임 |
| C7 | **L3 막힘 감지**: 앱에서 `UPDATE NSQLT_TX SET V='x' WHERE ID=1;`(미커밋) → 두 번째 세션에서 같은 행 `UPDATE`(대기 상태로 둠) → `tx.block_poll_secs=5` | 위험 토스트 "내 트랜잭션 때문에 1개 세션이 기다립니다" · 상태줄 `차단 중 1` · 트랜잭션 로그 창 상단 띠(기다리는 세션의 사용자·프로그램·대기 시간) → Commit/Rollback → 해소 로그 · 둘째 세션 UPDATE 진행 · **SQLite = 해당 없음**(둘째 접속은 `database is locked`) |
| C8 | L3 권한 없는 계정(Oracle `v$session` 못 읽는 사용자) | 기능이 그 세션에서 조용히 꺼짐 + 로그 1회(오류 토스트 없음) |
| C9 | **L4**(PG): `tx.server_idle_timeout_secs=20` → 재접속 → 수동 `INSERT` → 25초 대기 → 다음 실행 | 접속 직후 로그에 `SET idle_in_transaction_session_timeout` · 서버가 세션을 끊음 → 다음 실행에서 끊김 판정 · Tx 열 `끊김(롤백)` · 자동 재접속 없음 |
| C10 | `tx.lock_wait_timeout_secs=5`: 둘째 세션이 행을 잠근 채(미커밋 UPDATE) → 앱에서 같은 행 UPDATE | 5초 뒤 오류(PG `lock timeout` · MSSQL `Lock request time out` 1222 · Oracle `ORA-00054`는 DDL만 — DML은 대기가 상한 없음이 정상 · MySQL lock wait) · 42 정규화 분류 `Lock` 토스트 |
| C11 | 향상 모드(`perf.boost`) 켬 | `tx.block_poll_secs` 강제 0 = L3 정지(L1·L2·L4는 그대로 — 데이터 안전 기능) · 설정 창 잠금 표시 |

---

## 6. 시나리오 D — 운영(Production) 접속 유형(로그인 목록 ▸ 우클릭 ▸ 유형: 운영)

| # | 조작 | 기대 |
|---|---|---|
| D1 | 프로필을 운영으로 표시 → 접속 | 탭/상태줄에 **PROD 칩** · 세션 창에도 |
| D2 | `INSERT …` 실행(자동 커밋) | **실행되지 않고** 확인 안내: "3초 안에 같은 글을 다시 실행하면 진행" → 3초 안에 재실행 = 실행 · 4초 뒤 재실행 = 다시 안내 · `SELECT`는 묻지 않음 · DDL·블록도 묻는다 |
| D3 | 수동 모드 미커밋 → `tx.prod_stale_min`(시험 1) · `tx.prod_idle_limit_min`(2) | 일반값(§5)과 **더 엄격한 쪽**이 적용 |
| D4 | 유형을 없음으로 되돌림 | 칩 사라짐 · D2 확인 없음 |

---

## 7. 시나리오 E — 세션이 여럿일 때(52 · DR-34)

| # | 조작 | 기대 |
|---|---|---|
| E1 | 공유 연결에 탭 둘 → 탭 1에서 수동 `INSERT` | **배지는 세션 단위**: 탭 1·탭 2 모두 `●1`(같은 세션) · 탭 2에서 Commit → 둘 다 0 |
| E2 | 탭 3에서 `CONNECT <같은 서버 프로필>`(전용 세션) → 수동 `INSERT` | 탭 3만 `●1` · 탭 1·2는 0(다른 세션) · 세션 창에 두 세션 · 트랜잭션 로그 창은 **전 세션 합본**(세션 선택으로 걸러 봄) |
| E3 | 탭 3 `DISCONNECT`(미커밋 상태) | 3택 · 처리 뒤 공유로 복귀 |
| E4 | `session.mode=per-editor`로 재시작 → 탭 둘 각각 수동 `INSERT` | 탭마다 세션·배지 독립 · 한 탭 Commit이 다른 탭에 영향 없음 · 유휴 닫기(`session.idle_secs`)는 **열린 트랜잭션·상태 있는 세션 제외**(`alters_session_state`) |
| E5 | 탐색기 새로고침·객체 클릭(메타 세션) | 사용자 세션의 배지·트랜잭션에 영향 없음(메타 세션은 별도 · 로그 `txlog.meta` 켜면 Meta 행) |

---

## 8. 시나리오 F — CLI `nsql run`

| # | 명령 | 기대 |
|---|---|---|
| F1 | `nsql run -c <프로필> a.sql`(a.sql = `INSERT … (1,'cli');` 한 줄) | 러너 기본 = **수동**(SQL*Plus 관용) — PG 등은 첫 문장 앞 `BEGIN` · 스크립트 끝 `commit_at_exit` = **COMMIT** → 두 번째 세션에서 1행 |
| F2 | a.sql = `INSERT … (2,'x');` + `ROLLBACK;` | 0행 추가 · 종료 코드 0 |
| F3 | a.sql = `SET AUTOCOMMIT ON` + `INSERT … (3,'y');` + 틀린 문장 | 3행은 이미 커밋 · 오류 종료 코드 · `WHENEVER SQLERROR EXIT ROLLBACK`이 있으면 그 규칙(스크립트 엔진 · 61 §1) |
| F4 | `nsql plan a.sql -d postgres` | 계획에 `BEGIN`/`COMMIT` 삽입이 보이는가(러너가 여는 문장은 실행 시점이라 계획에는 없음 — **기대 = 없음** · 문서화) |
| F5 | `nsql run … --timing`으로 F1 | 타임라인에 `commit` 단계(Linux 91차 `commit 59 ms`와 같은 자리) |

---

## 9. 기록표(복사해서 채운다 · ✅ 기대대로 · ❌ 다름(journal에 번호·증상) · — 해당 없음)

| 시나리오 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL |
|---|---|---|---|---|---|
| A1~A9 자동 커밋 | | | | | — |
| B1~B8 수동 기본(★ B6 롤백이 진짜인가) | | | | | — |
| B9 오류 뒤 트랜잭션(PG aborted) | | | | | — |
| B10~B11 DDL·TRUNCATE(암묵 커밋 vs 롤백) | | | | | — |
| B12~B13 FOR UPDATE · 사용자 BEGIN | | | — | | — |
| B14 스크립트 SET AUTOCOMMIT | | | | | — |
| B15~B18 묻는 순간(전환·닫기·해제·종료) | | | | | — |
| B19 끊김 | | | | — | — |
| B20~B22 배지·strict·off | | | | | — |
| C1~C6 L2 경고·카운트다운 | | | | | — |
| C7~C8 L3 막힘 | | | | — | — |
| C9~C10 L4 서버 안전망 · 잠금 대기 상한 | — | | | — | — |
| C11 향상 모드 | | | | | — |
| D1~D4 운영 유형 | | | | | — |
| E1~E5 세션 여럿 | | | | | — |
| F1~F5 CLI | | | | | — |

---

## 10. 자동화가 이미 덮는 것(실기에서 건너뛸 수 있는 것) · 남는 것

| 검증점 | 자동 테스트 | 실기 필요 |
|---|---|---|
| 수동 모드 첫 문장 앞 `BEGIN`(방언별 · 조건 MC/DC) | nsql-run `manual_begin_sql` 단위 · nsql-core `caps` 표 | — |
| PG 수동 모드: SELECT 뒤 `idle` · INSERT 뒤 `idle in transaction` · 롤백 실효 | integration `pg_read_only_transaction_ends_in_manual_mode`(CI · 실서버) | Oracle·MSSQL·SQLite 같은 절차(B2·B3·B6) |
| L1 판정(`tx_effect` · FOR UPDATE · 사용자 BEGIN · 커서 유지) | nsql-run `read_only_transactions_end_in_manual_mode` · `tx_effect_rules` · nsql-core `tx_control_and_locking_read` | B12·B13 서버 쪽 잠금 확인 |
| L2 시계(경고 · 재알림 · 카운트다운 · 실행하면 되돌림) | nexa-sql `tx_guard_step_mcdc` | C1~C6 카드 표시·작업 표시줄 깜빡임 |
| L3 방언별 질의·세션 id 검증 | `blockers_sql_per_dialect_and_id_validation` | C7 실제 차단 · C8 권한 없음 |
| L4 문장 | `server_guard_and_session_id_sql` | C9 서버가 끊는 것 · 끊김 경로 |
| 암묵 커밋 · TRUNCATE 판정 | nsql-core `implicit_commit` 테스트 | B10·B11 서버 확인 |
| 묻는 순간의 3택 · 배지 · 상태줄 | 일부(`count_button_rules` 류) | B15~B18 전부 실기(키·마우스) |
| Disconnect·끊김 판정 | sessions `disconnect_plan` MC/DC · 53 `live_plan` | B19 |

> 실기 결과는 journal에 `66 §9 표 + ❌ 항목의 증상`으로 남기고, ❌는 TODO에 결함 번호로 올린다(재현 절차 = 이 문서의 단계 번호).
