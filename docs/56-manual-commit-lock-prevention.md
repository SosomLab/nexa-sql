# 56 · 수동 커밋 모드의 테이블 잠금 방지 — 다른 도구 조사 · 설계 방안 (사용자 요청 09-19)

> **요구**: "수동 커밋 모드에서 테이블 잠금을 방지하기 위해 다른 앱들은 어떤 구조로 설계되었는지 조사하고 설계 방안을 알려 달라 · 확인 후 개발".
> **상태**: 📐 조사·설계 → 🚧 개발(T-137) — D-100·101·102·105 확정(09-19) · D-103·104와 세부 조건은 개발 전 재확인 · **자동 처리의 값·조건은 전부 설정으로**(사용자 09-19). 관련: [34 트랜잭션 UX](34-transaction-ux.md) · [44 트랜잭션 로그](44-transaction-log.md) · [52 세션 모드](52-session-modes.md) · [53 접속 생존](53-connection-liveness.md) · [26 §8 네트워크 부하 규칙](26-performance-architecture.md).

## 0. 결론 여섯 줄

1. 잠금 사고의 원인은 둘이다: **(A) 변경(DML) 뒤 커밋을 잊음**(행·테이블 잠금이 남아 다른 세션이 기다림) · **(B) 조회만 했는데도 트랜잭션이 열려 있음**(PostgreSQL·MySQL·SQL Server·SQLite — 읽기 잠금·메타데이터 잠금·스냅샷이 DDL·VACUUM·쓰기를 막음. Oracle은 해당 없음).
2. 다른 도구의 해법은 다섯 갈래로 모인다: ① 트랜잭션을 **늦게 연다**(DBeaver 스마트 커밋 — 첫 DML에서만 수동으로) ② **유휴 트랜잭션을 자동으로 끝낸다**(DBeaver: 접속 유형별 10/15/30분 뒤 롤백) ③ **보이게 한다**(대기 문장 수·미커밋 표시·트랜잭션 로그) ④ **잃는 순간에 묻는다**(닫기·해제·종료 — DbVisualizer·Toad·SSMS·DataGrip) ⑤ **서버에 맡긴다**(PG `idle_in_transaction_session_timeout` · Oracle `MAX_IDLE_BLOCKER_TIME` — DBA 몫).
3. 우리는 ③④가 이미 있다(탭 배지·상태줄·툴바 배지·트랜잭션 로그 창·닫기/해제/종료 가드 · `tx.stale_min` 빨강). **빠진 것은 (B)의 구조적 해결과 ②의 자동 종료, 그리고 "내가 지금 누군가를 막고 있다"는 사실의 고지**다.
4. 현재 구조의 약점: 러너는 세션을 **항상 트랜잭션 안**에 두고 "자동 커밋 = 문장 뒤 `commit()`"으로 구현한다(`nsql-run` · PG 드라이버는 문장 앞 `BEGIN`). 그래서 수동 모드에서는 **SELECT 한 번만 해도** 트랜잭션이 열린 채 남는다 — (B)가 그대로 발생한다.
5. 권장 설계 = **네 겹**: **L1 읽기 트랜잭션 자동 종료**(수동 모드라도 변경이 없던 트랜잭션은 결과를 다 받은 뒤 조용히 끝낸다) → **L2 유휴 미커밋 경고·자동 종료**(설정 · 카운트다운 · 기본 = 경고만) → **L3 막힘 감지**(미커밋이 있는 동안만 · 저빈도 · 메타 세션으로 "n개 세션이 기다리는 중") → **L4 서버 안전망 세션 파라미터**(선택 · 접속 직후 `SET`).
6. 모두 설정으로 끄고 켤 수 있고(30 §1 · 39 §3), 접속 유형(개발/시험/운영) 프리셋은 2차. 자동 **커밋**은 기본으로 두지 않는다(의도하지 않은 확정이 롤백보다 위험) — 자동 종료의 기본 동작은 "경고만", 선택하면 "롤백".

---

## 1. 무엇이 잠기나 — DBMS별(수동 커밋 · 트랜잭션이 열린 채 유휴)

| DBMS | 조회(SELECT)만 한 트랜잭션이 쥐는 것 | 변경(DML) 뒤 미커밋이 쥐는 것 | 남이 겪는 증상 | 서버 쪽 안전망 |
|---|---|---|---|---|
| **Oracle** | 없음(일반 SELECT는 트랜잭션을 시작하지 않는다 · 예외 = `FOR UPDATE` · DB 링크 SELECT · 직렬화 격리) | 행 잠금(TX) + 테이블 RX 잠금(TM) | 같은 행 UPDATE/DELETE 대기 · 그 테이블 DDL이 `ORA-00054`(또는 `DDL_LOCK_TIMEOUT` 대기) | `MAX_IDLE_BLOCKER_TIME`(19c+ · **남을 막고 있는 유휴 세션만** n분 뒤 종료) · 프로파일 `IDLE_TIME` · Resource Manager |
| **PostgreSQL** | `ACCESS SHARE` 테이블 잠금 + 스냅샷(xmin) — 상태 `idle in transaction` | 행 잠금 + `ROW EXCLUSIVE` | `ALTER TABLE`·`DROP`·`TRUNCATE`·`VACUUM FULL` 대기(그 뒤에 선 **모든 조회도 줄줄이 대기**) · VACUUM이 죽은 튜플을 못 치움(팽창) | `idle_in_transaction_session_timeout`(9.6+ · 세션 단위 `SET` 가능) · `lock_timeout`(기다리는 쪽) |
| **MySQL/MariaDB**(InnoDB) | 메타데이터 잠금(MDL) + 일관 스냅샷 | 행 잠금(갭 잠금 포함) | `ALTER TABLE`이 "Waiting for table metadata lock" · 뒤따르는 조회도 대기 | `innodb_lock_wait_timeout`·`lock_wait_timeout`(기다리는 쪽) · `wait_timeout`(유휴 접속) — 유휴 **트랜잭션** 전용 타임아웃은 없음 |
| **SQL Server** | 암묵 트랜잭션(`IMPLICIT_TRANSACTIONS ON`)이면 SELECT도 트랜잭션을 연다 — READ COMMITTED에선 공유 잠금은 문장 뒤 풀리지만 트랜잭션은 남는다(로그 절단 방해 · 격리 수준이 높으면 공유 잠금 유지) | 행/페이지/테이블 X 잠금 | 다른 세션의 조회·변경이 **차단(blocking)** — RCSI가 아니면 조회도 막힌다 | 기다리는 쪽 `SET LOCK_TIMEOUT` · 유휴 트랜잭션 종료 기능 없음(DBA가 `KILL`) |
| **SQLite** | 읽기 잠금(SHARED) — 롤백 저널 모드면 쓰기 차단 | RESERVED/EXCLUSIVE | 다른 접속의 쓰기가 `database is locked`(`busy_timeout` 뒤 실패) · WAL이면 읽기는 무관 | `busy_timeout`(기다리는 쪽) |

> 핵심: **Oracle만 "조회는 공짜"** 다. 나머지 넷은 수동 모드에서 SELECT 한 번이 곧 "열린 트랜잭션"이고, DBeaver·DataGrip 이슈 트래커에도 이 증상이 반복 보고된다(DBeaver #7402 "쿼리 창을 닫아도 PG 트랜잭션을 쥐고 있다" · DataGrip DBE-10935 "수동 모드는 필요 없는데도 커밋/롤백을 요구한다" · Toad KB 4226590 "SELECT만 했는데 커밋을 묻는다 = DB 링크").

## 2. 다른 도구는 어떻게 하나

| 도구 | 트랜잭션을 여는 시점 | 유휴 미커밋 처리 | 표시 | 잃는 순간 | 비고 |
|---|---|---|---|---|---|
| **DBeaver** | 자동/수동 토글(접속 단위) · **스마트 커밋**: 자동 모드에서 SELECT는 트랜잭션 없이 · **첫 변경 문장 직전에 수동으로 전환** · "트랜잭션 끝나면 자동으로 복귀" 옵션 | **"Automatically end long idle transactions(초)"** — 접속 유형 기본: 개발 1800 · 시험 900 · 운영 600초 · 시간이 되면 **롤백하고 닫는다**(사용자 조작 불필요) · 유휴 접속도 4h/2h/1h 뒤 닫음 | 툴바 대기 문장 수 · Pending Transactions 창(전 접속) · Transaction Log · 종료 알림 | 해제·종료 시 확인 | 접속 유형(개발/시험/운영)이 기본값 묶음 — 운영 = 수동 커밋 + 실행 확인 |
| **DataGrip** | 콘솔 단위 `Tx: Auto / Manual` · 격리 수준 선택 | 도구 기능 없음 — 커뮤니티 권고는 서버 `idle_in_transaction_session_timeout` | 콘솔 탭 표시 · Commit/Rollback 버튼 활성 | 콘솔 닫기 시 확인 | 데이터 편집기는 "제출"과 "커밋"을 분리(제출 = 서버로 · 커밋 = 확정) |
| **DbVisualizer** | 접속 속성 Auto Commit ON/OFF | 없음 | 상태줄 `Auto-Commit: OFF` + **갱신 행 수 / 실행한 비조회 문장 수**(마지막 커밋 이후) | "Pending Transactions at Disconnect" = Commit/Rollback/Ask · "Ask when Auto Commit is OFF" = Always / **When Uncommitted Updates**(갱신이 있을 때만 묻기) | "변경이 있을 때만 묻는다"가 기본 철학 |
| **Toad for Oracle** | 수동이 기본(문장마다 커밋은 13.1에서 제거 — "나쁜 습관") | 없음(서버 몫) | Commit/Rollback 버튼 · 세션 브라우저에서 잠금·차단 조회 | "변경이 감지되면 커밋/롤백 묻기" — 서버에 "열린 트랜잭션이 있는가"를 물어 판단(DB 링크 SELECT도 걸림) | DBA 도구답게 **차단 세션 조회**가 1급 |
| **SSMS** | 기본 자동 커밋 · 옵션 `SET IMPLICIT_TRANSACTIONS`(기본 꺼짐 — 켜는 것은 "끔찍한 생각"이라는 게 업계 통설) | 없음 — **잠금을 쥐고 있어도 경고하지 않는다** | 없음 | 창 닫을 때 "There are uncommitted transactions. Do you wish to commit…?" | 사고 사례의 대부분이 "SSMS에서 BEGIN TRAN 뒤 자리 비움" |
| **TablePlus** | 데이터 편집은 항상 보류 → Ctrl+S에 한 번에 | — | 상단 "n changes" 바 | 닫을 때 묻기 | 편집을 **클라이언트에 모았다가** 짧은 트랜잭션으로 밀어 넣는 구조 = 잠금 시간이 최소 |

**공통 구조 다섯 가지**

1. **늦게 연다** — 읽기는 트랜잭션 밖에서, 트랜잭션은 첫 변경 때(DBeaver 스마트 커밋). 편집 UI는 변경을 클라이언트에 모았다가 한 번에(TablePlus·DataGrip "제출").
2. **오래 두지 않는다** — 유휴 타이머로 끝낸다(DBeaver). 끝내는 동작은 **롤백**이다(의도하지 않은 확정이 더 위험).
3. **보이게 한다** — 모드 · 대기 문장 수 · 갱신 행 수 · 경과 시간.
4. **변경이 있을 때만 묻는다** — 조회뿐이면 묻지 않고 조용히 끝낸다(DbVisualizer "When Uncommitted Updates").
5. **서버 안전망을 함께 쓴다** — 클라이언트가 죽거나 네트워크가 끊겨도 서버가 끝낸다.

## 3. 우리 현재 구조와 빈틈

| 항목 | 현재 | 빈틈 |
|---|---|---|
| 모드 | `session.autocommit`(기본 on) · 탭/세션별 수동 전환 · `tx.smart_commit`(자동 모드에서 첫 DML 뒤 수동으로 — DBeaver식) | — |
| 구현 | 세션은 **항상 트랜잭션 안** · 자동 커밋 = 러너가 문장 뒤 `commit()`(커서가 살아 있으면 닫힐 때까지 미룸) | **수동 모드에서 SELECT만 해도 트랜잭션이 열린 채 남는다**(§1의 (B)) — PG `idle in transaction` · MySQL MDL · SQLite SHARED |
| 표시 | 탭 배지(수/점) · 상태줄 · 툴바 배지+색(`TxClass`) · 트랜잭션 로그 창 · `tx.stale_min`(10분) 뒤 빨강 | 빨강이 될 뿐 **알리지 않는다** · 자리를 비우면 못 본다 |
| 잃는 순간 | 탭 닫기 · 해제 · 다른 서버 · 자동 전환 · 종료에서 묻기(`tx.close_action`) | — |
| 유휴 회수 | `session.idle_secs`(30분)는 **트랜잭션이 열려 있으면 닫지 않는다**(`idle_action` · `tx_open`) | 안전하지만, 그래서 **잊힌 트랜잭션은 영원히 남는다** |
| 막힘 인지 | 없음 | 내 미커밋이 남을 막고 있는지 알 길이 없다(Toad 세션 브라우저 같은 것 없음) |
| 서버 안전망 | 없음 | PG `idle_in_transaction_session_timeout` 같은 세션 파라미터를 접속 때 줄 수 있다 |

## 4. 설계 방안 — 네 겹

### L1 · 읽기 트랜잭션 자동 종료(구조 · 1순위)

> 수동 모드의 뜻을 **"변경은 내가 확정한다"** 로 좁힌다. 변경이 없던 트랜잭션은 사용자에게 의미가 없으므로 앱이 끝낸다.

- 러너가 문장마다 분류(`TxClass` — 이미 있음): `Read`(SELECT/WITH/SHOW/DESC/EXPLAIN)만 실행됐고 **이 세션에 대기 중인 변경이 하나도 없으면**(`tx_pending` 비어 있음) → 결과를 다 받은 뒤(커서가 닫힌 뒤 — 자동 커밋의 `deferred_commit`과 같은 지점) **조용히 `rollback()`**(읽기 트랜잭션은 커밋·롤백이 동치 · 롤백이 부작용 0).
- 예외(끝내지 않는다): `SELECT … FOR UPDATE/SHARE`(잠금이 목적) · 사용자가 직접 연 트랜잭션(`BEGIN`/`START TRANSACTION`/`SET TRANSACTION` 실행 뒤 — 반복 읽기·직렬화 의도) · 세션 임시 객체를 만든 뒤(PG `ON COMMIT DROP` 임시 테이블) · 함수 호출 SELECT가 변경을 일으킬 수 있는 방언에서 `tx.read_end = strict`면 서버에 물어 확인(PG `pg_current_xact_id_if_assigned()` · MSSQL `@@TRANCOUNT`+`sys.dm_tran_session_transactions` · Oracle `dbms_transaction.local_transaction_id`).
- Oracle은 SELECT가 트랜잭션을 열지 않으므로 사실상 무동작(DB 링크 SELECT만 해당 — Toad KB 사례) · 비용 0.
- 효과: §1 (B) 전부 해소 — 수동 모드로 하루 종일 조회만 해도 `idle in transaction`이 남지 않는다. 표시도 정직해진다(`Read`만 있던 상태의 초록 배지는 "끝났음"으로 사라진다).
- 설정: `tx.read_end` = `auto`(기본 · 위 규칙) / `strict`(서버에 물어 확인 뒤) / `off`(종전대로 유지 — 반복 읽기를 습관적으로 쓰는 사용자).

### L2 · 유휴 미커밋 경고 → (선택) 자동 종료

- 시계 = 세션의 **마지막 사용자 실행 시각**(조회 포함 · 탭 전환은 아님). 변경이 대기 중인 세션만 대상.
- 단계: `tx.stale_min`(기본 10 · 이미 있음) → 배지 빨강 + **토스트/실행 카드 "미커밋 n문장 · m분째 · [커밋] [롤백] [나중에]"**(창이 비활성이면 OS 작업 표시줄 깜빡임 · 같은 세션엔 `tx.remind_min`마다 1회) → `tx.idle_action`이 `rollback`이면 `tx.idle_limit_min`(기본 30)에 **60초 카운트다운 대화상자**(취소 가능) 뒤 롤백 · 트랜잭션 로그에 "자동 롤백(유휴 m분)" 기록.
- `tx.idle_action` = `warn`(기본) / `rollback` / `commit`(권하지 않음 · 설명에 경고). DBeaver는 조용히 롤백하지만, 우리는 **카운트다운으로 한 번 더 알린다**(작업 중이던 큰 변경을 말없이 잃지 않게).
- 실행 중(`busy`)·전체 조회 중(`aux`)인 세션은 건너뛴다(통제 규칙 52 §3).

### L3 · 막힘 감지 — "내 트랜잭션 때문에 n개 세션이 기다립니다"

- 미커밋 변경이 있는 세션만 · **메타 세션**(탐색기용 · 이미 있음)으로 `tx.block_poll_secs`(기본 30 · 0 = 끔)마다 한 문장 · 접속이 끊겼거나 유휴면 안 돈다(26 §8 점검 6항목 · 39 §3 부하원 등재).
- 방언별 질의: Oracle `v$session`(`blocking_session = 내 SID`) · PostgreSQL `pg_blocking_pids()` 역조회 · SQL Server `sys.dm_exec_requests.blocking_session_id` · MySQL `sys.innodb_lock_waits`/`performance_schema.data_lock_waits` · SQLite 해당 없음. 권한이 없으면(ORA-00942 등) 조용히 기능을 끄고 로그에 1회.
- 결과 > 0 → 위험색 토스트 + 상태줄 세그먼트 "차단 중 n" + 트랜잭션 로그 창 상단 띠(기다리는 세션의 사용자·프로그램·대기 시간). L2 타이머와 별개로 **즉시** 알린다(남을 막는 중이면 10분을 기다릴 이유가 없다).

### L4 · 서버 안전망(선택) — 접속 직후 세션 파라미터

| DBMS | 문장(값 = 설정) | 효과 |
|---|---|---|
| PostgreSQL | `SET idle_in_transaction_session_timeout = '<n>s'` · (선택) `SET lock_timeout` | 앱이 죽거나 네트워크가 끊겨도 서버가 유휴 트랜잭션 세션을 끝낸다 |
| SQL Server | `SET LOCK_TIMEOUT <ms>`(기다리는 쪽 보호) · 암묵 트랜잭션은 쓰지 않음(우리는 명시 `BEGIN TRAN`) | 내 문장이 남의 잠금에 무한 대기하지 않는다 |
| MySQL | `SET SESSION innodb_lock_wait_timeout` · `lock_wait_timeout` | 같음 |
| Oracle | `ALTER SESSION SET ddl_lock_timeout` | DDL 대기 상한(유휴 차단자 종료는 DBA의 `MAX_IDLE_BLOCKER_TIME`) |
| SQLite | `PRAGMA busy_timeout`(이미 드라이버 기본) | — |

- 설정 `tx.server_idle_timeout_secs`(기본 0 = 안 보냄) · `tx.lock_wait_timeout_secs`(기본 0). 프로필별 재정의는 2차(접속 유형과 함께).
- 세션이 서버에 의해 끊기면 [53 접속 생존](53-connection-liveness.md)의 "끊김 확인 → 다음 실행 때 재접속" 경로가 그대로 받는다(미커밋은 서버가 이미 롤백 — 트랜잭션 로그에 "서버 종료로 롤백됨").

### (2차) 접속 유형 프리셋

프로필에 `type = dev | test | prod`(색 띠 포함) — DBeaver처럼 유형이 L2/L4 기본값 묶음을 고른다(prod: 경고 5분 · 자동 롤백 10분 · 실행 확인). 1차는 전역 설정만.

## 5. 설정 키(안) · 모두 Session ▸ 트랜잭션 분류

| 키 | 기본 | 뜻 |
|---|---|---|
| `tx.read_end` | auto | L1 — 변경 없는 트랜잭션 자동 종료: auto / strict / off |
| `tx.stale_min` | 10 | (기존) 배지 빨강 + L2 첫 경고 시점 |
| `tx.remind_min` | 10 | L2 — 같은 세션 재알림 간격(0 = 한 번만) |
| `tx.idle_action` | **rollback**(D-102) | L2 — warn / rollback / commit |
| `tx.idle_limit_min` | 30 | L2 — 자동 동작 시점(`idle_action ≠ warn`일 때) |
| `tx.block_poll_secs` | 30 | L3 — 막힘 감지 주기(0 = 끔) |
| `tx.server_idle_timeout_secs` | 0 | L4 — 서버 유휴 트랜잭션 타임아웃(지원 방언만) |
| `tx.lock_wait_timeout_secs` | 0 | L4 — 내 문장의 잠금 대기 상한 |

성능 향상 모드(`perf.boost`)는 L3 폴링만 끈다(UI 전용 키 원칙 — 데이터 안전 기능인 L1·L2는 건드리지 않음).

## 6. 단계

| 단계 | 내용 | 크기 |
|---|---|---|
| ① | **L1** 러너 `read_end` 규칙 + 예외 판정 + 방언별 strict 질의 · 표시 연동 · 테스트(방언 4 · 커서 유지 중 · FOR UPDATE · 사용자 BEGIN) | 중 |
| ② | **L2** 경고 토스트/카드 · 재알림 · 카운트다운 자동 롤백 · 트랜잭션 로그 기록 · 설정 4키 | 중 |
| ③ | **L3** 메타 세션 막힘 질의 4방언 · 고지 3곳 · 권한 실패 처리 · 26 §8/39 §3 등재 | 중 |
| ④ | **L4** 접속 직후 세션 파라미터 · 서버 종료 경로 문구 | 소 |
| ⑤ | 접속 유형 프리셋(프로필 `type` · 색 띠) | 중(2차) |

## 7. 결정(사용자 답 대기)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| D-100 ✅ ①(09-19) | L1 기본값 — 수동 모드에서 변경 없는 트랜잭션을 자동으로 끝낼까 | ① auto(끝낸다) ② off(종전 유지 · 옵션으로만) | **①** — 잠금 사고의 구조적 원인 제거 · 예외 규칙으로 의도한 읽기 트랜잭션은 보존 |
| D-101 ✅ ① ROLLBACK(09-19) | L1에서 읽기 트랜잭션을 끝내는 동작 | ① ROLLBACK ② COMMIT | **①**(부작용 0 · 함수 부작용이 있었다면 strict가 걸러 묻는다) |
| D-102 ✅ **②**(09-19 사용자 — 30분 뒤 카운트다운 자동 롤백이 기본) | L2 자동 종료 기본값 | ① warn(경고만) ② rollback(30분 · 카운트다운) ③ DBeaver처럼 조용히 롤백 | **①** 기본 · ②는 선택 — 말없이 잃는 것보다 알리는 쪽 |
| D-103 ✅ ①(09-19 · 켬 30초) | L3 막힘 감지 기본 | ① 켬(30초 · 미커밋 있을 때만) ② 끔(옵션) | **①** — 트래픽은 미커밋 세션당 30초에 1문장 · 권한 없으면 자동 꺼짐 |
| D-104 ✅ ①(09-19 · 0 = 안 보냄) | L4 서버 타임아웃 기본 | ① 0(안 보냄) ② PG만 기본 1800초 | **①** — 서버 정책은 DBA 영역 · 필요할 때 켠다 |
| D-105 ✅ **①**(09-19 사용자 — L1~L4 한 번에) | 범위 | ① ①~④ 한 번에 ② ①②만 먼저 ③ ①만 먼저 | **②** — L1+L2가 사고의 대부분을 막는다 · L3/L4는 이어서 |

## 8. 검증 계획

- 단위: `read_end` 판정 MC/DC(분류 · 대기 변경 유무 · FOR UPDATE · 사용자 BEGIN · 커서 유지) · L2 시계(경고/재알림/자동 시점) · L3 방언별 질의 파서.
- 통합(도커 · integration CI): PG에서 수동 모드 SELECT 뒤 `pg_stat_activity.state`가 `idle`(≠ `idle in transaction`) · UPDATE 뒤 다른 접속의 `ALTER TABLE`이 대기 → L3가 1을 보고 · `idle_in_transaction_session_timeout` 만료 뒤 재접속 경로.
- 실기: 수동 모드로 SELECT 반복 → 툴바 배지 없음 · UPDATE 뒤 10분 → 경고 카드 · `rollback` 선택 시 카운트다운 → 트랜잭션 로그 "자동 롤백".

## 9. 구현(09-19 win 78차 · T-137 ✅ 1차)

추가 확정(사용자 09-19): **유휴 시계 = 그 세션의 문장 실행만**(다른 탭에서 일해도 잊힌 트랜잭션을 잡는다) · **카운트다운 카드 = [지금 롤백] [커밋] [연장] + 작업 표시줄 깜빡임** · **자동 처리의 값·조건은 전부 설정**.

| 겹 | 구현 | 위치 |
|---|---|---|
| L1 | 수동 모드에서 문장 뒤 `read_end_due`: 문장 흔적(`TxEffect` = Clean/Change/UserHold/Ended) → 변경·사용자 통제가 없으면 `ROLLBACK`(열린 커서가 있으면 닫힐 때로 미룸 — 자동 커밋의 미룬 커밋과 같은 자리 `deferred_end`) · `strict` = 서버 확인 질의(PG `pg_current_xact_id_if_assigned` · Oracle `local_transaction_id` · SQL Server DMV) · 이벤트 `RunEvent::ReadTxEnded` → 호스트가 읽기 표시를 걷고 트랜잭션 로그에 "읽기만 · 종료됨" | nsql-core `TxControl`·`is_locking_read` · nsql-run `ReadEnd`·`tx_effect`·`read_end_due`·`note_tx_ended` · 호스트 `tx_on_read_ended` |
| L2 | 순수 판정 `sessions::tx_guard_step`(MC/DC) → 경고 카드(`txwarn.rs` · 비모달 · 실행 카드 위) · 재알림 · 카운트다운 → 만료 시 자동 롤백/커밋(`TxOutcome::AutoRolledBack/AutoCommitted(유휴 분)`) · 나중에/연장 = `tx.remind_min`만큼 미룸 · 그 세션에서 실행하면 전부 되돌림 · 모든 세션(잠든 탭 포함)을 5초(카운트다운 중 1초)마다 점검 | `main.rs tx_guard_tick/tx_warn_show/tx_warn_pick/tx_guard_fire` |
| L3 | 접속 직후 세션 식별자(Oracle SID · PG pid · SQL Server SPID · MySQL connection id)를 받아 두고, 미커밋이 있는 동안만 메타 세션에 방언별 한 문장(`blockers_sql`) → 0→n = 위험 토스트·로그·상태줄 "차단 중 n" · n→0 = 해소 로그 · 질의 오류(권한 없음) = 그 세션에서 기능 끔 + 로그 1회 | `explorer.rs Req::Blockers` · `main.rs tx_block_tick/tx_block_drain` |
| L4 | 접속 직후 `server_guard_sql`(PG `idle_in_transaction_session_timeout`·`lock_timeout` · SQL Server `LOCK_TIMEOUT` · MySQL lock wait · Oracle `ddl_lock_timeout`) — PG의 SET은 트랜잭션에 묶이므로 **커밋으로** 끝낸다 | `worker.rs` 접속 직후 블록 |

설정(Session 분류 · 전부 즉시 반영 — 워커는 실행마다 설정을 다시 읽는다): `tx.read_end`(auto) · `tx.stale_min`(10 · 기존) · `tx.remind_min`(10) · `tx.idle_action`(**rollback**) · `tx.idle_limit_min`(30) · `tx.idle_countdown_secs`(60) · `tx.block_poll_secs`(30 · 향상 모드 0) · `tx.server_idle_timeout_secs`(0) · `tx.lock_wait_timeout_secs`(0).

테스트: nsql-core `tx_control_and_locking_read` · nsql-run `read_only_transactions_end_in_manual_mode`·`tx_effect_rules` · nexa-sql `tx_guard_step_mcdc`·`blockers_sql_per_dialect_and_id_validation`·`server_guard_and_session_id_sql` · 통합(CI PG) `pg_read_only_transaction_ends_in_manual_mode`(조회 뒤 `pg_stat_activity.state = idle` · 변경 뒤 `idle in transaction`).

남은 것: ⑤ 접속 유형 프리셋(2차) · 트랜잭션 로그 창 상단 "차단 중" 띠 · MySQL 드라이버가 들어오면 L1 strict/L3 실기 · 부하원 원장(39 §3)·네트워크 표(26 §8)에 L3 폴링 등재.

## 출처

- DBeaver: [Transaction mode(Auto/Manual · Smart commit · idle transactions)](https://dbeaver.com/docs/dbeaver/Auto-and-Manual-Commit-Modes/) · [Connection Types(유형별 기본 · 유휴 트랜잭션 600/900/1800초 롤백)](https://github.com/dbeaver/dbeaver/wiki/Connection-Types) · [이슈 #7402(창을 닫아도 PG 트랜잭션 유지)](https://github.com/dbeaver/dbeaver/issues/7402) · [블로그 "Change the data in the smart way"](https://dbeaver.com/2022/05/26/change-the-data-in-the-smart-way/)
- DataGrip: [Submit changes to a database](https://www.jetbrains.com/help/datagrip/submitting-and-reverting-changes.html) · [DBE-10935(수동 모드가 불필요한 커밋/롤백 요구)](https://youtrack.jetbrains.com/issue/DBE-10935)
- DbVisualizer: [Auto Commit, Commit and Rollback](https://www.dbvis.com/docs/ug/working-with-sql/auto-commit-commit-and-rollback/)
- Toad: [Auto commit 옵션(KB 4252878)](https://support.quest.com/toad-for-oracle/kb/4252878/how-to-enable-disable-auto-commit-option) · [SELECT 뒤 커밋을 묻는 이유 = DB 링크(KB 4226590)](https://support.quest.com/toad-for-oracle/kb/4226590/why-am-i-being-prompted-to-commit-or-rollback-when-closing-the-editor-after-running-a-select-stat) · [Auto commit 제거 이유(KB 4228153)](https://support.quest.com/toad-for-oracle/kb/4228153/why-is-the-auto-commit-option-no-longer-there)
- SQL Server: [Brent Ozar — SET IMPLICIT_TRANSACTIONS ON Is One Hell of a Bad Idea](https://www.brentozar.com/archive/2018/02/set-implicit_transactions-one-hell-bad-idea/) · [SQLServerCentral — How Implicit Transactions Hurt](https://www.sqlservercentral.com/articles/how-implicit-transactions-hurt-sql-server-performance-without-you-knowing) · [MS Learn — "There are uncommitted transactions…"](https://learn.microsoft.com/en-us/archive/msdn-technet-forums/da216480-2699-40d5-b66b-a8ebc28f711a)
- 서버 안전망: [Oracle 19c MAX_IDLE_BLOCKER_TIME](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/MAX_IDLE_BLOCKER_TIME.html) · [dbi services — MAX_IDLE_BLOCKER_TIME](https://www.dbi-services.com/blog/a-few-words-about-the-good-to-know-max_idle_blocker_time/) · [PostgreSQL Client Connection Defaults](https://www.postgresql.org/docs/current/runtime-config-client.html) · [CYBERTEC — idle_in_transaction_session_timeout](https://www.cybertec-postgresql.com/en/idle_in_transaction_session_timeout-terminating-idle-transactions-in-postgresql/)
