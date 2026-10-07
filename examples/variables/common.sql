-- ═══════════════════════════════════════════════════════════════════════════════════════════════
-- 변수 — Nexa SQL 공통 언어 전부 (docs/63 §12 개념 총정리의 실행 가능한 판 · 10-07)
--   실행:  nsql run -c sqlite::memory: examples/variables/common.sql KOREA      (서버 불필요 · 아무것도 남지 않는다)
--   보기:  nsql plan -d sqlite examples/variables/common.sql KOREA              (접속 없이 — 문장 분리·로컬 대입·재작성만)
--   GUI :  SQLite 접속(파일 아무거나 또는 :memory:) 탭에서 열고 전체 실행(F5) · 값 = View ▸ Variables 창 · 표 = SHOW VARIABLES 결과 탭
--
-- 이 파일은 **DBMS를 가리지 않는 우리 앱의 변수 언어**만 쓴다(서버 식은 SQLite가 받아 주는 가장 평범한 SQL). 같은 글이 Oracle · SQL Server ·
-- PostgreSQL에서 어떻게 **재작성**되어 가는지는 같은 폴더의 oracle.sql · mssql.sql · pg.sql(절 번호 같음)에서 본다.
--
-- 주석 규약(모든 예제 공통):  -- [§절] 기능 — 효과 · 방언 대체: Oracle … / SQL Server … / PG … / MySQL …
--   §절 = docs/63-variable-management.md의 절 번호(§12 = 개념 총정리 · §11 = 층 · §9 = 확장 시점 · §10 = 내장 변수 · §3-1 = 방언별 기법).
--
-- 변수는 **다섯 층위**가 있고 섞이지 않는다(이름이 같아도 별개 · §12-1):
--   ① 바인드 변수 `:V`   = 타입을 가진 **값** · 서버에 진짜 바인드로 간다          (VAR · EXEC · PRINT · SHOW VARIABLES)
--   ② 치환 변수   `&v`   = 글자 매크로 · 서버로 가기 전에 글에 끼워진다              (DEFINE · UNDEFINE · ACCEPT · COLUMN NEW_VALUE · :setvar · ${v:형식})
--   ③ 시스템 변수 `&_USER` 등 9종 = 읽기 전용 · 실행 시점 값                        (치환 변수와 같은 자리에서 쓴다)
--   ④ 내장 변수   `${workspaceFolder}` · `${file}` · `${config:키}` = 앱이 아는 값   (VS Code 이름 · §10)
--   ⑤ 환경 변수   `${env:PATH}` = 내장 별칭(`NSQL_*`) → OS 환경 변수
-- ═══════════════════════════════════════════════════════════════════════════════════════════════

-- 시험용 표(메모리 · 파일에 안 남는다).
CREATE TABLE emp (id INTEGER PRIMARY KEY, name TEXT, dept TEXT, sal INTEGER, hired TEXT);
INSERT INTO emp VALUES
  (1, '홍길동', 'SALES', 300, '2024-03-02'),
  (2, '김철수', 'DEV',   450, '2025-01-15'),
  (3, '이영희', 'DEV',   520, '2026-07-01');


-- ══ 1. 바인드 변수 — 선언 · 리터럴 대입 (§12-2 · §3-1 첫 줄 · DB 왕복 0) ═══════════════════════════════════

-- [§12-2] 타입을 적어 선언 — 타입 **고정**(뒤에 다른 타입 값을 넣어도 타입은 그대로 · 바인드는 이 타입으로 간다).
--   방언 대체: Oracle SQL*Plus `VARIABLE v NUMBER`(같음) / SQL Server `DECLARE @v INT`(배치가 끝나면 사라짐 — 우리 변수는 GO를 넘어 산다)
--   / psql 없음(`\set`은 글자뿐) / MySQL `SET @v = …`(세션 변수 · 타입 없음)
VAR V_LIMIT NUMBER = 500
VAR V_NOTE  VARCHAR2(40) = 'it''s a note'

-- [§12-2] 타입 생략 + 값 = Auto(값에서 추론 · Golden 관용).
VAR V_TAG = 'demo'

-- [§12-2] 타입 없는 `VAR x` = **이 탭에 선언**(Auto · NULL) — 글로벌/공유에 같은 이름이 있으면 이 탭에서 **가린다**(§11-4 · 10-07).
--   이미 이 탭/공유에 있으면 정보만 보여 준다(SQL*Plus의 "변수 보기" 뜻은 그때만).
VAR V_PLAIN

-- [§12-2] 리터럴 대입 = **클라이언트에서 끝난다**(왕복 0 · 전 방언 동일 · DR-8). 선언 없이 대입하면 그 자리에서 생긴다(자동 타입).
EXEC :V_DEPT := 'DEV'
EXEC :V_N    := 1999
-- NULL도 값이다(변수를 지우지 않는다 · 타입은 유지).
EXEC :V_EMPTY := NULL

-- [§11-4] 자동 타입 변수 = **지금 값**을 따라간다(10-07 보완 · 종전엔 첫 값에서 한 번만 추론해 글자를 넣어도 NUMBER로 남았다).
EXEC :V_N := 'Number to String'
-- → SHOW VARIABLES에서 V_N 타입 = VARCHAR2(16). 반대로 타입을 적어 선언한 V_LIMIT는 글자를 넣어도 NUMBER로 남는다(고정).

-- [§11-4] 타입은 **엄격하게** 읽는다 — 아래 둘은 오류(변수를 만들지 않는다). 종전엔 `VARCHAR2(50) GLOBAL`이 조용히 VARCHAR2(4000) 탭 변수가 됐다.
--   VAR V_BAD VARCHAR2(abc)
--   VAR V_BAD VARCHAR2(50) GLOBAAL
-- 길이 뒤 단위·정밀도는 받아 준다: `VARCHAR2(50 CHAR)` = 50 · `NUMBER(10,2)` = NUMBER.


-- ══ 2. 서버 식 대입 · 한 행 → 여러 변수 (§12-2 · §3-1 둘째·셋째 줄) ═════════════════════════════════════════

-- [§3-1] `EXEC :V := 식` — 식은 **서버**가 계산한다. 방언 대체: Oracle `BEGIN :V := 식; END;`(OUT 바인드) / SQL Server
--   `DECLARE @V sql_variant = …; SET @V = 식; SELECT @V, [V$type]`(꼬리 행 · 타입 복원 D-254) / PG·SQLite `SELECT (식) AS "V"` 1행 잡기 / MySQL 같음.
EXEC :V_MAX   := (SELECT MAX(sal) FROM emp WHERE dept = :V_DEPT)
EXEC :V_TODAY := date('now')

-- [§3-1] `EXEC SELECT … INTO :A, :B` — 1행을 **자리 순서로** 여러 변수에. 0행·여러 행 = 오류(Oracle 정책 · 설정 `vars.into_policy` = first면 첫 행 · D-139).
--   방언 대체: Oracle 그대로(OUT 바인드 블록) / SQL Server `SELECT @A = a, @B = b` + `@@ROWCOUNT` 검사(T-SQL은 여러 행이면 말없이 마지막 행) /
--   PG·SQLite = INTO를 떼고 보낸 뒤 클라이언트가 받는다 / MySQL 서버 `INTO @v`는 0행에 옛 값이 남아 피한다.
EXEC
SELECT name, sal
INTO   :V_TOP_NAME, :V_TOP_SAL
FROM   emp
WHERE  sal = :V_MAX

-- [§11-4] 자동 변수의 타입 변화를 SELECT INTO 경로에서도: 첫 줄 = NUMBER · 둘째 줄 = 글자 → VARCHAR2 / NUMBER로 바뀐다.
--   (FROM 없는 `SELECT … INTO`는 아직 INTO를 못 뗀다 = T-162 알려진 흠 → FROM을 둔다 · Oracle은 `FROM DUAL`)
EXEC SELECT 1999, 'Year' INTO :EXE1, :EXE2 FROM emp WHERE id = 1
EXEC SELECT 'Number to String', 2026 INTO :EXE1, :EXE2 FROM emp WHERE id = 1


-- ══ 3. 살펴보기 (§12-5) ═══════════════════════════════════════════════════════════════════════════════

-- [§12-5] PRINT 이름… = 값(GUI 로그 = 값 · 비밀 이름은 ******). 방언 대체: SQL*Plus `PRINT` 같음 / SQL Server `PRINT 'text'`·`PRINT @v`는 서버 문장(글자 상수·@면 서버로).
PRINT V_DEPT V_MAX V_TOP_NAME V_TOP_SAL V_N EXE1 EXE2
-- [§12-5] VARIABLE(인자 없음) = 선언 목록(이름 · 타입).
VARIABLE
-- [§12-5] SHOW VARIABLES = **모든 층**의 표(Name · Type · Value · Layer · Declared · Active) — 같은 이름은 tab → shared → global → profile 순 ·
--   `Active` `*` = 지금 쓰이는 값 · 빈칸 = 앞 층에 가려짐(10-07). 변수 창(View ▸ Variables)은 가려진 줄을 흐리게 보인다.
SHOW VARIABLES


-- ══ 4. 뒤 문장에서 바인드로 (§12-3 · D-142 이름은 대소문자 무시) ═════════════════════════════════════════

-- [§12-3] 값이 오는 자리에 `:V` — 진짜 바인드(글자 치환 아님 · 주입 안전 · 서버 계획 재사용). 방언 대체: Oracle 이름 바인드 / SQL Server
--   `sp_executesql` 인자 / PG `$n` + 선언 타입 캐스트 / SQLite `:x`(`@x` `$x`도 한 변수 · D-142) / MySQL `?`.
SELECT name, sal FROM emp WHERE dept = :V_DEPT AND sal < :V_LIMIT ORDER BY sal;
SELECT :v_dept AS dept, :V_Max AS max_sal, :v_top_name AS top_name;
-- [§12-3] 테이블 함수도 보통 조회 + 바인드(Oracle `TABLE(f(:x))` / SQL Server `dbo.f(@p)` / PG SRF / SQLite `json_each`).
EXEC :V_JSON := '[10, 20, 30]'
SELECT value FROM json_each(:V_JSON) WHERE value >= 20;


-- ══ 5. 층(Scope) — 탭 · 공유 · 글로벌 · 프로필 (§11 · D-135 · D-206 · D-264) ═══════════════════════════════

-- 층 넷 = tab(탭 하나) > shared(연결 공유 · 같은 세션의 탭 전부) > global(앱 전역 · 모든 서버·탭 · 디스크 보존 vars/global.sql) > profile(접속 프로필 · 읽기 전용).
-- 같은 이름은 **앞 층이 가린다**. 방언 대체: 다른 도구에는 이런 층이 없다 — SQL Workbench/J 작업공간 변수(글로벌 비슷) · DBeaver 전역 파일(서버끼리 섞임 · 우리는 명시 선언만).

-- [§11-3] `VAR x GLOBAL`(값 없음) = 없으면 **빈 글로벌로 선언** · 있으면 올리기(탭/공유 → 글로벌). SHARE · LOCAL도 같은 꼴.
VAR G1 GLOBAL
-- [§11-4 D-264] 대입은 **그 이름이 사는 층**에 쓴다(tab → shared → global 순으로 찾아 있는 층 · 어디에도 없으면 tab) — 글로벌 G1이 10이 된다.
EXEC :G1 := 10
-- [§11-4] 선언 + 층 한 줄: `VAR 이름 타입 [= 값] 층` 또는 `VAR 이름 층 타입 [= 값]` · 타입 생략 = Auto.
VAR G2 NUMBER = 7 GLOBAL
VAR G3 GLOBAL = 'three'
VAR S1 VARCHAR2(30) = 'shared one' SHARE
-- 지금: G1(global 10) · G2(global 7) · G3(global 'three') · S1(shared) — 다른 연결·다른 탭에서도 **바로** 보인다(글로벌 브로드캐스트 · 10-07).
SHOW VARIABLES

-- [§11-4] 이 탭에서만 다른 값을 쓰려면(가림) = **명시 선언**: `VAR G1`(Auto) 또는 `VAR G1 NUMBER = 5`. 그다음 대입은 탭에 간다 · 글로벌은 그대로.
VAR G1
EXEC :G1 := 5
PRINT G1
-- → 5 · SHOW VARIABLES = `G1 NUMBER 5 tab *` + `G1 NUMBER 10 global`(Active 빈칸 = 가려짐).
SHOW VARIABLES

-- [§11-4] `VAR x DROP` = 탭 층의 x 제거(탭에 없으면 공유 층) → 가려져 있던 글로벌이 다시 보인다.
VAR G1 DROP
PRINT G1
-- → 10
-- [§11-4] `VAR x DROP GLOBAL` = 글로벌 층까지 제거 → 없는 변수.
VAR G1 DROP GLOBAL
--   PRINT G1   → "변수 G1가 없습니다"

-- [§11-1] 층 옮기기(값은 따라간다): SHARE = 탭 → 연결 공유 · LOCAL = 공유 → 탭(= UNSHARE) · GLOBAL = 글로벌로.
VAR V_DEPT SHARE
VAR V_DEPT LOCAL

-- [§11-4] `VAR CLEAR` = 이 탭 변수 전부(공유·글로벌·프로필은 남음) · `VAR CLEAR GLOBAL` = 글로벌도 · `VAR CLEAR ALL` = 공유까지.
--   아래는 보여 주기용이라 주석 — 풀면 이 파일의 나머지 절이 쓰는 변수도 지워진다.
--   VAR CLEAR
--   VAR CLEAR GLOBAL
--   VAR CLEAR ALL
-- 정리(이 파일이 만든 글로벌·공유는 지운다 — 다음 실행·다른 탭에 남지 않게).
VAR G2 DROP GLOBAL
VAR G3 DROP GLOBAL
VAR S1 DROP
SHOW VARIABLES

-- [§12-6] 세션(연결)을 바꾸면: tab 층은 탭을 따라간다(타입은 추상 → 바인드 때 방언으로) · shared 층은 **새 연결의 것으로 바뀐다** · global 같음 ·
--   REFCURSOR 값은 무효 · `vars.expand_at = use`의 수식은 새 방언으로 재계산된다(Oracle 전용 식이면 오류).


-- ══ 6. 치환 변수 — 글자 매크로 (§12-4 · D-141) ═══════════════════════════════════════════════════════════

-- [§12-4] DEFINE 이름 = 글 · `&이름` = 그대로 끼운다(식별자·조각도 됨) · 뒤의 `.`은 이름 종결자(`&tab..id` → `emp.id`).
--   방언 대체: SQL*Plus `DEFINE`/`&`(같음) / sqlcmd `:setvar v 값`·`$(v)`(우리는 `:setvar`도 같은 저장소 · 참조는 `&v`) / psql `\set v 값`·`:v` / DBeaver `${v}`·`@set`.
DEFINE bonus = 10
DEFINE tab = emp
SELECT name, sal + &bonus AS with_bonus FROM &tab WHERE &tab..id = 1;
-- [§12-4] `:setvar`(sqlcmd) = DEFINE과 같은 저장소.
:setvar Top 2
SELECT name FROM emp ORDER BY sal DESC LIMIT &Top;
-- [§12-4] UNDEFINE = 치환 변수 제거(바인드 변수 `VAR x DROP`과 저장소가 다르다).
UNDEFINE Top
-- [§12-4] 스크립트 인자 `&1 &2 …` = `nsql run … common.sql KOREA` → &1 = KOREA. 방언 대체: SQL*Plus 같음 / sqlcmd `-v` / psql `-v`.
SELECT '&1' AS arg1;
-- [§12-4] `${이름:형식}` 안전 인용형(psql `:'v'`·`:"v"` 차용): q = SQL 글자 상수(따옴표 두 번 · SQL Server는 N'…' · MySQL은 역슬래시도) ·
--   id = 인용한 이름(Oracle/PG/SQLite "…" · SQL Server […] · MySQL `…`) · n = 수일 때만(아니면 그대로 = 주입 방지) · upper/lower · raw.
DEFINE who = "O'Neil"
SELECT ${who:q} AS quoted, ${bonus:n} AS num, '${tab:upper}' AS up;
SELECT COUNT(*) AS c FROM ${tab:id};

-- [§12-4 시스템 변수] 읽기 전용 9종(값은 실행 시점): _USER _CONNECT_IDENTIFIER _DIALECT _DATE _TIMESTAMP _ROW_COUNT _SQLCODE _ELAPSED_MS _FILE.
--   방언 대체: SQL*Plus `_USER` `_DATE` `_CONNECT_IDENTIFIER` / psql `:ROW_COUNT` `:SQLSTATE` / sqlcmd `$(SQLCMDUSER)`.
SELECT '&_DIALECT' AS dialect, '&_DATE' AS today, '&_ROW_COUNT' AS last_rows, '&_ELAPSED_MS' AS last_ms;

-- [§10 내장 변수] VS Code 이름 그대로(`${file}` `${fileBasename}` `${workspaceFolder}`(프로젝트가 있을 때) `${userHome}` `${config:키}` …).
--   CLI에서는 `${file}` = 이 스크립트의 절대 경로. 모르는 이름은 글자 그대로(묻지 않음). 설정 `vars.intrinsic`(끄면 층 없음).
SELECT '${fileBasename}' AS this_file, '${config:db.fetch_size}' AS fetch_size, '${os}' AS os;

-- [§12-4 환경 변수] `${env:이름}` = 내장 별칭(NSQL_*) → OS 환경 변수(설정 `vars.env_subst` · 없는 변수는 글자 그대로).
SELECT ${env:PATH:q} IS NOT NULL AS has_path, '${env:NSQL_DIALECT}' AS via_alias;

-- [§12-4] COLUMN 열 NEW_VALUE 변수 = 결과 그 열의 **마지막 행 값**을 치환 변수로(SQL*Plus 그대로 · 전 방언). CLEAR = 해제.
COLUMN max_id NEW_VALUE last_id
SELECT MAX(id) AS max_id FROM emp;
SELECT name FROM emp WHERE id = &last_id;
COLUMN max_id CLEAR

-- [§12-4] ACCEPT = 값을 물어서 치환 변수로(GUI = 입력 창 · CLI = 터미널 · `--no-prompt`면 기본값 · HIDE = 가려 입력 = 비밀 D-140).
--   선언도 값도 없는 **바인드**를 읽으면 GUI는 실행당 한 번 입력 창으로 묻는다(D-137 · 설정 `vars.undeclared`).
--   ACCEPT min_sal NUMBER DEFAULT 400 PROMPT 'Minimum salary: '
--   SELECT name FROM emp WHERE sal >= &min_sal;

-- [§12-4 D-141] `&`가 든 자료를 넣을 때는 끈다(주석 안은 원래 치환 안 함 · 문자열 안은 설정 `define.in_strings`).
SET DEFINE OFF
SELECT 'R&D' AS literal_amp;
SET DEFINE ON


-- ══ 7. 변수 안의 변수 — 확장 시점 설정 `vars.expand_at` (§9 · D-184 · 사용자 10-07 "설정을 바꿔 사용 시 확장") ══════

-- 설정은 스크립트 안이 아니라 **환경 설정**에서 바꾼다(SQL 문장이 아니므로):
--   GUI  = 환경 설정 ▸ 접속 ▸ 스크립트·변수 ▸ "변수 안의 변수 확장 시점" (assign | use)
--   CLI  = nsql config set vars.expand_at use      (되돌리기: nsql config set vars.expand_at assign)
-- 이 절은 두 값으로 **각각 한 번씩** 실행해 결과를 비교한다(주석의 기대값 참고).

-- [§9 치환 변수] assign(기본) = DEFINE할 때 바로 편다(SQL*Plus · psql · sqlcmd · Make `:=`) · use = 원문을 두고 쓸 때 재귀로 편다(Make `=` · SQL Workbench/J).
DEFINE v1 = 2
DEFINE v2 = &v1 + 5
DEFINE v1 = 5
SELECT &v2 AS v2;
-- → assign: 2 + 5 = 7 · use: 5 + 5 = 10
-- use 모드에서 DEFINE 목록·변수 창은 `원문 → 현재 값`(`&v1 + 5 → 5 + 5`)으로 보인다. 순환(`DEFINE a = &b` · `DEFINE b = &a`)은 오류 · 깊이 16.
DEFINE

-- [§9 바인드 식 · D-184] use 모드에서는 **바인드 수식**도 의존이 바뀐 뒤 처음 쓰는 문장 앞에서 서버에서 다시 계산한다(dirty 추적 · 왕복 1 · 안 바뀌면 0).
EXEC :B1 := 2
EXEC :B2 := :B1 + 5
EXEC :B1 := 5
SELECT :B2 AS b2;
-- → assign: 7(대입 시 값 고정 · 모든 SQL 도구의 관례) · use: 10(`:B2 := :B1 + 5`를 다시 계산한 뒤 조회)
-- 주의: 수식은 **지금 세션의 방언**으로 돈다 — Oracle 전용 식을 등록하고 세션을 SQL Server로 바꾸면 재계산이 오류를 낸다(§12-6).


-- ══ 8. 비밀 · 보존 (§12-6 · D-136 · D-140 · D-207) ═══════════════════════════════════════════════════════

-- [§12-6 D-140] 이름에 PASS · PWD · SECRET · TOKEN이 들면 비밀 — 창·로그·SHOW VARIABLES에서 ******, 파일에 저장하지 않는다. ACCEPT HIDE도 같다.
EXEC :V_API_TOKEN := 'abc123'
PRINT V_API_TOKEN
SHOW VARIABLES
-- [§12-6 D-136] 탭 변수는 파일별로 보존된다(설정 `vars.persist` · `vars/<경로 해시>.sql` = 실행 가능한 스크립트 · 비밀·커서 제외) —
--   다음에 이 파일을 열면 이어서 쓴다. 글로벌은 `vars/global.sql`(설정 `vars.global_persist` · D-207). 공유 층은 세션과 함께 사라진다.
-- [§12-6] 변수 값 크기 상한 = 설정 `vars.max_value_kb`(1024) · 변수 창·로그는 앞부분만 그린다.
