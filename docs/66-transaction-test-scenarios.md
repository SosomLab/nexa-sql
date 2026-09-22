# 66 · 트랜잭션 테스트 시나리오 — DBMS별로 **복사해서 그대로 실행**하는 대본

> **요청**(사용자 09-22): *"트랜잭션 자동 커밋·수동 커밋 상태에서 DBMS별로 구현된 기능을 점검하는 과정을 시나리오 문서로 만들어 순서대로 진행할 수 있게."* → **보강**(09-22): *"실제 복사해서 실행해 테스트해 볼 수 있도록 명령, 쿼리 등을 완성된 형태로 다시 구성 · 각 DBMS별로 순서대로 명령을 복사해서 테스트할 수 있도록."*
> **근거 문서**: [34 트랜잭션 UX](34-transaction-ux.md) · [44 트랜잭션 로그](44-transaction-log.md) · [56 수동 커밋 잠금 방지](56-manual-commit-lock-prevention.md) · [52 세션 모드](52-session-modes.md) · [53 접속 생존](53-connection-liveness.md) · [40 CLI 사용법](40-cli-usage.md) · [61 §1-5](61-core-design-and-working-rules.md)(**수동 커밋 = 진짜 트랜잭션** T-146).
> **쓰는 법**: §0을 한 번 하고, 자기 DBMS 절(§4 Oracle · §5 SQL Server · §6 PostgreSQL · §7 SQLite)로 간다. 그 절 안의 블록을 **위에서 아래로 복사**하면 된다. 왼쪽 창 = 우리 앱 편집기, 오른쪽 창 = 관찰 세션(sqlplus · sqlcmd · psql · sqlite3). 결과는 §9 표에.
> **표기**: 🅐 = 앱 편집기에 붙여넣고 **Ctrl+Enter로 한 문장씩** · 🅑 = 관찰 세션에 붙여넣기 · 🖱 = 마우스/키 조작(붙여넣을 것이 없다) · ▶ = 기대값.

**앱 단축키**: `Ctrl+Enter` 현재 문 실행 · `F5` 전체 실행 · `Ctrl+Alt+C` Commit · `Ctrl+Alt+R` Rollback · 상태줄의 `자동 커밋`/`수동 커밋` 세그먼트 클릭 = 모드 팝업.

> **검증(09-22)**: §0의 `nsql conn add/list/test` · `config set/get/list` · §7-4·§4-7의 CLI 대본(F1~F4)은 이 PC에서 **실제로 실행해 확인**했다(격리 홈 + SQLite 파일 · F1 = 1행·커밋 · F2 = `ROLLBACK;`으로 0행·종료 코드 0 · F3 = `SET AUTOCOMMIT ON` 뒤 오류에도 앞 행은 커밋·종료 코드 1 · F4 = 계획에 `BEGIN`/`COMMIT` 없음). 서버가 필요한 §4~§6(Oracle·SQL Server·PostgreSQL)의 SQL은 방언 문법과 우리 구현(§1)에 맞춰 적었고 **실서버 실행은 이 문서를 따라 할 때 확인**한다.

---

## 0. 준비 — 한 번만 (복사 그대로)

### 0-1. 빌드

```powershell
cd D:\Projects\kiros33\nexa-sql
cargo build --release          # target\release\nsql.exe · nexa-sql.exe
cargo build                    # 실기 테스트는 디버그 exe로(CLAUDE.md 규약)
```

### 0-2. 격리 홈 — 실제 설정·볼트를 건드리지 않는다

**Windows PowerShell**

```powershell
$env:NSQL_HOME = "$env:TEMP\nsql-tx-test\home"
$TX = "$env:TEMP\nsql-tx-test"
New-Item -ItemType Directory -Force $env:NSQL_HOME | Out-Null
$env:Path = "D:\Projects\kiros33\nexa-sql\target\release;$env:Path"
nsql conn path                 # ▶ 위에서 만든 격리 폴더가 나와야 한다
```

**macOS · Linux**

```bash
export NSQL_HOME="${TMPDIR:-/tmp}/nsql-tx-test/home"
export TX="${TMPDIR:-/tmp}/nsql-tx-test"
mkdir -p "$NSQL_HOME"
export PATH="$HOME/Projects/nexa-sql/target/release:$PATH"
nsql conn path
```

> ⚠️ **이 창에서 연 터미널에서만** `NSQL_HOME`이 산다. 앱도 **같은 창에서** 띄워야 격리 홈을 쓴다(§0-6). 끝나면 §11로 지운다.

### 0-3. 프로필 등록 — 네 개 (비밀번호는 물어본다)

```powershell
nsql conn add ORA_T  "oracle://BISCM@192.168.0.58:1521/BISCM"
nsql conn add MSS_T  "mssql://BISCM_MS@192.168.0.58:1433/M4PLAN_MS"
nsql conn add PG_T   "postgres://postgres@192.168.0.60:5433/matrixdb2"
nsql conn add LITE_T "sqlite:$env:TEMP\nsql-tx-test\tx.db"
nsql conn add ORA_PROD_T "oracle://BISCM@192.168.0.58:1521/BISCM?env=prod"   # §D 운영 유형용

nsql conn list                 # ▶ 다섯 줄 · PW 열에 ✓
nsql conn test ORA_T           # ▶ 붙는지 확인(넷 다 해 본다)
```

- 비밀번호를 스크립트로 주려면 `$env:NSQL_PASSWORD = 'pw'` 또는 `nsql conn add … --password-stdin`(첫 줄 = 비밀번호).
- **SQLite는 파일이어야 한다**(`:memory:`는 관찰 세션이 붙을 수 없다).

### 0-4. 설정 — 시험용 단축값(시간을 줄인다)

```powershell
# L2·L3·L4의 시계를 분 단위 → 1~2분으로(기본값으로는 10~30분을 기다려야 한다)
nsql config set tx.stale_min             1
nsql config set tx.remind_min            1
nsql config set tx.idle_limit_min        2
nsql config set tx.idle_countdown_secs   10
nsql config set tx.block_poll_secs       5
nsql config set tx.lock_wait_timeout_secs 5
nsql config set tx.idle_action           rollback
nsql config set tx.close_action          ask
nsql config set tx.badge                 count
nsql config set tx.read_end              auto
nsql config set session.autocommit       on
nsql config set tx.smart_commit          off
nsql config set run.prod_confirm         on
nsql config set ui.lang                  ko

# ▶ 확인 — 위 값이 그대로 보이는지(`config list`는 전체를 찍으므로 걸러 본다)
nsql config list | Select-String '^\s*tx\.'
nsql config get tx.stale_min             # ▶ 1
```

끝나고 **기본값으로 되돌리기**:

```powershell
foreach ($k in 'tx.stale_min','tx.remind_min','tx.idle_limit_min','tx.idle_countdown_secs',
               'tx.block_poll_secs','tx.lock_wait_timeout_secs','tx.idle_action','tx.close_action',
               'tx.badge','tx.read_end','tx.smart_commit','session.autocommit') { nsql config reset $k }
```

### 0-5. 관찰 세션 — DBMS별 접속 한 줄

| DBMS | 명령 |
|---|---|
| Oracle | `sqlplus BISCM@//192.168.0.58:1521/BISCM` |
| SQL Server | `sqlcmd -S 192.168.0.58,1433 -U BISCM_MS -d M4PLAN_MS` (없으면 SSMS 새 쿼리 창) |
| PostgreSQL | `psql "postgresql://postgres@192.168.0.60:5433/matrixdb2"` |
| SQLite | `sqlite3 "$env:TEMP\nsql-tx-test\tx.db"` (mac·Linux = `sqlite3 "$TX/tx.db"`) |

### 0-6. 앱 띄우기 (같은 터미널에서)

```powershell
.\target\debug\nexa-sql.exe
```

띄운 뒤 접속 창에서 `ORA_T` 등을 고른다. 화면을 특정 상태로 열고 싶으면 기동 명령을 쓴다(키 주입 금지 규약):

```powershell
$env:NSQL_STARTUP_CMD = "view.txlog"        # 트랜잭션 로그 창을 열고 시작(관찰이 쉬워진다)
.\target\debug\nexa-sql.exe
Remove-Item Env:\NSQL_STARTUP_CMD           # 다음 실행부터는 평소대로
```

- 쓸 수 있는 기동 명령: `view.txlog`(트랜잭션 로그) · `view.sessions`(세션 창) · `view.log`(로그 창) · `open:<파일 경로>` · `@connected:<명령>`(첫 접속 뒤 실행) · `@after:<ms>:<명령>`. 쉼표로 여러 개.

---

## 1. DBMS별 동작 — 우리가 구현한 것(기대값의 원천)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | 원천 |
|---|---|---|---|---|---|
| 서버가 문장마다 커밋(`server_autocommit`) | 아니오(암묵 트랜잭션) | 예 | 예 | 예 | `nsql-core caps.rs` |
| 수동 모드 첫 문장 앞에 러너가 여는 문장 | 없음 | `BEGIN TRANSACTION` | `BEGIN` | `BEGIN` | `manual_begin_sql`(T-146) |
| SELECT만 한 트랜잭션이 서버에 남나 | 없음(`FOR UPDATE` 예외) | 트랜잭션은 남음 | **`idle in transaction`** | SHARED 읽기 잠금 | [56 §1](56-manual-commit-lock-prevention.md) |
| L1 읽기 종료(`tx.read_end=auto`) | 사실상 무동작 | 트랜잭션 닫힘 | `idle`로 | 잠금 해제 | 56 L1 |
| DDL이 트랜잭션 안인가 | **아니오 — 암묵 커밋** | 예 | 예 | 예 | `Dialect::ddl_transactional` |
| TRUNCATE 롤백 | 아니오 | 예 | 예 | 예 | `truncate_transactional` |
| 오류 난 문장 뒤 | 트랜잭션 유지 | 유지 | **`aborted`** → Rollback 필요 | 유지 | 44 §2-3 |
| 막힘 감지 질의(L3) | `v$session.blocking_session` | `dm_exec_requests.blocking_session_id` | `pg_blocking_pids()` | 해당 없음 | 56 L3 |
| 접속 직후 서버 안전망(L4) | `ddl_lock_timeout` | `SET LOCK_TIMEOUT` | `idle_in_transaction_session_timeout`·`lock_timeout` | `busy_timeout` | 56 L4 |
| 앱이 서버에 알리는 이름 | (없음 — SID로 찾는다) | `application_name = nexa-sql` | `application_name = nexa-sql` | — | 드라이버 |

> ★ **핵심 검증점**: PG·SQL Server·SQLite에서 수동 모드의 DML이 **Commit 전에는 다른 세션에 보이지 않고, Rollback이면 사라져야** 한다(T-146 이전에는 "미커밋 ●"로 보이면서 이미 커밋돼 있었다). 각 절의 **B3 → B6**가 그 시험이다.

## 2. 어디를 보나 (관찰 창)

| 층 | 자동 커밋 | 수동 커밋 · 대기 n |
|---|---|---|
| 상태줄 세그먼트 | `자동 커밋` | `수동 커밋 ● n` · 클릭 = 팝업 |
| 탭 배지 | 없음 | `●n` · `tx.stale_min` 넘으면 빨강 |
| 툴바 Commit/Rollback | 비활성 | n>0이면 활성 |
| 트랜잭션 로그 창(View ▸ Transaction log) | Tx 열 `자동 커밋` | `대기` → `커밋됨 hh:mm` / `롤백됨 hh:mm`(취소선) / `암묵 커밋(CREATE)` / `읽기만 · 종료됨` / `끊김(롤백)` |
| 실행 상태 카드·토스트 | 완료/오류 | + L2 경고 카드 · L3 위험 토스트 |
| 세션 창(View ▸ Sessions) | — | 세션별 미커밋 수 |

## 3. 시나리오 여섯 — 무엇을 보는가

| 대본 | 무엇 | 걸리는 시간 |
|---|---|---|
| **A** 자동 커밋 | 문장이 곧 커밋인가 · 오류·중지·DDL·`tx.smart_commit` | 5분 |
| **B** 수동 커밋 | **진짜 트랜잭션인가**(핵심) · 3층 표시 · 오류 뒤 상태 · DDL/TRUNCATE · 묻는 순간 | 15분 |
| **C** 잠금 방지 | L2 경고·자동 처리 · L3 막힘 감지 · L4 서버 안전망 | 10분(대기 포함) |
| **D** 운영 유형 | PROD 칩 · 3초 재실행 확인 · 더 엄격한 시계 | 3분 |
| **E** 세션 여럿 | 배지가 **세션 단위**인가 · 전용 `CONNECT` · 합본 로그 | 5분 |
| **F** CLI | `nsql run`의 기본 = 수동 + 끝에 커밋 | 3분 |

---

## 4. Oracle 대본 (`ORA_T`)

### 4-0. 관찰 세션 열고 내 세션 번호 알아내기

🅑 관찰 세션

```sql
sqlplus BISCM@//192.168.0.58:1521/BISCM

SET LINESIZE 200 PAGESIZE 200
COLUMN PROGRAM FORMAT A28
COLUMN V FORMAT A12
```

🅐 앱 편집기 — **내 세션 번호**(관찰 질의에 넣을 값)

```sql
SELECT SYS_CONTEXT('USERENV','SID') AS MY_SID FROM DUAL;
```

🅑 관찰 세션 — 위에서 나온 숫자를 넣어 변수로 둔다(이후 블록이 그대로 돈다)

```sql
DEFINE APPSID = 123      -- ← 앱이 알려 준 SID로 바꾼다

-- 관찰 3종(이후 단계마다 이 블록을 다시 실행한다)
SELECT COUNT(*) AS N FROM NSQLT_TX;
SELECT sid, status, taddr, program FROM v$session WHERE sid = &APPSID;      -- taddr NOT NULL = 트랜잭션 열림
SELECT sid, blocking_session, seconds_in_wait FROM v$session WHERE blocking_session IS NOT NULL;
```

> `v$session`을 못 읽으면(권한) `GRANT SELECT_CATALOG_ROLE TO BISCM;` 또는 DBA 계정으로 관찰한다. 권한이 없으면 §C7·C8은 `—`로 기록한다.

### 4-1. 준비

🅐 앱

```sql
DROP TABLE NSQLT_TX PURGE;          -- 처음이면 ORA-00942 — 무시하고 다음 줄
DROP TABLE NSQLT_TX2 PURGE;         -- 같음
CREATE TABLE NSQLT_TX (ID NUMBER PRIMARY KEY, V VARCHAR2(40));
```

### 4-2. A · 자동 커밋 (상태줄 = `자동 커밋`)

🅐 앱 — 한 문장씩 `Ctrl+Enter`

```sql
-- A2 ▶ 1행 · 배지 없음 · 로그 Tx 열 `자동 커밋`
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'a');
-- A3 ▶ 각 1행 · 즉시 반영
UPDATE NSQLT_TX SET V = 'b' WHERE ID = 1;
DELETE FROM NSQLT_TX WHERE ID = 1;
-- A4 ▶ 결과 탭 · 트랜잭션 남지 않음
SELECT * FROM NSQLT_TX;
-- A5 ▶ 오류 카드 ORA-01722 · 배지 없음 · 다음 문장은 정상
INSERT INTO NSQLT_TX (ID, V) VALUES ('x', 'bad');
INSERT INTO NSQLT_TX (ID, V) VALUES (2, 'ok');
-- A6 ▶ 실행 중 툴바 ■ 중지 → `중지 · Fetch …` · 트랜잭션 남지 않음
SELECT o.object_name, o.object_type FROM all_objects o, all_objects o2 WHERE ROWNUM <= 2000000;
-- A7 ▶ Tx 열 `자동 커밋`(자동 모드라 `암묵 커밋(ALTER)`이 아니다)
ALTER TABLE NSQLT_TX ADD (X NUMBER);
-- A9 ▶ 정리
DELETE FROM NSQLT_TX;
```

🅑 A2·A3 사이에 관찰(3종 블록) ▶ `N=1` → `N=0` · `taddr` **NULL**(트랜잭션 없음)

🖱 **A8 스마트 커밋**: 터미널에서 `nsql config set tx.smart_commit on` → 앱 재시작 → 🅐 `INSERT INTO NSQLT_TX (ID,V) VALUES (3,'smart');` ▶ 상태줄이 **수동 커밋 ● 1**로 바뀐다 · `Ctrl+Alt+C` Commit → `수동 커밋`(자동으로 돌아가지 않는다 · DBeaver와 같음) → 상태줄 팝업으로 자동 복귀 · 끝나면 `nsql config set tx.smart_commit off`.

### 4-3. B · 수동 커밋 ★ 핵심

🖱 **B1**: 상태줄 `자동 커밋` 클릭 ▸ **수동 커밋** ▶ `수동 커밋`(n 없음) · Commit 비활성

🅐 앱

```sql
-- B2 ▶ 배지 없음 · Oracle은 애초에 트랜잭션이 열리지 않는다(Tx 열 비어 있음)
SELECT * FROM NSQLT_TX;
SELECT COUNT(*) FROM NSQLT_TX;
-- B3 ▶ 상태줄 `수동 커밋 ● 1` · 탭 `●1` · 툴바 Commit 1 · Tx 열 `대기`
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'a');
-- B4 ▶ `● 3`
INSERT INTO NSQLT_TX (ID, V) VALUES (2, 'b');
UPDATE NSQLT_TX SET V = 'c' WHERE ID = 1;
-- B5 ▶ 내 세션에는 2행이 보인다(배지는 그대로 ●3)
SELECT * FROM NSQLT_TX;
```

🅑 B3·B5 ▶ `N=0`(**보이지 않아야 한다**) · `taddr` **NOT NULL**

🖱 **B6**: 툴바 **Rollback**(`Ctrl+Alt+R`) ▶ 즉시 `수동 커밋`(n 없음) · 로그 세 행이 **`롤백됨 hh:mm`**(취소선)
🅑 ▶ `N=0` · `taddr` NULL ← **T-146의 핵심. 여기서 2행이 남으면 결함이다.**

🅐 앱

```sql
-- B7 ▶ ●1 → (Ctrl+Alt+C) → `커밋됨 hh:mm`
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'a');
-- B8 ▶ 통제 문장은 배지에 세지 않는다 · 각각 `커밋됨` / `롤백됨`
INSERT INTO NSQLT_TX (ID, V) VALUES (2, 'b');
COMMIT;
INSERT INTO NSQLT_TX (ID, V) VALUES (3, 'c');
ROLLBACK;
-- B9 ▶ 오류 행은 배지에 없음 · Oracle은 다음 문장이 **정상** 실행(●1)
INSERT INTO NSQLT_TX (ID, V) VALUES ('x', 'bad');
INSERT INTO NSQLT_TX (ID, V) VALUES (4, 'd');
-- B10 ▶ Tx 열 `암묵 커밋(CREATE)` · **대기 중이던 DML도 함께 커밋되어 n=0**
CREATE TABLE NSQLT_TX2 (A NUMBER);
-- B11 ▶ `암묵 커밋(TRUNCATE)` · Rollback해도 되돌아오지 않는다
TRUNCATE TABLE NSQLT_TX;
-- B12 ▶ L1이 끝내지 않는다(잠금이 목적 · TxEffect::UserHold) · 배지 0이지만 트랜잭션 유지
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'lock');
COMMIT;
SELECT * FROM NSQLT_TX WHERE ID = 1 FOR UPDATE;
-- B13 ▶ 사용자가 연 트랜잭션은 러너가 건드리지 않는다("이미 트랜잭션 안" 오류가 나면 결함)
SET TRANSACTION READ ONLY;
SELECT COUNT(*) FROM NSQLT_TX;
COMMIT;
-- B14 ▶ 첫 INSERT = `자동 커밋` · 둘째 = `대기 ●1` (F5 전체 실행으로)
SET AUTOCOMMIT ON
INSERT INTO NSQLT_TX (ID, V) VALUES (11, 'auto');
SET AUTOCOMMIT OFF
INSERT INTO NSQLT_TX (ID, V) VALUES (12, 'manual');
```

🅑 B12 ▶ 같은 행을 잠가 본다(대기해야 정상 · 앱에서 Rollback하면 풀린다)

```sql
UPDATE NSQLT_TX SET V = 'other' WHERE ID = 1;    -- ▶ 여기서 멈춘다(대기)
-- 앱에서 Rollback 뒤 이 문장이 진행된다 → ROLLBACK; 으로 원복
ROLLBACK;
```

🖱 **B15~B18 묻는 순간**(붙여넣을 것 없음 · `●n` 상태를 만들어 두고 한다)

| # | 조작 | ▶ 기대 |
|---|---|---|
| B15 | 상태줄 팝업 ▸ 자동 커밋으로 전환 | 3택 모달 **Commit and switch / Rollback and switch / Cancel** |
| B16 | 탭 닫기(`Ctrl+W`) | `tx.close_action=ask`면 3택 · `commit`/`rollback`으로 바꿔 각 1회 |
| B17 | 툴바 Disconnect | 같은 3택 · 해제 뒤 세션 창에서 사라짐 |
| B18 | 앱 종료 | 탭별 목록 + Commit all · Rollback all · Cancel |

🅐 **B19 끊김** — `●n`을 만든 뒤 🅑 관찰 세션에서 세션을 끊는다

```sql
-- 세션을 끊을 값 얻기(앱 SID 기준)
SELECT 'ALTER SYSTEM KILL SESSION ''' || sid || ',' || serial# || ''' IMMEDIATE;' AS CMD
  FROM v$session WHERE sid = &APPSID;
-- 위 결과 한 줄을 그대로 실행(DBA 권한 필요)
```

🅐 앱에서 아무 문장이나 실행 ▶ 끊김 판정(플러그 빨강 · Broken) · 로그 Tx 열 **`끊김(롤백)`** · 배지 0 · **자동 재접속 없음**
🅑 ▶ 행 없음(서버가 롤백)

🖱 **B20~B22 설정 바꿔 반복**: `nsql config set tx.badge dot`(그 다음 `off`)로 B3 · `nsql config set tx.read_end strict`로 B2·B12 · `off`로 B2(Oracle은 셋 다 차이가 거의 없다 — PG에서 의미가 크다).

### 4-4. C · 잠금 방지

🅐 앱(수동 모드) — 한 건 넣고 **그 세션에서 아무것도 하지 않는다**

```sql
INSERT INTO NSQLT_TX (ID, V) VALUES (9, 'idle');
```

| # | 대기·조작 | ▶ 기대 |
|---|---|---|
| C1 | 1분(`tx.stale_min=1`) | 탭 배지 **빨강** · 경고 카드 `[지금 롤백] [커밋] [연장]` · 비활성 창이면 작업 표시줄 깜빡임 |
| C2 | [연장] | 카드 사라졌다가 1분 뒤 다시 |
| C3 | 그 세션에서 아무 문장 실행 | 시계가 처음부터 다시 |
| C4 | 2분(`tx.idle_limit_min=2`) | **10초 카운트다운 카드** → 자동 롤백 · 로그 `롤백됨(자동 · 유휴 2분)` |
| C5·C6 | `nsql config set tx.idle_action warn` / `commit`으로 반복 | warn = 카드만 · commit = 자동 커밋(🅑에서 행이 보인다) |

🅑 **C7 막힘 감지** — 앱이 잠근 행을 관찰 세션이 기다리게 만든다

```sql
-- 1) 앱(수동): UPDATE NSQLT_TX SET V='x' WHERE ID=9;   ← 커밋하지 않는다
-- 2) 관찰 세션에서 같은 행을 건드려 대기 상태로 둔다
UPDATE NSQLT_TX SET V = 'other' WHERE ID = 9;     -- ▶ 멈춘 채로 둔다
```

▶ 앱: 5초 안에 위험 토스트 "내 트랜잭션 때문에 1개 세션이 기다립니다" · 상태줄 `차단 중 1` · 트랜잭션 로그 창 상단 띠(기다리는 세션·대기 시간) → Commit/Rollback하면 해소되고 🅑의 UPDATE가 진행된다 → 🅑 `ROLLBACK;`

🅐 **C10 잠금 대기 상한**(반대 방향) — 🅑이 먼저 잠그고 앱이 기다린다

```sql
-- 관찰 세션: UPDATE NSQLT_TX SET V='hold' WHERE ID=9;   ← 커밋하지 않는다
-- 앱:
UPDATE NSQLT_TX SET V = 'mine' WHERE ID = 9;
```

▶ Oracle은 DML 잠금에 상한이 없는 것이 정상(무한 대기) — `tx.lock_wait_timeout_secs`는 DDL(`ddl_lock_timeout`)에만 걸린다. 🅑에서 `ROLLBACK;`하면 앱이 진행된다. **C10은 Oracle에서 `—`**, PG·MSSQL에서 확인한다.

🖱 **C11**: `nsql config set perf.boost on` ▶ `tx.block_poll_secs`가 강제 0이 되어 L3만 멈춘다(L1·L2·L4는 그대로) · 설정 창에 잠금 표시 → `off`로 원복.

### 4-5. D · 운영 유형

🖱 접속 창에서 `ORA_PROD_T`로 접속(또는 목록 우클릭 ▸ 유형 ▸ 운영) ▶ 탭·상태줄·세션 창에 **PROD 칩**

🅐

```sql
INSERT INTO NSQLT_TX (ID, V) VALUES (21, 'prod');
```

▶ **실행되지 않고** "3초 안에 같은 글을 다시 실행하면 진행" 안내 → 3초 안에 `Ctrl+Enter` = 실행 · 4초 뒤면 다시 안내 · `SELECT * FROM NSQLT_TX;`는 묻지 않는다 · 수동 미커밋이면 `tx.prod_stale_min`(1)·`tx.prod_idle_limit_min`(2)의 **더 엄격한 쪽**이 적용.

### 4-6. E · 세션 여럿

🖱 탭 둘을 같은 공유 연결로 → 탭 1에서 🅐 `INSERT INTO NSQLT_TX (ID,V) VALUES (31,'s1');` ▶ **탭 1·2 모두 `●1`**(배지는 세션 단위) · 탭 2에서 Commit → 둘 다 0

🅐 탭 3에서 전용 세션

```sql
CONNECT ORA_T
INSERT INTO NSQLT_TX (ID, V) VALUES (32, 's3');
```

▶ 탭 3만 `●1` · 탭 1·2는 0 · 세션 창에 두 세션 · 트랜잭션 로그는 **전 세션 합본**(세션 선택으로 거른다) · 탭 3 `DISCONNECT` ▶ 3택

🖱 **E4**: `nsql config set session.mode per-editor` → 재시작 ▶ 탭마다 세션·배지 독립 · 유휴 닫기는 **열린 트랜잭션 세션 제외** → `nsql config reset session.mode`

### 4-7. F · CLI

```powershell
# F1 — 러너 기본은 수동(SQL*Plus 관용)이고 스크립트 끝에서 커밋한다(commit_at_exit)
Set-Content "$TX\f1.sql" "INSERT INTO NSQLT_TX (ID,V) VALUES (101,'cli');"
nsql run -c ORA_T "$TX\f1.sql" --timing          # ▶ 타임라인에 commit 단계
"SELECT COUNT(*) FROM NSQLT_TX WHERE ID=101;" | nsql run -c ORA_T -      # ▶ 1

# F2 — 스크립트가 ROLLBACK으로 끝나면 남지 않는다
Set-Content "$TX\f2.sql" "INSERT INTO NSQLT_TX (ID,V) VALUES (102,'x');"
Add-Content "$TX\f2.sql" "ROLLBACK;"
nsql run -c ORA_T "$TX\f2.sql"                   # ▶ 종료 코드 0($LASTEXITCODE)
"SELECT COUNT(*) FROM NSQLT_TX WHERE ID=102;" | nsql run -c ORA_T -      # ▶ 0

# F3 — 스크립트 안의 SET AUTOCOMMIT이 그 실행에 우선한다
Set-Content "$TX\f3.sql" "SET AUTOCOMMIT ON"
Add-Content "$TX\f3.sql" "INSERT INTO NSQLT_TX (ID,V) VALUES (103,'y');"
Add-Content "$TX\f3.sql" "INSERT INTO NSQLT_TX (ID,V) VALUES ('bad','z');"
nsql run -c ORA_T "$TX\f3.sql"                   # ▶ 103은 이미 커밋 · 마지막 줄은 오류 · 종료 코드 ≠ 0
"SELECT COUNT(*) FROM NSQLT_TX WHERE ID=103;" | nsql run -c ORA_T -      # ▶ 1

# F4 — 접속 없이 계획만(러너가 여는 BEGIN/COMMIT은 실행 시점이라 계획에 없는 것이 정상)
nsql plan "$TX\f1.sql" -d oracle
```

### 4-8. 정리

🅐 `DROP TABLE NSQLT_TX PURGE;` · `DROP TABLE NSQLT_TX2 PURGE;`

---

## 5. SQL Server 대본 (`MSS_T`)

### 5-0. 관찰 세션

```powershell
sqlcmd -S 192.168.0.58,1433 -U BISCM_MS -d M4PLAN_MS
```

🅑 관찰 3종 — **`application_name`으로 우리 앱 세션을 바로 찾는다**

```sql
SELECT COUNT(*) AS N FROM NSQLT_TX;
SELECT session_id, program_name, open_transaction_count, status
  FROM sys.dm_exec_sessions WHERE program_name = 'nexa-sql';
SELECT session_id, blocking_session_id, wait_type, wait_time
  FROM sys.dm_exec_requests WHERE blocking_session_id <> 0;
GO
```

### 5-1. 준비

🅐 앱

```sql
DROP TABLE IF EXISTS NSQLT_TX;
DROP TABLE IF EXISTS NSQLT_TX2;
CREATE TABLE NSQLT_TX (ID INT PRIMARY KEY, V NVARCHAR(40));
```

### 5-2. A · 자동 커밋

```sql
-- A2 ▶ 1행 · 배지 없음 · 🅑 N=1 · open_transaction_count = 0
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'a');
-- A3
UPDATE NSQLT_TX SET V = 'b' WHERE ID = 1;
DELETE FROM NSQLT_TX WHERE ID = 1;
-- A4
SELECT * FROM NSQLT_TX;
-- A5 ▶ 오류(Msg 245 변환 실패) · 다음 문장 정상
INSERT INTO NSQLT_TX (ID, V) VALUES ('x', 'bad');
INSERT INTO NSQLT_TX (ID, V) VALUES (2, 'ok');
-- A6 ▶ 실행 중 ■ 중지(Attention 전송 · 44 §6)
SELECT TOP 5000000 a.name AS n1, b.name AS n2 FROM sys.all_columns a CROSS JOIN sys.all_columns b;
-- A7
ALTER TABLE NSQLT_TX ADD X INT;
-- A9
DELETE FROM NSQLT_TX;
```

### 5-3. B · 수동 커밋 ★

🖱 상태줄 ▸ 수동 커밋

```sql
-- B2 ▶ 로그 Tx 열 `읽기만 · 종료됨`(L1) · 🅑 open_transaction_count = 0
SELECT * FROM NSQLT_TX;
SELECT COUNT(*) FROM NSQLT_TX;
-- B3 ▶ ●1 · 🅑 N=0 · open_transaction_count = 1
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'a');
-- B4 ▶ ●3
INSERT INTO NSQLT_TX (ID, V) VALUES (2, 'b');
UPDATE NSQLT_TX SET V = 'c' WHERE ID = 1;
-- B5 ▶ 내게는 2행
SELECT * FROM NSQLT_TX;
```

🖱 **B6 Rollback**(`Ctrl+Alt+R`) ▶ 🅑 `N=0` · `open_transaction_count = 0` ← **핵심 검증점**

```sql
-- B7 ▶ Commit 뒤 🅑 N=1
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'a');
-- B8
INSERT INTO NSQLT_TX (ID, V) VALUES (2, 'b');
COMMIT;
INSERT INTO NSQLT_TX (ID, V) VALUES (3, 'c');
ROLLBACK;
-- B9 ▶ 오류 뒤에도 트랜잭션 유지(PG와 다르다) · 둘째 문장 정상 ●1
INSERT INTO NSQLT_TX (ID, V) VALUES ('x', 'bad');
INSERT INTO NSQLT_TX (ID, V) VALUES (4, 'd');
-- B10 ▶ DDL도 트랜잭션 안 · ●n+1 → Rollback → 🅑에서 표가 사라진다
CREATE TABLE NSQLT_TX2 (A INT);
-- B11 ▶ TRUNCATE도 롤백된다(행 복구)
TRUNCATE TABLE NSQLT_TX;
-- B12 ▶ 잠금 유지(UserHold) · 배지 0이지만 트랜잭션 남음
INSERT INTO NSQLT_TX (ID, V) VALUES (1, 'lock');
COMMIT;
SELECT * FROM NSQLT_TX WITH (UPDLOCK) WHERE ID = 1;
-- B13 ▶ 사용자가 연 트랜잭션(러너의 BEGIN이 겹치면 안 된다)
BEGIN TRANSACTION;
SELECT COUNT(*) FROM NSQLT_TX;
COMMIT;
-- B14 (F5 전체 실행)
SET AUTOCOMMIT ON
INSERT INTO NSQLT_TX (ID, V) VALUES (11, 'auto');
SET AUTOCOMMIT OFF
INSERT INTO NSQLT_TX (ID, V) VALUES (12, 'manual');
```

🅑 **B19 끊김**

```sql
SELECT 'KILL ' + CAST(session_id AS VARCHAR(10)) + ';' AS CMD
  FROM sys.dm_exec_sessions WHERE program_name = 'nexa-sql';
GO
-- 위 결과를 실행(sysadmin 또는 ALTER ANY CONNECTION 권한)
```

▶ 앱: 다음 실행에서 끊김 판정 · Tx 열 `끊김(롤백)` · 🅑 행 없음

### 5-4. C · 잠금 방지

🅐 `INSERT INTO NSQLT_TX (ID, V) VALUES (9, 'idle');` (커밋하지 않음) → C1~C6은 §4-4와 같다.

🅑 **C7**

```sql
-- 앱(수동): UPDATE NSQLT_TX SET V='x' WHERE ID=9;  ← 커밋 안 함
BEGIN TRANSACTION;
UPDATE NSQLT_TX SET V = 'other' WHERE ID = 9;   -- ▶ 대기
-- 앱에서 Commit/Rollback 뒤 진행 → ROLLBACK;
GO
```

🅐 **C10 잠금 대기 상한**(`tx.lock_wait_timeout_secs=5`)

```sql
-- 관찰 세션: BEGIN TRANSACTION; UPDATE NSQLT_TX SET V='hold' WHERE ID=9;   ← 커밋 안 함
UPDATE NSQLT_TX SET V = 'mine' WHERE ID = 9;
```

▶ 5초 뒤 **Msg 1222 Lock request time out** · 42 정규화 분류 `Lock` 토스트 → 🅑 `ROLLBACK;`

### 5-5. D·E·F

- **D**: `nsql conn add MSS_PROD_T "mssql://BISCM_MS@192.168.0.58:1433/M4PLAN_MS?env=prod"` → 접속 → §4-5와 같다.
- **E**: §4-6에서 `CONNECT ORA_T` → `CONNECT MSS_T`로 바꾼다.
- **F**:

```powershell
Set-Content "$TX\f1.sql" "INSERT INTO NSQLT_TX (ID,V) VALUES (101,'cli');"
nsql run -c MSS_T "$TX\f1.sql" --timing
"SELECT COUNT(*) FROM NSQLT_TX WHERE ID=101;" | nsql run -c MSS_T -   # ▶ 1
```

### 5-6. 정리

🅐 `DROP TABLE IF EXISTS NSQLT_TX;` · `DROP TABLE IF EXISTS NSQLT_TX2;`

---

## 6. PostgreSQL 대본 (`PG_T`) — L1·L4가 가장 잘 드러나는 곳

### 6-0. 관찰 세션

```bash
psql "postgresql://postgres@192.168.0.60:5433/matrixdb2"
```

🅑 관찰 3종 — 마지막 줄의 `\watch 2`를 붙이면 2초마다 저절로 다시 돈다

```sql
SELECT count(*) AS n FROM nsqlt_tx;
SELECT pid, application_name, state, xact_start, left(query, 40) AS q
  FROM pg_stat_activity WHERE application_name = 'nexa-sql';
SELECT pid, pg_blocking_pids(pid) AS blockers
  FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0;
```

```sql
-- 상태만 계속 지켜보기(Ctrl+C로 멈춤)
SELECT state, xact_start FROM pg_stat_activity WHERE application_name = 'nexa-sql' \watch 2
```

### 6-1. 준비

🅐 앱

```sql
DROP TABLE IF EXISTS nsqlt_tx;
DROP TABLE IF EXISTS nsqlt_tx2;
CREATE TABLE nsqlt_tx (id INT PRIMARY KEY, v VARCHAR(40));
```

### 6-2. A · 자동 커밋

```sql
-- A2 ▶ 🅑 n=1 · state = idle (idle in transaction 아님)
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'a');
-- A3
UPDATE nsqlt_tx SET v = 'b' WHERE id = 1;
DELETE FROM nsqlt_tx WHERE id = 1;
-- A4 ▶ 🅑 state = idle
SELECT * FROM nsqlt_tx;
-- A5 ▶ 오류 22P02 · **자동 모드는 문장마다 새 트랜잭션이라 다음 문장이 정상**
INSERT INTO nsqlt_tx (id, v) VALUES ('x', 'bad');
INSERT INTO nsqlt_tx (id, v) VALUES (2, 'ok');
-- A6 ▶ 실행 중 ■ 중지
SELECT g, md5(g::text) FROM generate_series(1, 20000000) g;
-- A7
ALTER TABLE nsqlt_tx ADD COLUMN x INT;
-- A9
DELETE FROM nsqlt_tx;
```

### 6-3. B · 수동 커밋 ★

🖱 상태줄 ▸ 수동 커밋

```sql
-- B2 ★ L1의 목적 ▶ 로그 Tx 열 `읽기만 · 종료됨` · 🅑 state = **idle**(idle in transaction이면 결함)
SELECT * FROM nsqlt_tx;
SELECT count(*) FROM nsqlt_tx;
-- B3 ▶ ●1 · 🅑 n=0 · state = **idle in transaction**
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'a');
-- B4 ▶ ●3
INSERT INTO nsqlt_tx (id, v) VALUES (2, 'b');
UPDATE nsqlt_tx SET v = 'c' WHERE id = 1;
-- B5
SELECT * FROM nsqlt_tx;
```

🖱 **B6 Rollback** ▶ 🅑 `n=0` · `state=idle` ← **핵심 검증점**

```sql
-- B7
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'a');
-- B8
INSERT INTO nsqlt_tx (id, v) VALUES (2, 'b');
COMMIT;
INSERT INTO nsqlt_tx (id, v) VALUES (3, 'c');
ROLLBACK;
-- B9 ★ PG 고유 ▶ 둘째 문장이 `25P02 current transaction is aborted`
--    → 상태줄/카드가 Rollback을 권한다 · Rollback 뒤 정상
INSERT INTO nsqlt_tx (id, v) VALUES ('x', 'bad');
INSERT INTO nsqlt_tx (id, v) VALUES (4, 'd');
-- B10 ▶ DDL도 트랜잭션 안 · Rollback하면 표가 사라진다
CREATE TABLE nsqlt_tx2 (a INT);
-- B11 ▶ TRUNCATE도 롤백된다
TRUNCATE TABLE nsqlt_tx;
-- B12 ▶ UserHold — 배지 0이어도 트랜잭션 유지 · 🅑 idle in transaction
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'lock');
COMMIT;
SELECT * FROM nsqlt_tx WHERE id = 1 FOR UPDATE;
-- B13
BEGIN;
SELECT count(*) FROM nsqlt_tx;
COMMIT;
-- B14 (F5)
SET AUTOCOMMIT ON
INSERT INTO nsqlt_tx (id, v) VALUES (11, 'auto');
SET AUTOCOMMIT OFF
INSERT INTO nsqlt_tx (id, v) VALUES (12, 'manual');
```

🅑 **B19 끊김**

```sql
SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = 'nexa-sql';
```

▶ 앱: 다음 실행에서 끊김 판정 · `끊김(롤백)` · 자동 재접속 없음

🖱 **B22**: `nsql config set tx.read_end off` → B2 반복 ▶ 🅑가 **`idle in transaction`으로 남는다**(종전 동작) → `nsql config set tx.read_end auto`로 원복. `strict`로도 한 번(서버 탐침 `pg_current_xact_id_if_assigned()`가 개발자 모드 로그에 보인다).

### 6-4. C · 잠금 방지 — L3·L4 둘 다 되는 유일한 곳

🅐 `INSERT INTO nsqlt_tx (id, v) VALUES (9, 'idle');`(커밋 안 함) → C1~C6은 §4-4와 같다.

🅑 **C7**

```sql
-- 앱(수동): UPDATE nsqlt_tx SET v='x' WHERE id=9;   ← 커밋 안 함
BEGIN;
UPDATE nsqlt_tx SET v = 'other' WHERE id = 9;    -- ▶ 대기
-- 앱에서 Commit/Rollback 뒤 진행 → ROLLBACK;
```

🅐 **C9 L4 서버 안전망**(PG만)

```powershell
nsql config set tx.server_idle_timeout_secs 20     # 그리고 앱 재접속
```

```sql
INSERT INTO nsqlt_tx (id, v) VALUES (10, 'l4');
-- 25초 기다린 뒤 아래를 실행
SELECT 1;
```

▶ 접속 직후 개발자 로그에 `SET idle_in_transaction_session_timeout = '20s'` · 25초 뒤 서버가 세션을 끊음 → 끊김 판정 · `끊김(롤백)` → `nsql config reset tx.server_idle_timeout_secs`

🅐 **C10 잠금 대기 상한**

```sql
-- 관찰 세션: BEGIN; UPDATE nsqlt_tx SET v='hold' WHERE id=9;   ← 커밋 안 함
UPDATE nsqlt_tx SET v = 'mine' WHERE id = 9;
```

▶ 5초 뒤 `55P03 lock_not_available`(또는 `57014`) · `Lock` 분류 토스트 → 🅑 `ROLLBACK;`

### 6-5. D·E·F

- **D**: `nsql conn add PG_PROD_T "postgres://postgres@192.168.0.60:5433/matrixdb2?env=prod"`
- **E**: §4-6에서 `CONNECT PG_T`
- **F**:

```powershell
Set-Content "$TX\f1.sql" "INSERT INTO nsqlt_tx (id,v) VALUES (101,'cli');"
nsql run -c PG_T "$TX\f1.sql" --timing
"SELECT count(*) FROM nsqlt_tx WHERE id=101;" | nsql run -c PG_T -    # ▶ 1
nsql plan "$TX\f1.sql" -d postgres                                    # ▶ BEGIN/COMMIT은 계획에 없다
```

### 6-6. 정리

🅐 `DROP TABLE IF EXISTS nsqlt_tx;` · `DROP TABLE IF EXISTS nsqlt_tx2;`

---

## 7. SQLite 대본 (`LITE_T`) — 파일 하나 · 잠금이 전부

### 7-0. 관찰 세션

```powershell
sqlite3 "$env:TEMP\nsql-tx-test\tx.db"
```

```sql
.mode box
.headers on
SELECT count(*) AS n FROM nsqlt_tx;
PRAGMA journal_mode;      -- delete(롤백 저널)면 쓰기 중 읽기도 막힐 수 있다 · wal이면 읽기는 자유
-- 앱이 쓰기 트랜잭션을 쥐고 있는지 확인하는 방법(상태 뷰가 없다)
BEGIN IMMEDIATE;          -- ▶ "Error: database is locked" = 앱이 쥐고 있다
ROLLBACK;                 -- 위가 성공했을 때만
```

### 7-1. 준비

🅐 앱

```sql
DROP TABLE IF EXISTS nsqlt_tx;
DROP TABLE IF EXISTS nsqlt_tx2;
CREATE TABLE nsqlt_tx (id INTEGER PRIMARY KEY, v TEXT);
```

### 7-2. A · 자동 커밋

```sql
-- A2 ▶ 🅑 n=1 즉시
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'a');
-- A3
UPDATE nsqlt_tx SET v = 'b' WHERE id = 1;
DELETE FROM nsqlt_tx WHERE id = 1;
-- A4
SELECT * FROM nsqlt_tx;
-- A5 ▶ datatype mismatch 오류 · 다음 문장 정상
INSERT INTO nsqlt_tx (id, v) VALUES ('x', 'bad');
INSERT INTO nsqlt_tx (id, v) VALUES (2, 'ok');
-- A6 ▶ 실행 중 ■ 중지
WITH RECURSIVE g(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM g WHERE i < 20000000)
SELECT i, hex(randomblob(8)) FROM g;
-- A7
ALTER TABLE nsqlt_tx ADD COLUMN x INT;
-- A9
DELETE FROM nsqlt_tx;
```

### 7-3. B · 수동 커밋 ★

🖱 상태줄 ▸ 수동 커밋

```sql
-- B2 ▶ Tx 열 `읽기만 · 종료됨` · 🅑 BEGIN IMMEDIATE가 **성공**해야 한다(읽기 잠금이 풀렸다)
SELECT * FROM nsqlt_tx;
-- B3 ▶ ●1 · 🅑 n=0 · BEGIN IMMEDIATE = database is locked
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'a');
-- B4 ▶ ●3
INSERT INTO nsqlt_tx (id, v) VALUES (2, 'b');
UPDATE nsqlt_tx SET v = 'c' WHERE id = 1;
-- B5
SELECT * FROM nsqlt_tx;
```

🖱 **B6 Rollback** ▶ 🅑 `n=0` · `BEGIN IMMEDIATE` 성공 ← **핵심 검증점**

```sql
-- B7 → Commit ▶ 🅑 n=1
INSERT INTO nsqlt_tx (id, v) VALUES (1, 'a');
-- B8
INSERT INTO nsqlt_tx (id, v) VALUES (2, 'b');
COMMIT;
INSERT INTO nsqlt_tx (id, v) VALUES (3, 'c');
ROLLBACK;
-- B9 ▶ 오류 뒤에도 트랜잭션 유지 · 둘째 문장 정상
INSERT INTO nsqlt_tx (id, v) VALUES ('x', 'bad');
INSERT INTO nsqlt_tx (id, v) VALUES (4, 'd');
-- B10 ▶ DDL도 트랜잭션 안 · Rollback하면 표가 사라진다
CREATE TABLE nsqlt_tx2 (a INT);
-- B11 ▶ SQLite에 TRUNCATE는 없다 — DELETE로 대신하고 Rollback으로 복구
DELETE FROM nsqlt_tx;
-- B13
BEGIN;
SELECT count(*) FROM nsqlt_tx;
COMMIT;
-- B14 (F5)
SET AUTOCOMMIT ON
INSERT INTO nsqlt_tx (id, v) VALUES (11, 'auto');
SET AUTOCOMMIT OFF
INSERT INTO nsqlt_tx (id, v) VALUES (12, 'manual');
```

- **B12**(`FOR UPDATE`)·**B19**(세션 KILL)·**C7~C9**(막힘 감지·서버 안전망)은 SQLite에 없다 → `—`로 기록.
- **C10**: 관찰 세션이 `BEGIN IMMEDIATE;`로 파일을 쥔 채 두면 앱의 쓰기가 `database is locked`(드라이버 `busy_timeout`) → 🅑 `ROLLBACK;`.
- **C1~C6**(L2 시계)은 SQLite에서도 그대로 돈다(시간 기반이라 DBMS와 무관).

### 7-4. F · CLI

```powershell
Set-Content "$TX\f1.sql" "INSERT INTO nsqlt_tx (id,v) VALUES (101,'cli');"
nsql run -c LITE_T "$TX\f1.sql" --timing
"SELECT count(*) FROM nsqlt_tx WHERE id=101;" | nsql run -c LITE_T -  # ▶ 1
```

### 7-5. 정리

🅐 `DROP TABLE IF EXISTS nsqlt_tx;` · `DROP TABLE IF EXISTS nsqlt_tx2;`

---

## 8. 한 번에 훑는 빠른 점검(시간이 없을 때 · DBMS마다 3분)

```sql
-- ① 자동 모드에서 넣고(🅑 즉시 보임) ② 수동으로 바꿔 넣고(🅑 안 보임) ③ 롤백(🅑 그대로 0) ④ 다시 넣고 커밋(🅑 보임)
INSERT INTO nsqlt_tx (id, v) VALUES (91, 'auto');     -- 🖱 자동 커밋 상태
-- 🖱 상태줄 ▸ 수동 커밋
INSERT INTO nsqlt_tx (id, v) VALUES (92, 'manual');   -- ▶ ●1 · 🅑 91만 보임
-- 🖱 Ctrl+Alt+R (Rollback)                            -- ▶ 🅑 여전히 91만  ← T-146
INSERT INTO nsqlt_tx (id, v) VALUES (93, 'manual2');
-- 🖱 Ctrl+Alt+C (Commit)                              -- ▶ 🅑 91, 93
DELETE FROM nsqlt_tx WHERE id IN (91, 92, 93);
-- 🖱 Ctrl+Alt+C
```

Oracle은 표·컬럼 이름을 대문자(`NSQLT_TX`)로 바꿔 쓴다.

---

## 9. 기록표(복사해서 채운다 · ✅ 기대대로 · ❌ 다름(journal에 번호·증상) · — 해당 없음)

| 시나리오 | Oracle | SQL Server | PostgreSQL | SQLite |
|---|---|---|---|---|
| A2~A4 자동 커밋 기본 | | | | |
| A5 오류 뒤 다음 문장 | | | | |
| A6 중지 · A7 DDL | | | | |
| A8 `tx.smart_commit` | | | | |
| B2 읽기만(L1) | | | | |
| B3~B5 대기 · 안 보임 | | | | |
| **B6 롤백이 진짜인가(★)** | | | | |
| B7~B8 커밋·문장 통제 | | | | |
| B9 오류 뒤 트랜잭션(PG aborted) | | | | |
| B10~B11 DDL·TRUNCATE | | | | — |
| B12~B13 FOR UPDATE · 사용자 BEGIN | | | | — |
| B14 스크립트 SET AUTOCOMMIT | | | | |
| B15~B18 묻는 순간 | | | | |
| B19 끊김 | | | | — |
| B20~B22 배지·strict·off | | | | |
| C1~C6 L2 경고·카운트다운 | | | | |
| C7~C8 L3 막힘 | | | | — |
| C9 L4 서버 유휴 타임아웃 | — | | | — |
| C10 잠금 대기 상한 | — | | | |
| C11 향상 모드 | | | | |
| D1~D4 운영 유형 | | | | |
| E1~E5 세션 여럿 | | | | |
| F1~F5 CLI | | | | |

## 10. 자동화가 이미 덮는 것 · 실기로만 되는 것

| 검증점 | 자동 테스트 | 실기 필요 |
|---|---|---|
| 수동 모드 첫 문장 앞 `BEGIN`(방언별) | nsql-run `manual_begin_sql` · nsql-core `caps` | — |
| PG 수동: SELECT 뒤 `idle` · INSERT 뒤 `idle in transaction` · 롤백 실효 | integration `pg_read_only_transaction_ends_in_manual_mode`(CI · 실서버) | Oracle·MSSQL·SQLite의 같은 절차(B2·B3·B6) |
| L1 판정(`tx_effect` · FOR UPDATE · 사용자 BEGIN) | nsql-run `read_only_transactions_end_in_manual_mode` · `tx_effect_rules` | B12·B13 서버 쪽 잠금 확인 |
| L2 시계(경고·재알림·카운트다운) | nexa-sql `tx_guard_step_mcdc` | C1~C6 카드·작업 표시줄 깜빡임 |
| L3 방언별 질의·세션 id 검증 | `blockers_sql_per_dialect_and_id_validation` | C7 실제 차단 · C8 권한 없음 |
| L4 문장 | `server_guard_and_session_id_sql` | C9 서버가 끊는 것 |
| 암묵 커밋·TRUNCATE 판정 | nsql-core `implicit_commit` | B10·B11 서버 확인 |
| 묻는 순간의 3택·배지·상태줄 | 일부 | B15~B18 전부 실기 |
| Disconnect·끊김 판정 | `disconnect_plan` MC/DC · 53 `live_plan` | B19 |

## 11. 끝내고 치우기

```powershell
# 서버의 시험 표는 각 절의 "정리" 블록으로 이미 지웠다고 보고, 로컬만 정리한다
nsql conn rm ORA_T; nsql conn rm MSS_T; nsql conn rm PG_T; nsql conn rm LITE_T
nsql conn rm ORA_PROD_T
Remove-Item -Recurse -Force "$env:TEMP\nsql-tx-test"
Remove-Item Env:\NSQL_HOME
```

> 실기 결과는 journal에 `66 §9 표 + ❌ 항목의 증상`으로 남기고, ❌는 TODO에 결함 번호로 올린다(재현 절차 = 이 문서의 단계 번호).
