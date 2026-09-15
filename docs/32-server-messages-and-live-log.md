# 32 · 서버 메시지 · 실행 중 로그(라이브 로그) — SQL Server/PG는 즉시 · Oracle은 폴링 모니터

> **요청**(사용자 09-15): *"로그를 확인하는 기능 · SQL Server는 flush로 실행 중 로그 확인이 가능한데 Oracle도 가능하도록 구현해 줄 수 있는지 확인"* · *"프로시저 실행 · Output 변수 · 프로시저의 SELECT 결과를 DBMS별로 받아 표시"*.
> **선행**: [26 성능](26-performance-architecture.md)(로그 허브 · 단계 계측) · [08 세션 변수](08-session-variables.md)(OUT 회수 규약) · [28 탐색기](28-object-explorer.md)(메타 세션 = 별도 접속).
> **상태**: ✅ SQL Server·PostgreSQL 실시간(09-15 구현 · 실서버 확인) · 📐 Oracle 폴링 모니터(**T-71**) · 결정 **D-48**.

---

## 0. 결론 다섯 줄

1. **SQL Server** `PRINT` · `RAISERROR(...,0,1) WITH NOWAIT`는 TDS Info 토큰으로 **실행 중에 도착**한다 — 도착 즉시 UI 로그/stdout에 흘린다(✅).
2. **PostgreSQL** `RAISE NOTICE/WARNING/INFO`도 비동기 Notice 메시지로 **실행 중 도착** — 즉시 표시(✅).
3. **Oracle** `DBMS_OUTPUT`은 **서버 버퍼**다. 호출(프로시저)이 **끝나야** `GET_LINE(S)`로 읽을 수 있다 — 실행 중 flush는 **서버 구조상 불가**. 이것은 도구가 아니라 Oracle의 제약(SQL*Plus·SQL Developer·Toad 모두 같다).
4. Oracle에서 "실행 중 로그"를 보려면 **프로시저가 다른 통로로 써야** 한다: ⓐ **자율 트랜잭션 로그 테이블**(가장 흔한 현장 관행) · ⓑ `DBMS_APPLICATION_INFO.SET_CLIENT_INFO/SET_ACTION`(V$SESSION에 즉시 보임) · ⓒ `DBMS_PIPE`(실시간 파이프). 도구는 **두 번째 세션으로 폴링**해 보여 준다 → **라이브 모니터**(T-71).
5. 프로시저 **OUT 변수 · 결과 집합**은 방언별로 이미 회수된다(§3 표 · 09-15 실서버 확인) — 결과 집합은 여러 개 순서대로 그리드 탭/CLI 연속 출력, OUT은 `PRINT`/변수.

---

## 1. 전달 경로(구현)

```
드라이버 스레드                                  UI / CLI
  tiberius Info 토큰 ──tracing INFO 이벤트──▶ InfoCapture(스레드 기본 구독자)
  postgres Notice    ──notice_callback────▶ Notices(공유 버퍼)
        │ 싱크 있음: MessageSink(m) 즉시 ──▶ RunEvent::Message → 로그 창 · 상태 · stdout(즉시 flush)
        │ 싱크 없음: ExecResult::messages(실행 뒤 한꺼번에)
  Oracle DBMS_OUTPUT ─ 실행 뒤 GET_LINE 폴링 ─▶ messages(변경 없음 · SET SERVEROUTPUT ON)
```

| 항목 | 내용 |
|---|---|
| 포트 | `Session::set_message_sink(Option<MessageSink>)`(기본 no-op) · `MessageSink = Arc<dyn Fn(String) + Send + Sync>` |
| SQL Server | tiberius는 Info 토큰을 public API로 주지 않고 `tracing` **INFO 이벤트**로만 낸다 → 어댑터가 `execute` 동안 **현재 스레드 기본 구독자**(`tracing::subscriber::set_default`)로 캡처. 다른 스레드·크레이트 무영향. 싱크 있으면 즉시 · 없으면 `messages` |
| PostgreSQL | `Config::notice_callback` — 접속 시 심는 콜백이 `SEVERITY: message`로 만들어 싱크/버퍼로 |
| 호스트 | `Runner::with_message_sink` → 접속마다 세션에 심는다. GUI = 워커 이벤트 채널(`RunEvent::Message` + 깨우기) · CLI = stdout 즉시 flush |
| 실측(09-15) | M4PLAN: `PRINT`·`RAISERROR … WITH NOWAIT` — `WAITFOR DELAY 1s` 전후로 각각 도착(+1481ms · +2488ms) · matrixdb2: `RAISE NOTICE` 2건 `pg_sleep(0.5)` 사이로 도착(+141ms · +668ms) |

**부작용 0 원칙**: 캡처 구독자는 `target.starts_with("tiberius")` · INFO만 · 실행 스레드 범위. 로그 허브([26 §3](26-performance-architecture.md))와 같은 격리 — 메시지 폭주(수만 줄 PRINT)는 링 버퍼가 감당하고 UI는 이벤트 드레인만.

---

## 2. Oracle 라이브 모니터(T-71 · 📐)

DBMS_OUTPUT은 못 바꾸지만, **DBA가 이미 쓰는 관행**을 도구가 읽어 준다. 편집기 세션이 프로시저를 실행하는 동안 **메타 세션**([28 §3](28-object-explorer.md) · 별도 접속)이 주기 폴링한다.

| 소스 | 프로시저 쪽 | 도구 쪽(폴링 SQL) | 장단 |
|---|---|---|---|
| ⓐ 로그 테이블 | `PRAGMA AUTONOMOUS_TRANSACTION` 프로시저로 `INSERT … COMMIT` | `SELECT … FROM <log_table> WHERE ts > :last ORDER BY ts`(설정: 테이블·시각 컬럼·본문 컬럼 · 프로필별) | 가장 흔함 · 이력 남음 · 테이블 규약 필요 |
| ⓑ V$SESSION | `DBMS_APPLICATION_INFO.SET_CLIENT_INFO('step 3/10')` · `SET_ACTION` | `SELECT client_info, action, module FROM v$session WHERE sid = :sid`(편집기 세션의 SID는 접속 시 `SYS_CONTEXT('USERENV','SID')`로 기억) | 코드 1줄 · 권한 `SELECT ON V_$SESSION` 필요 · 최신 값 하나만 |
| ⓒ DBMS_PIPE | `DBMS_PIPE.PACK_MESSAGE` + `SEND_MESSAGE('nsql_<sid>')` | `RECEIVE_MESSAGE('nsql_<sid>', 0)` 반복 | 실시간 · 이력 없음 · 파이프 규약 필요 |
| ⓓ V$SESSION_LONGOPS | `DBMS_APPLICATION_INFO.SET_SESSION_LONGOPS` | `SELECT sofar, totalwork, message FROM v$session_longops WHERE sid = :sid` | 진행률 게이지에 적합 |

**설계**: 로그 창 세그먼트 "Live"(켜면 폴링 시작 · `oracle.live.source = table|session|pipe|off` · `oracle.live.interval_ms` 1000 · `oracle.live.table/ts_col/text_col`) — 실행 시작 ~ 종료+1틱 동안만 폴링(유휴엔 0) · 오류는 세그먼트에 ⚠(편집기 실행 무영향) · 각 줄 = `RunEvent::Message` 경로와 같은 로그 엔트리(타임스탬프 첫 컬럼). CLI = `nsql run --live table:<t>` 옵션. 부하 원칙([26 §8](26-performance-architecture.md)): 실행 중에만 · 1s 이상 · 프로필당 1 · 마지막 시각 이후만.

**결정 D-48**: 기본 소스는 ⓑ(코드 변경 최소 · 권한만) vs ⓐ(현장 관행)? — 권장 **ⓐ 테이블 기본 + ⓑ 세그먼트 병행 표시**(둘 다 설정 시).

---

## 3. 프로시저 OUT · 결과 집합(방언별 · 09-15 실서버 확인)

| 방언 | OUT 회수 | 결과 집합 | 메시지 | 확인 |
|---|---|---|---|---|
| Oracle | 이름 바인드 InOut(VARCHAR2 4000 · `VarType`) · REF CURSOR 핸들 → `PRINT rc` 1회 소비 | REF CURSOR = 그리드(`PRINT`) | DBMS_OUTPUT 실행 뒤(SERVEROUTPUT ON) · 컴파일 오류 `ALL_ERRORS` → Error 이벤트 | `examples/it-oracle.sql` biscm ✅ |
| SQL Server | `DECLARE @V 타입 = @Pn; EXEC … @V OUTPUT; SELECT @V AS V` 트레일러 | 프로시저 안 SELECT 여러 개 → 결과 집합 순서대로(트레일러는 OUT으로 흡수) | PRINT/RAISERROR 즉시(§1) | 임시 `NSQL_IT_P` M4PLAN ✅(OUTPUT 42 · 결과 2개) |
| PostgreSQL | `CALL p(…, NULL)` → OUT이 1행 결과 → 호스트가 변수로 흡수 · 함수는 `SELECT * FROM f()` | `SELECT`/`CALL` 결과 | RAISE NOTICE 즉시(§1) | `examples/it-pg.sql` matrixdb2 ✅ |
| SQLite/MySQL | `SELECT expr AS "V"` 별칭 흡수 | 일반 | — | sqlite 테스트 ✅ |

`SELECT … INTO :A, :B FROM …`(SQL*Plus 관용)은 OUT이 없는 방언에서 `SELECT … AS "A", … AS "B" FROM …`으로 재작성해 1행을 흡수한다(09-15 · `rewrite_select_into_alias`).

---

## 4. 작업

| ID | 항목 | 의존 |
|---|---|---|
| ✅ | `MessageSink` 포트 · MSSQL tracing 캡처 · PG notice_callback · GUI/CLI 싱크 · T-SQL `PRINT 'x'`/`CREATE OR ALTER` 분리 수정 | — |
| **T-71** | Oracle 라이브 모니터(§2) — 메타 세션 폴링 · 로그 창 Live 세그먼트 · 설정 `oracle.live.*` · CLI `--live` | 28 T-56 D-48 |
| **T-49** | DBMS_OUTPUT `GET_LINES` 배열 회수(속도) · UNLIMITED 안내 | — |
