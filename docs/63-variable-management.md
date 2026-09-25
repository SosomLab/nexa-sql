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
| 14 | 살펴보기 UI | `VARIABLE` `PRINT` `DEFINE` · **편집 가능한 목록(WbVarList)** · 패널(이름·타입·값·바뀜 표시) · 값 위 hover **—** · 미정의 경고 **—** | PL/SQL Developer · DBeaver | ✅ `SHOW VARIABLES` 결과 표 · 변수 창 · **GUI 로그의 `PRINT` = 값 · `VARIABLE` = 이름·타입**(mac 09-21) / ☐ hover · 미정의 경고 |
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

### 3-2. DBMS별 사용법 샘플 (09-21 · 실행해서 확인한 것만 담았다)

| 파일 | 실행 | 서버에 남는 것 |
|---|---|---|
| [examples/variables/oracle.sql](../examples/variables/oracle.sql) | `nsql run -c <프로필> examples/variables/oracle.sql SCOTT` | §1~7·§9 = 없음 · §8만 `NSQLT_VARS_DEMO` 프로시저를 만들고 지운다 |
| [examples/variables/mssql.sql](../examples/variables/mssql.sql) | `nsql run -c <프로필> examples/variables/mssql.sql 5` | 없음(세션 임시 `#…`) |
| [examples/variables/pg.sql](../examples/variables/pg.sql) | `nsql run -c <프로필> examples/variables/pg.sql 5` | 없음(`pg_temp.…`) |
| [examples/variables/sqlite.sql](../examples/variables/sqlite.sql) | `nsql run -c sqlite::memory: examples/variables/sqlite.sql KOREA` | 없음(메모리) |

네 파일은 **같은 절 순서**(선언·리터럴 → 서버 식 → `SELECT … INTO` → 살펴보기 → 바인드 → 프로시저·커서·다중 결과 → 치환 변수 → 범위·비밀)라 나란히 놓고 방언 차이를 볼 수 있다. 접속 없이 재작성만 보려면 `nsql plan -d <방언> <파일>`. MySQL/MariaDB·NoSQL은 드라이버가 없어 샘플도 없다. 검증 중 나온 흠 = [TODO](TODO.md) T-162.

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
| **V3 입력** | ✅ 09-21 89차(win): `ACCEPT [타입] [FORMAT] [DEFAULT] [PROMPT|NOPROMPT] [HIDE]`(입력 창 = 안내 글·기본값·가림 · 실행할 때마다) · 미정의 바인드 경고(실행 메시지 1회). ✅ 09-21 GUI 입력 창(실행당 **한 번** · 빠진 바인드 + `&` 매크로를 한 격자에서 · Skip = 종전 · 값은 탭에 기억 · `vars.undeclared`). ☐ 타입 열·미리보기 · `ACCEPT [HIDE] [DEFAULT]` · `COLUMN … NEW_VALUE` | 🔶 |
| **V4 범위·보존** | ✅ 09-21 D-135 계층(탭이 주인 · `VAR x SHARE|LOCAL` · 재접속 = 커서 무효화) · D-136 **파일별 보존**(`vars/<경로 해시>.sql` = 실행 가능한 스크립트 · 비밀·커서 제외) · 스크립트로 내보내기. ☐ 이름 없는 탭(S-1 hot exit와 함께) · 프로필 층 값의 출처(접속 프로필 필드) | 🔶 |
| **V5 능력표** | ✅ 09-21 89차 후반(win · 실서버): Oracle DATE·TIMESTAMP·BOOLEAN 진짜 바인드 · SQL Server 서명 → `OUTPUT` 보충(`OutputMarks`) · 선언 없는 글자 변수 넓히기. ✅ 09-21 89차(win): **`Caps` 포트**(`nsql-core/caps.rs` · 방언 분기 12곳 → 능력 질의 · 동작 불변) · **PG refcursor**(드라이버 `expand_refcursors`) · **PG 서명 → OUT 받기**(`call_captures`). ✅ 09-21 `Request.captures`(1행 잡기를 명시 · 자리 순서) · D-139 `vars.into_policy` · SQL Server `@@ROWCOUNT` 검사 · PG 배열 조각 오탐 — 실서버 통합 9/9. ☐ `Caps` 포트로 방언 `match` 걷어내기 · **PG refcursor(`FETCH ALL IN`)** · 서명 추론 PG/SQL Server/MySQL · DATE/TIMESTAMP/BOOLEAN 타입 | 🔶 |
| **V6 매크로** | ✅ 09-21 89차 끝(win): **`${env:이름[:형식]}`** = OS 환경 변수(sqlcmd `$(v)`의 환경 변수 폴백 · VS Code `${env:NAME}`과 같은 쓰임 · 없는 변수는 글자 그대로 · `vars.env_subst`). ✅ 09-21 89차 후반(win): 시스템 변수 9종(`Engine.sysvars` — 사용자 정의와 분리) · `${이름:형식}`(q·id·n·upper·lower) · `COLUMN … NEW_VALUE`. 남음 = 문자열 안 치환 선택(`define.in_strings`) · 상태줄 `&` 토글. ✅ 09-21 89차(win): CLI `-v 이름=값`. MacroStore 분리 · 시스템 변수 · `${v:형식}` · `-v name=value` · 문자열 안 치환은 선택(`define.in_strings`) · 상태줄 `&` 토글 | 🚧 |
| **V7 전수 점검** | ✅ 09-21 89차 후반(win): 값 크기 상한 `vars.max_value_kb`(1024) · 실서버 통합 13/13(Oracle·SQL Server·PG — 로컬 프로필). ✅ 09-21 1차 계측 `bench_vars`(§4 표) + 훑기 최적화 · 실서버 통합 9/9(Oracle·SQL Server·PG). ☐ 값 크기 상한 · 4방언 GUI 실기(Codespaces) · 변수 창·입력 창의 포커스/마우스 라우팅 실기표 | 🔶 |
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

## 9. 변수 안의 변수 — 확장 시점(대입 시 vs 사용 시) 조사·권장·구현 (09-22 · 사용자 요청)

문제(사용자 예):
```
:V1 := 2
:V2 := :V1 + 5      -- V2의 뜻 = "V1 + 5"인가, "7"인가
:V1 := 5
SELECT :V2 FROM DUAL;   -- 대입 시 확장 = 7 · 사용 시 확장 = 10
```

### 9-1. 다른 도구는 어떻게 하나 ([63a](63a-variable-research.md) 근거 + 언어 관례)

| 도구 · 언어 | 치환(매크로) 변수 | 바인드 변수의 식 | 시점 | 비고 |
|---|---|---|---|---|
| SQL\*Plus · SQLcl | `DEFINE a = &b` — `&b`는 그 줄을 읽을 때 바뀐다(전처리) | `EXEC :a := :b + 1` = 서버가 그때 계산 | **대입 시** | 저장된 값에 `&`가 남아도 다시 펴지 않는다 |
| psql | `\set a :b` — 메타 명령 인자에서 `:b`를 그때 끼운다 | `\gset` = 결과를 그때 저장 | **대입 시** | 문자열만 |
| sqlcmd · go-sqlcmd | `:setvar a $(b)` — 배치 전처리에서 `$(…)`를 그때 바꾼다[I] | `SET @a = @b + 1` 서버 | **대입 시** | 중첩 재확장 없음 |
| DBeaver | `@set a = ${b} + 5` — **원문 그대로 저장**, 쓸 때 한 번만 바꿈(중첩 `${b}`는 남는다) | 없음 | 원문 저장이지만 **재귀 없음** | 사실상 결함(63a 1.6) |
| SQL Workbench/J | `WbVarDef a=$[b]+5` — 원문 저장 · **쓸 때 재귀 확장**(자기 참조 가드) | 없음 | **사용 시** | 유일한 "사용 시" 도구 |
| Golden · Toad · PL/SQL Developer | 프롬프트/바인드 값 = 값만 | 서버 | 대입 시 | 식 저장 없음 |
| MySQL 클라이언트 · SSMS | 서버 세션 변수 `SET @a = @b + 1` | 서버 | 대입 시 | 클라이언트 변수 없음 |
| Make | `a = $(b)+5`(재귀 · **사용 시**) vs `a := $(b)+5`(단순 · **대입 시**) | — | **둘 다 · 문법으로 구분** | 사용 시가 기본이라 성능 함정으로 유명 |
| 셸(bash) · 파스칼/Go `:=` | 대입 시 | — | 대입 시 | 프로그래밍 관례 |
| 스프레드시트 | 수식 저장 · 의존 셀이 바뀌면 다시 계산 | — | **사용 시 + 의존 추적** | 값·수식을 따로 보여 준다 |

### 9-2. 방법 셋과 비용

| 방법 | 저장 | 읽을 때 | 성능 | 메모리 | 함정 |
|---|---|---|---|---|---|
| ① 대입 시(eager) | 확장된 값 | 그대로 | **0** | 값 하나 | 뒤에 바꾼 V1이 반영되지 않음(그러나 모든 SQL 도구·`:=` 관례) |
| ② 사용 시(lazy · 재귀) | 원문 | 쓸 때마다 재귀 치환(깊이 상한 · 순환 검출) | 쓸 때마다 O(원문 길이 × 깊이) — 치환 변수는 μs · **바인드 식은 서버 왕복** | 원문 하나 | 순환 · 값 표시가 "정의"와 "현재 값"으로 갈린다 |
| ③ 사용 시 + **의존 추적 캐시**(dirty) | 원문 + 마지막 값 + 의존 이름 | 의존이 바뀐 뒤 첫 사용에만 다시 계산 · 아니면 캐시 | ①과 같음(변경 없으면 0) | 원문 + 값 | 구현이 조금 더(의존 = 원문에 나온 변수 이름 · 대입마다 세대 번호) |

### 9-3. 권장과 결정

- **기본 = ① 대입 시**(`vars.expand_at = assign`) — SQL\*Plus·psql·sqlcmd·바인드 서버 계산·`:=` 관례와 같다 · 사용자가 놀라지 않고 비용 0.
- **선택 = ③ 사용 시(의존 추적)**(`vars.expand_at = use`) — 치환 변수(`DEFINE` · `&v` · `${v}` · `:setvar`)는 **클라이언트 재귀 확장**(깊이 16 · 순환 = 오류 `순환 참조 V2 → V1 → V2`) · 원문을 보관하고 `DEFINE`/변수 창은 **정의와 현재 값을 나란히** 보여 준다(사용자 09-22 "저장 안에 변수 방식으로 보관 · 조회 시점에 값을 별도 표시").
- 바인드 변수의 **식**(`EXEC :V2 := :V1 + 5`)은 서버가 계산하므로 "사용 시"는 **의존이 바뀐 뒤 첫 사용 전에 서버에서 다시 계산**(왕복 1 · 안 바뀌면 0)이어야 한다 — 러너가 "다음 문장 전에 재계획"을 해야 해서 **D-184 결정 대기**(아래) · 이번엔 치환 변수만.
- Make처럼 문법으로 나누는 안(`DEFINE a = …` vs `DEFINE a == …`)은 **보류** — 설정 하나가 단순하고, 필요하면 나중에 문법을 얹는다(D-185).

| # | 결정 | 권장 |
|---|---|---|
| D-184 | 바인드 식의 "사용 시" 재계산 | **의존 추적(dirty)** — ✅ 09-23 최소판: `VarStore.formulas`(수식·의존·stale) · 참조 문장 계획 때 stale이면 `EXEC :V := 식` + `Action::Replan` → 러너가 먼저 실행하고 재계획(journal §90) |
| D-185 | 변수별 시점 문법 | 보류(설정만) |

### 9-4. 구현(09-22 · 치환 변수)

- `nsql-script` `Settings.expand_at_use`(설정 **`vars.expand_at`** = `assign`(기본) \| `use`) — CLI·GUI 워커·러너가 같은 키로 배선.
- `assign`: `DEFINE a = 값`의 값은 **정의할 때** 치환한다(SQL\*Plus) — 종전엔 원문을 저장하고 쓸 때 한 번만 바꿔 중첩이 남았다(DBeaver와 같은 결함) → 이번에 고침.
- `use`: 원문 저장 · `&a`/`${a}`를 읽을 때 재귀 치환(깊이 16 · 순환 = `Action::Error`) · `DEFINE`(목록·한 개)은 `원문  →  현재 값`으로 보여 준다.
- 테스트 = 사용자 예(치환 변수판): assign → `2 + 5` · use → `5 + 5` · 순환 오류 · 깊이.

## 10. 내장 변수 층 — `${workspaceFolder}`처럼(09-23 · 사용자 "VS Code 같은 intrinsic 변수 · 시스템 환경 변수 위에 Nexa 층을 겹쳐 사용법은 동일")

- **층 순서**(한 해석기 · `Engine::substitute`): `DEFINE`/`&` 치환 변수 → **내장 표**(`Settings.intrinsic`) → 글자 그대로. `${env:이름}` = **내장 별칭(`NSQL_*`) → OS 환경 변수** — 이미 있던 `${env:…}` 문법으로 `${env:NSQL_PROJECT_DIR}`가 그대로 나온다(프로세스 환경 변수를 실제로 바꾸지는 않는다 · `set_var`는 스레드 안전하지 않고 자식 프로세스 상속도 원하지 않았다).
- **부품** = [nsql-script `intrinsic.rs`](../crates/nsql-script/src/intrinsic.rs): `Context`(호스트가 아는 것만 · 모르면 변수를 만들지 않는다) → `build()` → `BTreeMap<이름, 값>`(`Arc`로 엔진에 · 복사 0) · `lookup(이름)` · `expand(글, 표)`(설정·경로용 · 형식 없음 · `${env:}` 포함).
- **이름**: VS Code와 같은 것(`workspaceFolder` · `workspaceFolderBasename` · `workspaceFolder:이름` · `file` · `fileDirname` · `fileBasename` · `fileBasenameNoExtension` · `fileExtname` · `relativeFile` · `relativeFileDirname` · `fileWorkspaceFolder` · `fileDirnameBasename` · `lineNumber` · `columnNumber` · `userHome` · `cwd` · `execPath` · `pathSeparator`/`/` · `config:키`) + Nexa(`workspaceFile` · `workspaceName` · `nsqlHome` · `profile` · `dialect` · `os`) + 대문자 별칭(`NSQL_PROJECT_DIR` `NSQL_PROJECT_FILE` `NSQL_PROJECT_NAME` `NSQL_FILE` `NSQL_FILE_DIR` `NSQL_FILE_NAME` `NSQL_USER_HOME` `NSQL_HOME` `NSQL_CWD` `NSQL_EXEC` `NSQL_PROFILE` `NSQL_DIALECT` `NSQL_OS`). 대소문자 구분(VS Code와 같음) · 마지막 `:형식`(`q`·`id` 등)은 정확한 이름이 없을 때만 형식으로 본다(`workspaceFolder:ui`가 폴더 이름과 겹치지 않게).
- **배선**: GUI = 실행마다 `run_intrinsic()` 스냅숏(프로젝트 · 등록 폴더 · 활성 탭 경로·캐럿 · 홈 · `NSQL_HOME` · 실행 파일 · cwd · 활성 프로필·방언 · 설정 전부) → `Cmd::Run.intrinsic` · CLI = `intrinsic_vars(path, dialect)`(프로젝트 없음 = `${workspaceFolder}` 없음 · `${file}` = 스크립트 절대 경로) · 경로 설정(`log.file` · `oracle.client_dir` · `oracle.tns_admin`)은 읽을 때 `expand`.
- 설정 `vars.intrinsic`(Session · on) — 끄면 빈 표(= 층 없음). `vars.brace_subst`가 꺼져 있으면 `${…}` 자체가 돌지 않는다(기존 규칙).
- 시험: `intrinsic::tests` 3 + 엔진 `intrinsic_layer_between_defines_and_env`(DEFINE 우선 · 폴더 이름 · config · `:q` · env 별칭 · 모르는 이름 그대로). 사용자 문서 = [위키 Variables](wiki/Variables.md).
- 다음 후보: 프로젝트 파일 경로 앵커([73](73-project-path-portability.md) `${project}`·`${folder:이름}`)를 이 표와 **같은 이름**으로 맞춘다(`${workspaceFolder:이름}` = 73의 `${folder:이름}`) — T-172 P2에서 한 이름으로 통일.

## 11. 글로벌 변수 — 개념 정리·설계(사용자 09-25 · 변수 창 "Layer = tab · Declared = auto" 캡처)

### 11-1. 지금의 층(D-135 ③ · 구현 ✅)
| 층(`Layer`) | 변수 창 표시 | 주인 · 수명 | 넣는 법 | 보존 |
|---|---|---|---|---|
| **tab**(`Local`) | `tab` | 편집기 탭 하나 · 탭이 닫히면 끝 | 실행 중 대입·`VAR`·입력 창·변수 창 편집 | 파일 탭 = `vars/<경로 해시>.sql`(D-136 · `vars.persist`) |
| **shared**(`Shared`) | `shared` | **연결(세션)** — 같은 공유 세션에 묶인 탭 전부 · 재접속을 넘어 살고 커서만 무효 | `VAR x SHARE` · 변수 창 ↑ | 없음(세션과 함께) |
| **fixed**(`Fixed`) | (읽기 전용) | 접속 프로필의 고정 값 | 프로필 | 프로필 |

`Declared` 열: `decl` = `VAR`로 선언(타입 고정) · `auto` = 대입·OUT으로 암묵 생성(타입 추론). 우선순위 = tab > shared > fixed(같은 이름이면 앞 층이 이긴다).

### 11-2. 빠진 개념 = "글로벌"
- 요구: 어느 서버·어느 탭에서나 같은 값(예 `:PROJECT_CD` · `:V_USER`) · 앱을 다시 켜도 남음. 지금은 탭마다/연결마다 다시 넣거나 스크립트(`vars.script`)로 옮겨야 한다.
- 타 도구: SQL Workbench/J = 작업공간 변수(`WbVarDef` · 파일 저장) · DBeaver = 전역 변수 파일(서버끼리 섞임이 단점 · 63 §1) · cbq/usql = 세션 한정.

### 11-3. 설계(T-211 · 구현 = 이어서)
- 층 하나 추가: **global**(`Layer::Global`) — 앱 전역(모든 서버·세션·탭) · 우선순위 **tab > shared > global > fixed**(글로벌은 "기본값" 성격 · 탭/연결이 덮어쓴다) · 보존 = `NSQL_HOME/vars/global.sql`(`vars_to_script` 형식 · 실행 가능한 스크립트 · 비밀·커서 값 제외 · 바뀔 때 저장 · 시작 때 읽음).
- 넣는 법: `VAR x GLOBAL`(↔ `VAR x SHARE` · `VAR x LOCAL`) · 변수 창 Layer 열을 **드롭다운**(tab/shared/global)으로 · 우클릭 "글로벌로 올리기/내리기".
- 흐름: GUI `App.global_vars`(단일 원천) → 세션이 생기거나 값이 바뀔 때 워커 `Cmd::GlobalVars` → 러너 `VarStore.set_global` · 러너가 스크립트에서 `VAR x GLOBAL`로 올리면 `RunEvent::Vars{global}`로 되돌아와 앱이 갱신·저장 · 다른 세션에도 즉시 전파(브로드캐스트).
- 서버 간 오염 방지(DBeaver 단점): 글로벌은 **명시적으로 올린 값만**(자동 생성 금지) · 변수 창에 `global` 표시 · 로그 "변수 X 글로벌".
- 결정: **D-206** 우선순위(권장 tab > shared > global > fixed) · **D-207** 자동 저장(권장 켬 · `vars.global_persist`).
