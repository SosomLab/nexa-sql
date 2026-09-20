# 63 — 변수 관리 (사용자 09-21 · DBMS를 가리지 않는 한 벌의 설계)

> 요구(09-21): Oracle에서 쓰던 방식 그대로 — `EXEC :V := …` · `EXEC SELECT … INTO :V1, :V2 …` · 뒤 문장에서 `:V` 사용 ·
> `VAR PC_RET REFCURSOR` + `EXEC 프로시저(:PC_RET, …)` 결과 보기 · 커서 2개 이상 · 테이블 함수 — 가 동작해야 하고, **SQL Server ·
> MariaDB · PostgreSQL · NoSQL에도 같은 수준으로** 적용할 수 있는 추상 구조여야 한다. 예시는 예시일 뿐 — **변수를 쓰는 전체 범위를
> 먼저 정리**하고 그 수준으로 조사한다. 속도·성능·안정성·메모리는 앞서 정한 기준([26](26-performance-architecture.md) ·
> [39](39-resource-governance.md))을 따르고, 개발 뒤 **전수 점검·최적화**한다. 결정이 필요한 것은 조사 뒤 정리해 묻는다.
>
> 선행: [08 세션 변수](08-session-variables.md)(DR-8 — 변수는 클라이언트에 산다) · [04 §8](04-oracle-tools.md) · [05 §7](05-mssql-tools.md) ·
> [43](43-fetch-model-and-result-tabs.md)(결과 탭) · [52](52-session-modes.md)(DR-34 세션) · [30](30-architecture-patterns.md)(포트 + 레지스트리 + 설정).
> 조사 원문(영문 · 출처·검증 표시 · 약 8천 단어) = [63a](63a-variable-research.md).

## 0. 변수가 쓰이는 전체 범위 — Usage 분류 (19군)

"어디에 있나"는 조사한 도구 가운데 대표만. **굵은 —** = 어느 도구에도 없다(차별화 여지). 우리 = 지금 상태(✅ 있음 · 🔶 일부 · ☐ 없음).

| # | 군 | 쓰임(예) | 대표 도구 | 우리 |
|---|---|---|---|---|
| 1 | 선언·타입 | `VAR n NUMBER` · `VAR s VARCHAR2(30)='x'` · 첫 등장 시 암묵 선언 · 격자에서 타입 고르기 | SQL\*Plus · **Golden(암묵)** · PL/SQL Developer(타입 격자) | ✅ 선언·암묵 / ☐ DATE·BOOLEAN · 격자 |
| 2 | 대입 | 리터럴(왕복 0) · 서버 식 · **한 행 → 여러 변수**(`EXEC SELECT … INTO`) · `COLUMN … NEW_VALUE` · 열 → 목록 · 결과 전체 → 변수 · 질의 글 자체를 변수로 | SQL\*Plus · psql `\gset` · WbVarDef · MySQL `INTO @v` | ✅ 셋째까지 / ☐ 나머지 |
| 3 | 바인드 | 진짜 타입 IN 바인드 · OUT/INOUT · 함수 반환값 · `RETURNING INTO` · 배열/벌크 | Oracle 도구 · WbCall · (DBeaver·DataGrip·HeidiSQL 등은 **글자 치환**) | ✅ IN·OUT·반환 / ☐ 배열 |
| 4 | 결과 흐름 | REF CURSOR → 그리드 · AUTOPRINT · 암묵 결과 · 셀 안 커서 · 다중 결과 집합 → 탭 · OUT 값을 표로 · 테이블 함수 | Golden(그리드) · **WbCall(커서마다 탭 · 탭 이름 = 인자)** · SSMS | ✅ 09-21(§6) / ☐ 셀 안 커서 |
| 5 | 치환(글자 매크로) | `&v` `&&v` `DEFINE` · `$(v)` · `${v}` · 안전 인용형 `:'v'` · 형식 지정 `${v:csv}` · 식별자·조각(`FROM &tab`) | SQL\*Plus · sqlcmd · psql · Grafana | 🔶 `&` `:setvar` / ☐ 인용형·형식 |
| 6 | 입력 받기 | `ACCEPT … HIDE DEFAULT` · 미정의 시 자동 묻기 · 목록·질의 기반 목록·다중 선택 · 값 이력 · 비밀번호 | **PL/SQL Developer(가장 풍부)** · WbJ · Redash | 🔶 CLI만 / ☐ **GUI는 빈 글로 넘어감(T-16c)** |
| 7 | 시스템 변수 | `_USER` `_DATE` · `ROW_COUNT` `SQLSTATE` `LAST_ERROR_MESSAGE` · 경과 시간 **—** · 마지막 삽입 id | SQL\*Plus · psql | ☐ |
| 8 | 환경·인자 | `@script a b` → `&1` · `-v name=value` · 환경 변수 폴백 | 전부 | ✅ `&1` / ☐ `-v`·env |
| 9 | 흐름 제어 | `\if` · `WHENEVER SQLERROR` · `:on error` · `GO n` · `\watch` | psql · SQL\*Plus · sqlcmd | 🔶 WHENEVER·GO n |
| 10 | 클라이언트 명령 안 | `SPOOL &f..log` · `CONNECT &u@&db` · `@&script` | SQL\*Plus | ✅(치환이 먼저 돈다) |
| 11 | 형식 | NEW_VALUE/OLD_VALUE · 숫자·날짜 형식 · NLS(SQL\*Plus엔 DATE 바인드가 없어 날짜가 글자로 다닌다) | SQL\*Plus | ☐ |
| 12 | 범위·수명·공유 | 프로세스 전역 · **탭별(Golden)** · 연결별 · 계층(WbJ: 명령줄 > 작업공간 > 프로필) · 디스크 · dev/test/prod 값 묶음 **—** · 가져오기/내보내기 | §2 | 🔶 세션(러너)별 · 저장 없음 |
| 13 | 비밀 값 | `ACCEPT HIDE` · 가려 보이기·기록 안 함·저장 안 함 **—** | — | ☐ |
| 14 | 살펴보기 UI | `VARIABLE` `PRINT` `DEFINE` · **편집 가능한 목록(WbVarList)** · 패널(이름·타입·값·바뀜 표시) · 값 위 hover **—** · 미정의 경고 **—** | PL/SQL Developer · DBeaver | 🔶 `SHOW VARIABLES` 글자 / ☐ **GUI에서 값이 안 보인다** |
| 15 | 디버깅·출력 | DBMS_OUTPUT · PRINT · RAISE NOTICE · 디버거 watch | 각 도구 | ✅ 서버 메시지 / ☐ 디버거 |
| 16 | 결과 ↔ 변수 | 그리드 셀 → 변수 **—** · 주-상세 질의 연결(`:m_<열>`) · 매개변수 있는 저장 질의·템플릿·대시보드 | PL/SQL Developer · Metabase | ☐ |
| 17 | 트랜잭션 | 클라이언트 변수는 ROLLBACK과 무관 · PG 사용자 GUC는 **트랜잭션을 탄다** | — | ✅(클라이언트 보관) |
| 18 | 기록·감사 | 바인드 값 기록과 가림 · 서버 쪽 기록(`log_parameter_max_length` · `V$SQL_BIND_CAPTURE`) | DBeaver QM | 🔶 이름만 기록 |
| 19 | NoSQL | 이름 매개변수 = JSON 값(`$name` Couchbase·Neo4j · `@name` Cosmos · `?name` ES\|QL · `:name` Cassandra) · 자리만(`?` DynamoDB · ARGV Redis) · **식별자 자리표**(ES\|QL `??` · DynamoDB `#n` · Redis KEYS) · 서버 글자 틀(ES mustache) | cbq · cypher-shell · Kibana | ☐(드라이버 없음 — 구조만 대비) |

## 1. 조사 요약 — 무엇을 베끼고 무엇을 피하나 ([63a](63a-variable-research.md))

**전제 바로잡기**: ① 진짜 SQL\*Plus는 바인드를 암묵 선언하지 **않는다**(`SP2-0552`) — 사용자 스크립트의 관용은 **Golden**의 것("보이는 바인드를 문자열로 자동 정의" · 커서 그리드 · **변수 목록 = 탭별**) ② `EXEC SELECT … INTO :A` 는 EXEC가 `BEGIN … END;`로 감싸므로 SQL\*Plus에서도 된다 ③ **DBeaver는 바인드하지 않는다** — 글자로 끼워 `createStatement()`로 보낸다(진짜 바인드 요청 #15310 미해결 · Oracle 암묵 결과는 첫 장만 #17451) ④ tiberius는 OUTPUT RPC 인자·반환 상태를 주지 않는다 → SQL Server는 **꼬리 SELECT로 값을 읽어 오는 것이 필수**(지금 구현이 그것).

| 베낀다 | 피한다 |
|---|---|
| **psql 어휘 규칙** — 변수는 코드 상태에서만 · `::`·`:=`를 토큰으로 먼저 먹는다 · 치환한 글을 다시 훑지 않는다 | 정규식 글자 치환을 "매개변수"라 부르기(HeidiSQL·Beekeeper·DbGate·SQLTools) |
| **엔진이 말해 주는 매개변수 이름**(rusqlite `parameter_name` · rust-oracle `bind_names()`)로 교차 확인 | 모든 바인드를 VARCHAR2로(SQL Developer F9 — 실행 계획이 앱과 달라진다) |
| **WbCall 결과 모델** — OUT 값은 `이름│값` 표 · **커서마다 탭, 탭 이름 = 인자** · Golden의 커서 그리드 | 문자열·주석 안까지 치환(SQL\*Plus `&` · DBeaver `${}`) |
| `\gset`·WbVarDef처럼 **0행/여러 행/NULL 정책을 명시** | 이름만으로 묶는 전역 디스크 저장(DBeaver — 서버끼리 값이 샌다) |
| WbJ의 **계층 범위**(명령줄 > 작업공간 > 프로필) · cbq의 중첩 스크립트용 스택 | 문장마다 뜨는 대화상자 · 매크로가 같은 이름의 바인드를 가로채기(usql) |
| PL/SQL Developer 격자(타입·바뀐 값 강조·오래된 변수는 지우지 않고 끔) · Grafana 형식 지정 `${v:csv}` | 다시 포맷한 SQL을 실행(Beekeeper) · SQL Server/MySQL에서 `@`를 클라이언트 접두로 쓰기 |

## 2. 지금 있는 것 (09-21 점검)

- **저장소**: `nsql-script::VarStore`(대소문자 무시 · `declare`/`assign`(암묵 생성)/`absorb`) · `Engine`(바인드 추출 → 방언 재작성 `prepare` · `wrap_exec` · `&` 치환) · 값 = `Value`(Null·Int·Float·Decimal·Str·Bool·Bytes·Cursor) · 타입 = `VarType`(REFCURSOR 포함).
- **포트**: `Session::execute(ExecRequest{sql, params[BindParam{name,value,ty,direction}]}) → ExecResult{out_params, result_sets[], messages, pending}` + `fetch_cursor(CursorId)` — OUT 값과 다중 결과는 이미 1급이다.
- **드라이버**: Oracle(이름 바인드 · OUT · REF CURSOR · 암묵 결과) · SQL Server(`DECLARE … = @Pn` 머리 + 꼬리 `SELECT @V`) · PG(`$n` · CALL OUT = 1행 결과) · SQLite(`?`) · MySQL/MariaDB·NoSQL = **드라이버 없음**.
- **수명**: 변수 표는 `Runner`(= 세션 `Sess`의 워커) 안 → **공유 세션에 묶인 탭 전부가 한 표를 같이 쓴다** · 전용 세션 탭은 따로 · 재시작하면 사라진다.
- **GUI**: `&` 입력 창 없음(빈 글로 넘어감 · T-16c) · `RunEvent::Print` 무시 → **값이 어디에도 안 보인다** · 변수 패널 없음.

## 3. 추상 구조 — DBMS를 가리지 않는 한 벌

```
편집기 글 ─ split(방언 어휘) ─► Item
  Engine.plan(Item, VarStore, MacroStore, Caps) ─► Action
      LocalAssign │ NeedInput(입력 명세) │ Execute(Request) │ Print │ …
  Request { text, params[Param{name, dir In│Out│InOut│Return, ty, value}],
            captures[RowToVars{names, policy}] }             ← "1행 결과 = 돌아온 값"을 **요청이 명시**(추측 금지)
  Session.execute(Request) ─► ResultStream = 사건*
      RowSet{label, source: Primary│OutCursor(이름)│Implicit(n)│ResultSet(n)} │ OutValues │ ReturnStatus │ Affected │ Message
  Engine.absorb(OutValues │ 잡은 행) ─► VarStore ─► VarsChanged(바뀐 것만) ─► 패널·로그
```

| 조각 | 역할 | 규칙 |
|---|---|---|
| **VarStore**(바인드 변수) | 이름 → {타입 · 값 · 선언됨 · **비밀** · 출처(스크립트·입력·OUT·잡은 행·프로필) · 이번 실행에서 바뀜 · 소비됨(커서)} | 값은 **타입을 가진 채** 보관하고 타입 있는 바인드로 보낸다(절대 "전부 문자열" 아님) · `Value`에 `Missing`·`Array`·`Doc`·`Ext{tag}`·`Query(글)` 추가 예정(NoSQL · 커서 없는 DBMS의 "지연 커서") |
| **MacroStore**(치환 변수) | `&v` · `DEFINE` · `:setvar`/`$(v)` · 인자 `&1` · 시스템 변수(`_USER` `_DATE` `_ROW_COUNT` `_SQLCODE` `_ELAPSED_MS` `_FILE`) · 범용 `${v:형식}` | **바인드와 섞지 않는다**(이름이 같아도 별개 · 매크로가 바인드를 가로채지 않는다) · 패널에서는 한 목록에 "종류" 열로 |
| **Caps**(드라이버 능력 — 포트 + 레지스트리 · [30](30-architecture-patterns.md)) | `marker`{`:n` `@n` `$N` `?` `$name` 엔진이 알려줌} · `bind_named/positional/array` · `ident_placeholder` · `out_values`{없음·프로토콜·꼬리 행·결과 행} · `return_status` · `cursor_out`{없음·핸들·이름(트랜잭션 필요)} · `multi_result` · `implicit_results` · `paging` · `server_vars` · `bind_scope`{어디나·DML만·WHERE만} · `describe_params` | 엔진의 방언 `match`를 **능력 질의**로 바꾼다 → 새 DBMS·NoSQL 어댑터는 능력표 + 어댑터 함수(`to_native` · `from_native` · `quote_value` · `quote_ident` · `format`)만 채운다 |
| **호출 서명 추론** | `EXEC proc(:A, :B)`의 자리·이름 ↔ 서버 서명 → 선언 없는 바인드의 타입·방향 | ✅ Oracle `ALL_ARGUMENTS`(09-21) · 다음 = PG `pg_proc` · SQL Server `sys.parameters` · MySQL `information_schema.PARAMETERS` · 루틴당 1회 · 캐시 |
| **낮추기 사다리** | 진짜 바인드 → (문장 종류가 바인드를 못 받으면 · DDL·PG 유틸리티·`DO`) 어댑터가 인용한 글자 + **눈에 보이는 안내** → PROD 접속에서 인용 없는 자유 글은 확인 | 조용한 글자 치환 금지 |

### 3-1. 방언별 구현 기법 (같은 글 → 같은 결과)

| 쓰임 | Oracle | SQL Server | PostgreSQL | MySQL/MariaDB | SQLite | NoSQL |
|---|---|---|---|---|---|---|
| `EXEC :V := '리터럴'` | 로컬 · 왕복 0 | 같음 | 같음 | 같음 | 같음 | 같음(JSON 값) |
| `EXEC :V := 식` | `BEGIN :V := 식; END;` | `DECLARE @V T = @P1; SET …; SELECT @V`(꼬리 행) | `SELECT (식) AS "V"` → 잡기 | 같음 | 같음 | Neo4j `RETURN 식` |
| `EXEC SELECT a,b INTO :A,:B` | OUT 바인드 블록 | `SELECT @A=a,@B=b` + **`@@ROWCOUNT` 검사**(T-SQL은 마지막 행을 말없이 쓴다) | **INTO를 떼고 행을 클라이언트가 잡는다**(0행·여러 행 정책) | 같음(서버 `INTO @v`는 0행에 옛 값이 남아 피한다) | 같음 | 같음 |
| 뒤 문장의 `:V` | 이름 바인드 | `sp_executesql` 인자(배치 머리 DDL은 `DECLARE` 앞붙임/글자) | `$n` + 선언 타입 캐스트 · 유틸리티·`DO` = 인용 글자 | `?` | 엔진이 알려 준 이름(`:x` `@x` `$x` = 한 변수) | 네이티브 매개변수 · 없으면 `format()` 글자 |
| 커서·다중 결과 | OUT 커서 = 이름 붙은 결과 탭 · 암묵 결과 = "ResultSet #n" | 결과 집합마다 탭(커서 바인드 없음 — `VAR c REFCURSOR`는 안내) | 트랜잭션 안에서 호출 → 커서 **이름**을 읽어 `FETCH ALL IN "n"` → 탭 → `CLOSE`(자동 커밋이면 러너가 감싼다 · T-146 기반) | n번째 결과 = n번째 탭 · OUT = 추가 1행 | `Query` 값(PRINT 때 실행) | 커서/토큰 페이징을 같은 흐름으로 |
| 테이블 함수 | `TABLE(f(:x))` | `dbo.f(@p)` | SRF | `JSON_TABLE` | `json_each(:j)` | — (전부 보통 조회 + 바인드) |

## 4. 성능·안정성·메모리 기준 (개발 뒤 전수 점검 항목)

| 항목 | 기준 | 지금 |
|---|---|---|
| 리터럴 대입 | DB 왕복 0 | ✅ |
| 문장 준비(바인드 추출 + 재작성) | 문장 길이에 선형 · 1 MB 스크립트에서 문장당 0.1 ms 아래 · 실행마다 다시 하지 않아도 되는 것은 캐시 | ✅ **2.9~3.2 µs/문장**(Release · `bench_vars` · 4만 문장 5.9 MB = 115 ms) |
| 실행 전 훑기(D-137) | 실행을 눈에 띄게 늦추지 않는다 · 볼 것이 없는 스크립트는 공짜 | ✅ 덤프 4만 문장 **0.37 ms**(`:`·`&`가 없으면 나누지도 않음) · 최악(모든 문장에 바인드+치환) 167 ms/5.9 MB · 보통 1천 문장 4 ms |
| 서명 조회 | 루틴당 1회(캐시) · 타입이 필요한 바인드가 있을 때만 · 실패는 조용히 종전 동작 | ✅ |
| 재작성한 SQL | **결정적**(같은 글 → 같은 SQL) — 서버 계획 캐시 재사용 · 진짜 타입 바인드 = 앱과 같은 `sql_id`/계획 | ✅ Oracle·PG / 🔶 SQL Server `DECLARE` 앞붙임은 값이 글자로 들어간다 |
| 값 크기 | 변수 하나 상한(설정 `vars.max_value_kb`) · 패널은 앞부분만 그린다 · LOB는 보기 창에서 요청 시 | 🔶 창·로그는 앞부분만(200/80자) · 1 MB 값 왕복 0.21 ms · 상한 설정 ☐ |
| 열린 커서 | **거버넌스 대상**(Oracle `open_cursors` · MariaDB `max_open_cursors` 50) — 자동 표시는 받자마자 닫는다 · 끄면 상한 + 재접속/유휴에서 무효화 | 🔶 자동 표시 ✅ / 무효화 ☐ |
| 결과 탭 | `grid.result_tabs_max` 안 · 딸린 탭은 재실행 때 재사용 · 안 쓰인 것은 실행 끝에 걷고 `memtrim` | ✅ |
| 패널 그리기 | 변수 사건(`Vars`)이 올 때만 · 매 프레임 0 | ✅ 변수 창 = 사건·탭 전환·자기 입력에서만 다시 그림(닫으면 0) |
| 비밀 값 | 패널·로그·트랜잭션 로그에서 가림 · 저장 안 함 · 서버 쪽 기록 가능성은 문서로 알림 | ✅ 이름 규칙(D-140) → 창·로그·`SHOW VARIABLES` 가림 · 보존 파일·스크립트에 값 없음 / ☐ `ACCEPT HIDE` |
| 글자 치환 | 다시 훑지 않음 · 인용형 · PROD 확인 — 주입 위험을 기본으로 낮춘다 | 🔶 |

## 5. 단계 (브랜치 `feat/session-vars`)

| 단계 | 내용 | 상태 |
|---|---|---|
| **V0 기반 결함** | 저장 코드 DDL은 바인드 안 함(`:NEW`/`:OLD`) · Oracle 암묵 결과 전부 · Oracle/SQL Server는 1행 결과를 변수로 흡수하지 않음 · ★ T-146 수동 커밋이 실제로는 자동 커밋이던 결함 | ✅ 09-21 |
| **V1 커서·다중 결과** | REF CURSOR 자동 표시(`run.cursor_autoshow`) · 결과 라벨 = 변수 이름 · 서명 추론(`call_shape` + `routine_args`) · GUI 딸린 결과 탭 | ✅ 09-21(Oracle 실기) |
| **V2 보이기** | ✅ 09-21 `RunEvent::Vars` · 로그에 바뀐 값(비밀 가림) · `SHOW VARIABLES` = 결과 표 · **변수 창 `vars_win.rs`**(View ▸ Variables — 탭 층 + 공유 층 · 바뀐 줄 강조 · 제자리 편집 · NULL · 공유 토글 · 삭제 · `이름 = 값` 새 변수 · 스크립트로). ☐ 옆 패널 · 편집기 hover 값 · 미정의 경고 | ✅(잔여) |
| **V3 입력** | ✅ 09-21 GUI 입력 창(실행당 **한 번** · 빠진 바인드 + `&` 매크로를 한 격자에서 · Skip = 종전 · 값은 탭에 기억 · `vars.undeclared`). ☐ 타입 열·미리보기 · `ACCEPT [HIDE] [DEFAULT]` · `COLUMN … NEW_VALUE` | 🔶 |
| **V4 범위·보존** | ✅ 09-21 D-135 계층(탭이 주인 · `VAR x SHARE|LOCAL` · 재접속 = 커서 무효화) · D-136 **파일별 보존**(`vars/<경로 해시>.sql` = 실행 가능한 스크립트 · 비밀·커서 제외) · 스크립트로 내보내기. ☐ 이름 없는 탭(S-1 hot exit와 함께) · 프로필 층 값의 출처(접속 프로필 필드) | 🔶 |
| **V5 능력표** | ✅ 09-21 `Request.captures`(1행 잡기를 명시 · 자리 순서) · D-139 `vars.into_policy` · SQL Server `@@ROWCOUNT` 검사 · PG 배열 조각 오탐 — 실서버 통합 9/9. ☐ `Caps` 포트로 방언 `match` 걷어내기 · **PG refcursor(`FETCH ALL IN`)** · 서명 추론 PG/SQL Server/MySQL · DATE/TIMESTAMP/BOOLEAN 타입 | 🔶 |
| **V6 매크로** | MacroStore 분리 · 시스템 변수 · `${v:형식}` · `-v name=value` · 문자열 안 치환은 선택(`define.in_strings`) · 상태줄 `&` 토글 | ☐ |
| **V7 전수 점검** | ✅ 09-21 1차 계측 `bench_vars`(§4 표) + 훑기 최적화 · 실서버 통합 9/9(Oracle·SQL Server·PG). ☐ 값 크기 상한 · 4방언 GUI 실기(Codespaces) · 변수 창·입력 창의 포커스/마우스 라우팅 실기표 | 🔶 |
| 뒤로 | 행마다 실행(배열 바인드) · 그리드 셀 → 변수 · 주-상세 연결 · 서버에 비추기(SESSION_CONTEXT · `set_config`) · MySQL/MariaDB · NoSQL 어댑터 | ☐ |

## 6. 09-21 실기 (Oracle 19c · SNOP-DB · `NSQLT_` 객체만 · 사전 0 → 이름 지정 삭제 → 잔여 0)

사용자 예시 그대로: `EXEC :V := '…'`(탭 구분 포함) ✅ · 맨 `EXEC` + 여러 줄 `SELECT … INTO :A, :B … ;` ✅ · `SELECT :A, :B FROM DUAL` ✅ · `VAR PC_RET REFCURSOR` + `EXEC p(:PC_RET, :V, 3)` → **바로 결과**(`PC_RET:` 라벨) ✅ · **선언 없이** `EXEC p2(:PC_A, :PC_B, :V_MSG)` → 서명에서 커서 2 + 문자열 OUT ✅(종전 PLS-00306) · `DBMS_SQL.RETURN_RESULT` 2장 ✅(종전 = 첫 장만, 둘째는 변수로 빨려 들어감) · `SELECT * FROM TABLE(파이프라인 함수(:V))` ✅ · GUI = 결과 탭 `PC_A` · `PC_B` ✅(캡처).

## 7. 결정 (D-135~ · 권장안 = 굵게) — ✅ **D-135~138 = 09-21 사용자가 전부 권장안으로 확정** · D-139~143 = 권장안으로 진행

| # | 결정 | 선택지 | 권장 |
|---|---|---|---|
| **D-135** | 변수 표의 주인 | ① 탭(Golden) ② 세션 = 지금(공유 세션의 탭들이 한 표를 같이 씀) ③ 계층: **탭 표 + "연결과 공유" 층 + 읽기 전용 프로필 층**(WbJ식) | **③** — 기본은 탭(다른 탭의 실행이 내 값을 바꾸지 않는다) · 일부러 공유하고 싶은 값만 올린다 · 값은 `CONNECT`를 넘어 살고 커서만 무효 |
| **D-136** | 재시작 뒤 보존 | ① 없음 ② **hot exit와 함께 탭 상태로**(비밀·커서 제외) ③ 파일로만 내보내기/가져오기 | **② + ③** |
| **D-137** | 값이 없는 바인드를 **읽을** 때 | ① 말없이 NULL(지금) ② **한 번 묻는다**(실행당 한 번 · 격자) ③ 오류(SQL\*Plus) | **②**(대입 대상은 종전처럼 자동 생성 · 읽기만 있는 미지의 이름은 대개 오타나 빠진 입력) · 엄격 모드 = ③ |
| **D-138** | 스크립트 전체 실행에서 조회가 여럿일 때 | ① 지금처럼 마지막 결과가 덮는다 ② **문장마다 결과 탭**(상한 안 · DBeaver "탭별 실행") ③ 설정 | **③, 기본 ②** — 커서 여러 개는 이미 탭으로 나뉜다(V1) |
| D-139 | `SELECT … INTO` 0행·여러 행 | Oracle식 오류 / psql식 / 첫 행 | **Oracle식**(설정 `vars.into_policy`) · NULL = Null(지우지 않음) |
| D-140 | 비밀 값 | 없음 / **`ACCEPT HIDE` + 이름 규칙(`*PASS*` `*PWD*` `*SECRET*` `*TOKEN*`)으로 표시 → 가림·기록 안 함·저장 안 함** | 권장대로 |
| D-141 | `&` 치환 | SQL\*Plus 그대로 / **Oracle 방언 탭에서만 기본 켬 · 주석 안 제외(지금) · 문자열 안은 선택 · 상태줄 토글** | 권장대로 |
| D-142 | 이름 대소문자 | **무시(처음 쓴 표기 보존)** · `:"이름"`만 구분 · SQLite `:x @x $x` = 한 변수 | 권장대로 |
| D-143 | 서버에 비추기 | 안 함 / **변수별 선택**(SESSION_CONTEXT · `set_config` · `SET @v` — PG는 트랜잭션을 탄다는 경고) | 뒤로 |

## 8. 시험 환경

- Oracle = SNOP-DB(VPN · `NSQLT_` 접두 · 사전 0 확인 → 이름 지정 삭제 → 잔여 0 · 스냅숏 대조). **VPN이 끊기면 GitHub Codespaces**(`.devcontainer` = dev + Oracle Free + SQL Server · PG/MySQL 프로필 · `NSQL_*_URL`) — 다른 DBMS도 Codespaces · 끝나면 닫는다(사용자 09-21). 막힌 것: gh 토큰에 `codespace` 권한 없음 → `gh auth refresh -h github.com -s codespace`(사용자 브라우저 인증 1회).
- CI `integration` = PG · SQL Server · Oracle 컨테이너에서 같은 흐름(지금 T-146로 실패 중 → 이번 수정으로 통과 예상).
