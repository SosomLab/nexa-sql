-- ═══════════════════════════════════════════════════════════════════════════════════════════════
-- 변수 사용법 — Oracle (docs/63 · SQL*Plus / Golden 관용 그대로)
--   실행:  nsql run -c <프로필> examples/variables/oracle.sql SCOTT
--   보기:  nsql plan -d oracle examples/variables/oracle.sql SCOTT     (접속 없이 — 문장 분리·대입·재작성만)
--   GUI :  Oracle 접속 탭에서 열고 전체 실행(F5) · 값 = View ▸ Variables 창 · 커서 = 이름 붙은 결과 탭
-- §1~§7은 서버에 **아무것도 만들지 않는다**. §8만 프로시저 하나(NSQLT_VARS_DEMO)를 만들고 끝에서 지운다.
-- 변수는 두 가지다 — 섞이지 않는다(이름이 같아도 별개):
--   바인드 변수 `:V`   = 타입을 가진 값 · 서버에 **진짜 이름 바인드**로 간다 (VAR · EXEC · PRINT)
--   치환 변수   `&v`   = 글자 매크로 · 서버로 가기 전에 글에 끼워진다       (DEFINE · ACCEPT · COLUMN NEW_VALUE · ${v:형식})
-- ═══════════════════════════════════════════════════════════════════════════════════════════════


--	CONNECT oracle://BISCM@192.168.0.58:1521/BISCM


-- ── 1. 선언 · 리터럴 대입 (DB 왕복 0) ──────────────────────────────────────────────────────
VAR V_LIMIT NUMBER = 3
VAR V_NOTE  VARCHAR2(40) = 'it''s a note'
EXEC :V_PRG_NM := 'SP_DEMO'
-- 선언 없이 대입해도 된다(Golden식 암묵 선언 — 진짜 SQL*Plus는 SP2-0552). 탭으로 줄을 맞춘 관용도 그대로.
EXEC	:V_OWNER			:=	'SYS';

-- ── 2. 서버 식 대입 — `BEGIN :V := 식; END;` 로 간다(OUT 바인드) ─────────────────────────────
EXEC :V_USER := USER
EXEC :V_CNT  := 1 + 2

SELECT
	:V_LIMIT
,	:V_NOTE
,	:V_PRG_NM
,	:V_OWNER
,	:V_USER
,	:V_CNT
FROM
	DUAL
;

-- 날짜·시각·불리언은 **진짜 타입**으로 바인드된다(글자로 바뀌어 시각이 잘리지 않는다 · DUMP = Typ=12).
VAR V_D  DATE
VAR V_TS TIMESTAMP
VAR V_B  BOOLEAN
EXEC :V_D  := TO_DATE('2026-09-21 13:45:10', 'YYYY-MM-DD HH24:MI:SS')
EXEC :V_TS := SYSTIMESTAMP
EXEC :V_D  := :V_D + 1
SELECT TO_CHAR(:V_D, 'YYYY-MM-DD HH24:MI:SS') AS d, SUBSTR(DUMP(:V_D), 1, 6) AS typ FROM DUAL;
BEGIN :V_B := (1 = 1); END;
/

-- ── 3. 한 행 → 여러 변수 — 맨 `EXEC` 다음 줄부터 여러 줄 SELECT … INTO 도 된다 ────────────────────
--   0행 = ORA-01403 · 여러 행 = ORA-01422 (설정 vars.into_policy = first면 첫 행)
EXEC
SELECT
	COUNT(*)
,	MIN(OBJECT_NAME)
INTO
	:V_OBJ_CNT
,	:V_FIRST_OBJ
FROM
	ALL_OBJECTS
WHERE 1=1
AND	OWNER			=	:V_OWNER
AND	ROWNUM			<=	:V_LIMIT

-- PL/SQL 블록 안의 바인드도 같은 변수 표를 쓴다(IN/OUT 둘 다).
BEGIN
  :V_TWICE := :V_FIRST_OBJ || '/' || :V_FIRST_OBJ;
END;
/

-- ── 4. 살펴보기 ───────────────────────────────────────────────────────────────────────────
PRINT V_PRG_NM V_USER V_OBJ_CNT V_FIRST_OBJ V_TWICE
VARIABLE
SHOW VARIABLES

-- ── 5. 뒤 문장에서 바인드로 쓴다(이름은 대소문자 무시 · D-142) ────────────────────────────────
SELECT
	:V_PRG_NM		AS	PRG_NM
,	:V_USER			AS	USR
,	:V_OBJ_CNT		AS	OBJ_CNT
,	:v_first_obj	AS	FIRST_OBJ
FROM
	DUAL
;

-- 테이블 함수 + 바인드
SELECT COLUMN_VALUE AS n FROM TABLE(SYS.ODCINUMBERLIST(1, 2, 3, 4, 5)) WHERE COLUMN_VALUE >= :V_LIMIT;

-- DBMS_OUTPUT · 서명 추론: 선언 없는 바인드가 패키지의 OUT 인자(VARCHAR2 · INTEGER)를 받는다.
SET SERVEROUTPUT ON
EXEC DBMS_OUTPUT.PUT_LINE('hello ' || :V_PRG_NM)

-- ── 6. REF CURSOR · 다중 결과 ──────────────────────────────────────────────────────────────
-- 커서 변수: 실행 뒤 **바로 결과**(라벨 = 변수 이름 · GUI = 결과 탭 `RC`). 설정 run.cursor_autoshow 를 끄면 PRINT RC 로.
VAR RC REFCURSOR
EXEC OPEN :RC FOR SELECT OBJECT_NAME, OBJECT_TYPE FROM ALL_OBJECTS WHERE OWNER = :V_OWNER AND ROWNUM <= :V_LIMIT

-- 암묵 결과(DBMS_SQL.RETURN_RESULT) — 장마다 결과 탭 "ResultSet #n"
DECLARE
  c1 SYS_REFCURSOR;
  c2 SYS_REFCURSOR;
BEGIN
  OPEN c1 FOR SELECT 1 AS one FROM DUAL;
  DBMS_SQL.RETURN_RESULT(c1);
  OPEN c2 FOR SELECT LEVEL AS n FROM DUAL CONNECT BY LEVEL <= 3;
  DBMS_SQL.RETURN_RESULT(c2);
END;
/

-- ── 7. 치환 변수 ─────────────────────────────────────────────────────────────────────────
DEFINE own = SYS
DEFINE tab = ALL_OBJECTS
-- `&이름` = 그대로 끼운다(식별자·조각도) · 뒤의 `.`은 이름 종결자(`&tab..OWNER` → `ALL_OBJECTS.OWNER`)
SELECT COUNT(*) AS c FROM &tab WHERE &tab..OWNER = '&own' AND ROWNUM <= 10;
-- 스크립트 인자: `nsql run … oracle.sql SCOTT` → &1 = SCOTT
SELECT '&1' AS arg1 FROM DUAL;
-- `${이름:형식}` — 안전 인용형. q = 글자 상수('…' · 따옴표 두 번) · id = "인용한 이름" · n = 수일 때만 · upper/lower
DEFINE who = "O'Neil"
SELECT ${who:q} AS quoted, '${own:lower}' AS low FROM DUAL;
-- 시스템 변수: _USER _CONNECT_IDENTIFIER _DIALECT _DATE _TIMESTAMP _ROW_COUNT _SQLCODE _ELAPSED_MS _FILE
SELECT '&_USER' AS usr, '&_CONNECT_IDENTIFIER' AS db, '&_DATE' AS today, '&_ROW_COUNT' AS last_rows FROM DUAL;
-- OS 환경 변수(설정 vars.env_subst · 없는 변수는 글자 그대로)
SELECT ${env:PATH:q} AS os_path FROM DUAL;

-- 결과의 열 값 → 치환 변수(그 열의 마지막 행 값)
COLUMN today NEW_VALUE run_date
SELECT TO_CHAR(SYSDATE, 'YYYYMMDD') AS today FROM DUAL;
PROMPT run date = &run_date
COLUMN today CLEAR

-- 값을 물어서 받기(GUI = 입력 창 · CLI = 터미널 · `--no-prompt`면 기본값). HIDE = 가려서 입력.
--   ACCEPT p_owner CHAR DEFAULT 'SYS' PROMPT 'Owner: '
--   SELECT COUNT(*) FROM ALL_TABLES WHERE OWNER = '&p_owner';
-- 선언도 값도 없는 바인드를 **읽으면** GUI는 실행당 한 번 입력 창으로 묻는다(D-137 · 설정 vars.undeclared).

-- `&`가 든 자료를 넣을 때는 끈다.
SET DEFINE OFF
SELECT 'R&D' AS literal_amp FROM DUAL;
SET DEFINE ON

-- ── 8. 프로시저 호출 — OUT · 커서 2개 (⚠ 객체 하나를 만들고 지운다) ────────────────────────────
CREATE OR REPLACE PROCEDURE NSQLT_VARS_DEMO (
  PC_A   OUT SYS_REFCURSOR,
  PC_B   OUT SYS_REFCURSOR,
  P_N    IN  NUMBER,
  P_MSG  OUT VARCHAR2
) AS
BEGIN
  OPEN PC_A FOR SELECT LEVEL AS n FROM DUAL CONNECT BY LEVEL <= P_N;
  OPEN PC_B FOR SELECT 'row ' || LEVEL AS label FROM DUAL CONNECT BY LEVEL <= P_N;
  P_MSG := 'opened 2 cursors x ' || P_N;
END;
/
SHOW ERRORS

-- **선언 없이** 호출한다 — 타입·방향은 서명(ALL_ARGUMENTS)에서 온다(루틴당 1회 조회 · 설정 vars.signature_lookup).
-- 결과 = 탭 `PC_A` · `PC_B` + 변수 V_MSG.
EXEC NSQLT_VARS_DEMO(:PC_A, :PC_B, :V_LIMIT, :V_MSG)
PRINT V_MSG
-- 이름 표기도 된다.
EXEC NSQLT_VARS_DEMO(P_N => 2, PC_A => :PC_A, PC_B => :PC_B, P_MSG => :V_MSG)

DROP PROCEDURE NSQLT_VARS_DEMO;

-- ── 9. 층(Scope) — 탭 · 공유 · 글로벌 · 프로필 (docs/63 §11 · D-135 · D-206 · D-264 · 10-07) ───────────────
-- 공통 언어 전부 = common.sql §5. 여기서는 Oracle 식으로 같은 흐름.
-- [§11-1] 변수 표의 주인은 **탭**(D-135). 같은 연결의 탭과 나누려면 SHARE · 되돌리려면 LOCAL. 값은 CONNECT/재접속을 넘어 살고 **커서만** 무효.
VAR V_OWNER SHARE
VAR V_OWNER LOCAL
-- [§11-3] 글로벌 = 모든 서버·탭 공통 · 디스크 보존(vars/global.sql) · 명시 선언으로만 생긴다. 선언 + 층 한 줄(타입·값 포함).
VAR G_PROJECT VARCHAR2(30) = 'SEBANG' GLOBAL
VAR G_LIMIT GLOBAL
-- [§11-4 D-264] 대입은 그 이름이 **사는 층**에 — G_LIMIT는 글로벌이라 글로벌 값이 3이 된다(SELECT INTO · OUT도 같다).
EXEC :G_LIMIT := 3
SELECT :G_PROJECT AS prj, :G_LIMIT AS lim FROM DUAL;
-- [§11-4] 이 탭에서만 다른 값 = 명시 선언으로 가림(`VAR 이름` 또는 `VAR 이름 타입 [= 값]`) → 뒤 대입은 탭에 · 글로벌 그대로.
VAR G_LIMIT NUMBER = 1
SHOW VARIABLES
-- → G_LIMIT NUMBER 1 tab * / G_LIMIT NUMBER 3 global (Active 빈칸 = 가려짐)
-- [§11-4] DROP = 탭 층 제거 → 글로벌 3 복귀 · DROP GLOBAL = 글로벌까지 · CLEAR [GLOBAL|ALL] = 층 단위 전부(common.sql §5).
VAR G_LIMIT DROP
PRINT G_LIMIT
VAR G_LIMIT DROP GLOBAL
VAR G_PROJECT DROP GLOBAL
--   방언 대체: SQL*Plus에는 층·DROP이 없다(변수는 세션 끝까지) · PL/SQL Developer 격자는 "오래된 변수는 지우지 않고 끔" · SQL Workbench/J `WbVarDelete`.

-- ── 10. 자동 타입 (§11-4 · 10-07) ────────────────────────────────────────────────────────
-- [§11-4] 선언 없이 생긴 변수는 **값마다** 타입을 다시 추론(1999 → NUMBER · 'Number to String' → VARCHAR2) · 타입을 적어 선언하면 고정.
EXEC SELECT 1999, 'Year' INTO :EXE1, :EXE2 FROM DUAL
EXEC SELECT 'Number to String', 2026 INTO :EXE1, :EXE2 FROM DUAL
SHOW VARIABLES
-- [§11-4] 타입은 엄격하게 읽는다 — `VAR X VARCHAR2(50) GLOBAAL`·`VARCHAR2(abc)`는 오류(변수 안 생김). 종전엔 조용히 VARCHAR2(4000)이 됐다.

-- ── 11. 변수 안의 변수 — 확장 시점 설정 `vars.expand_at` (§9 · D-184) ──────────────────────────
-- 설정에서 바꾼다(GUI 환경 설정 ▸ 스크립트·변수 · CLI `nsql config set vars.expand_at use`). 두 모드로 각각 실행해 비교.
DEFINE v1 = 2
DEFINE v2 = &v1 + 5
DEFINE v1 = 5
SELECT &v2 AS v2 FROM DUAL;          -- assign: 7 · use: 10
EXEC :B1 := 2
-- 서버 식(BEGIN :B2 := :B1 + 5; END;) · use 모드 = 수식으로 등록
EXEC :B2 := :B1 + 5
EXEC :B1 := 5
SELECT :B2 AS b2 FROM DUAL;          -- assign: 7 · use: 10(:B1이 바뀐 뒤 처음 쓰는 문장 앞에서 서버 재계산 1회)
--   방언 대체: SQL*Plus·psql·sqlcmd = 전부 대입 시(assign) · 사용 시는 SQL Workbench/J `$[v]`뿐 · Make `=`/`:=`.

-- ── 12. 비밀 · 보존 (§12-6 · D-136 · D-140) ─────────────────────────────────────────────────
-- 이름에 PASS · PWD · SECRET · TOKEN이 들면 비밀 — 창·로그·SHOW VARIABLES에서 ******, 파일에 저장하지 않는다(D-140). ACCEPT HIDE도 같다.
EXEC :V_API_TOKEN := 'abc123'
SHOW VARIABLES
-- 탭 변수는 파일별 보존(vars/<경로 해시>.sql · 비밀·커서 제외) · 글로벌 = vars/global.sql · 공유 = 세션과 함께.

-- ── 부록. SQL*Plus / Golden 과 우리 앱 — 같은 글이 어떻게 다른가 ───────────────────────────────
--   같다   : VARIABLE · EXEC · PRINT · DEFINE/UNDEFINE/&/&& · ACCEPT · COLUMN NEW_VALUE · SET DEFINE OFF · @script &1
--   더 있다: 선언 없는 바인드(Golden 관용 · 진짜 SQL*Plus는 SP2-0552) · 층(SHARE/GLOBAL · DROP/CLEAR) · ${v:q|id|n} 안전 인용형 · 시스템 변수 9종 ·
--            내장 변수 ${file} ${workspaceFolder} · ${env:} · SHOW VARIABLES 표 · 변수 창 · 파일별 보존 · 확장 시점 설정
--   다르다 : DATE/TIMESTAMP/BOOLEAN이 **진짜 타입**으로 바인드된다(SQL*Plus는 DATE 바인드가 없어 글자로) · 커서는 결과 탭으로 바로
