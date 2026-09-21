-- ═══════════════════════════════════════════════════════════════════════════════════════════════
-- 변수 사용법 — SQLite (docs/63 · 서버 없이 그대로 돈다)
--   실행:  nsql run -c sqlite::memory: examples/variables/sqlite.sql KOREA
--   보기:  nsql plan -d sqlite examples/variables/sqlite.sql KOREA      (접속 없이 — 문장 분리·대입·재작성만)
--   GUI :  SQLite 접속 탭에서 열고 전체 실행(F5) · View ▸ Variables 창에서 값 확인
-- 같은 글이 Oracle · SQL Server · PostgreSQL에서도 같은 뜻이다(같은 폴더의 oracle.sql · mssql.sql · pg.sql).
-- 변수는 두 가지다 — 섞이지 않는다(이름이 같아도 별개):
--   바인드 변수 `:V`   = 타입을 가진 값 · 서버에 **진짜 바인드**로 간다 (VAR · EXEC · PRINT)
--   치환 변수   `&v`   = 글자 매크로 · 서버로 가기 전에 글에 끼워진다     (DEFINE · ACCEPT · COLUMN NEW_VALUE · ${v:형식})
-- ═══════════════════════════════════════════════════════════════════════════════════════════════

CREATE TABLE emp (id INTEGER PRIMARY KEY, name TEXT, dept TEXT, sal INTEGER, hired TEXT);
INSERT INTO emp VALUES
  (1, '홍길동', 'SALES', 300, '2024-03-02'),
  (2, '김철수', 'DEV',   450, '2025-01-15'),
  (3, '이영희', 'DEV',   520, '2026-07-01');

-- ── 1. 선언 · 리터럴 대입 (DB 왕복 0 — 클라이언트에서 끝난다) ─────────────────────────────────
VAR V_LIMIT NUMBER = 500
VAR V_NOTE  VARCHAR2(40) = 'it''s a note'
EXEC :V_DEPT := 'DEV'
-- 선언 없이 대입하면 그 자리에서 생긴다(타입은 값에서). NULL도 값이다(변수를 지우지 않는다).
EXEC :V_EMPTY := NULL

-- ── 2. 서버 식 대입 — SQLite는 `SELECT (식)`으로 보내 1행을 잡는다 ─────────────────────────────
EXEC :V_MAX := (SELECT MAX(sal) FROM emp WHERE dept = :V_DEPT)
EXEC :V_TODAY := date('now')

-- ── 3. 한 행 → 여러 변수 (자리 순서대로 · 0행/여러 행 = 오류 · 설정 vars.into_policy = first면 첫 행) ──
EXEC
SELECT name, sal
INTO   :V_TOP_NAME, :V_TOP_SAL
FROM   emp
WHERE  sal = :V_MAX

-- ── 4. 살펴보기 ───────────────────────────────────────────────────────────────────────────
PRINT V_DEPT V_MAX V_TOP_NAME V_TOP_SAL
-- VARIABLE(인자 없음) = 선언 목록 · SHOW VARIABLES = 이름/타입/값 표(GUI = 결과 탭)
VARIABLE
SHOW VARIABLES

-- ── 5. 뒤 문장에서 바인드로 쓴다 — 이름은 대소문자를 가리지 않는다(D-142) ─────────────────────────
--   ⚠ SQLite 고유의 `@x` · `$x` 표기는 아직 변수로 묶이지 않는다(`:x`만 쓴다).
SELECT name, sal FROM emp WHERE dept = :V_DEPT AND sal < :V_LIMIT ORDER BY sal;
SELECT :v_dept AS dept, :V_Max AS max_sal, :v_top_name AS top_name;

-- 테이블 값 함수도 보통 조회 + 바인드다.
EXEC :V_JSON := '[10, 20, 30]'
SELECT value FROM json_each(:V_JSON) WHERE value >= 20;

-- ── 6. 치환 변수 ─────────────────────────────────────────────────────────────────────────
DEFINE bonus = 10
DEFINE tab = emp
-- `&이름` = 그대로 끼운다(식별자·조각도 된다) · 뒤의 `.`은 이름을 끝내는 글자(`&tab..id` → `emp.id`)
SELECT name, sal + &bonus AS with_bonus FROM &tab WHERE &tab..id = 1;
-- 스크립트 인자: `nsql run … sqlite.sql KOREA` → &1 = KOREA
SELECT '&1' AS arg1;
-- `${이름:형식}` — 안전 인용형. q = SQL 글자 상수(따옴표를 두 번) · id = 인용한 이름 · n = 수일 때만 · upper/lower
--   (값에 작은따옴표가 들면 큰따옴표로 감싼다 — 맨 작은따옴표는 문장 분리기가 글자 상수의 시작으로 읽는다)
DEFINE who = "O'Neil"
SELECT ${who:q} AS quoted, ${bonus:n} AS num, '${tab:upper}' AS up;
SELECT COUNT(*) AS c FROM ${tab:id};
-- 시스템 변수(읽기 전용 · 값은 실행 시점): _USER _CONNECT_IDENTIFIER _DIALECT _DATE _TIMESTAMP _ROW_COUNT _SQLCODE _ELAPSED_MS _FILE
SELECT '&_DIALECT' AS dialect, '&_DATE' AS today, '&_ROW_COUNT' AS last_rows;
-- OS 환경 변수(설정 vars.env_subst · 없는 변수는 글자 그대로 남는다)
SELECT ${env:PATH:q} IS NOT NULL AS has_path;

-- 결과의 열 값을 치환 변수로: 그 열의 **마지막 행** 값이 들어간다.
COLUMN max_id NEW_VALUE last_id
SELECT MAX(id) AS max_id FROM emp;
SELECT name FROM emp WHERE id = &last_id;
COLUMN max_id CLEAR

-- 값을 물어서 받기(GUI = 입력 창 · CLI = 터미널 · `--no-prompt`면 기본값). HIDE = 가려서 입력.
--   ACCEPT min_sal NUMBER DEFAULT 400 PROMPT 'Minimum salary: '
--   SELECT name FROM emp WHERE sal >= &min_sal;

-- 치환을 끄고 싶을 때(글자 `&`가 든 자료를 넣는 스크립트) — 다시 켜기 = SET DEFINE ON
SET DEFINE OFF
SELECT 'R&D' AS literal_amp;
SET DEFINE ON

-- ── 7. 범위 · 비밀 ───────────────────────────────────────────────────────────────────────
-- 변수 표의 주인은 **탭**이다(다른 탭의 실행이 내 값을 바꾸지 않는다 · D-135). 같은 연결의 탭들과 나눠 쓰려면 올린다.
VAR V_DEPT SHARE
VAR V_DEPT LOCAL
-- 이름에 PASS · PWD · SECRET · TOKEN이 들면 비밀 — 창·로그·SHOW VARIABLES에서 ******, 파일에 저장하지 않는다(D-140).
EXEC :V_API_TOKEN := 'abc123'
SHOW VARIABLES
-- 값은 파일별로 보존된다(설정 vars.persist · 비밀·커서 제외) — 다음에 이 파일을 열면 이어서 쓸 수 있다.
