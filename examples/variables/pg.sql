-- ═══════════════════════════════════════════════════════════════════════════════════════════════
-- 변수 사용법 — PostgreSQL (docs/63 · Oracle과 **같은 글**이 `$n` 바인드로 재작성되어 돈다)
--   실행:  nsql run -c <프로필> examples/variables/pg.sql 5
--   보기:  nsql plan -d postgres examples/variables/pg.sql 5           (접속 없이 — 재작성된 SQL을 볼 수 있다)
--   GUI :  PostgreSQL 접속 탭에서 열고 전체 실행(F5) · 값 = View ▸ Variables 창 · 커서 = 이름 붙은 결과 탭
-- 서버에 남는 것은 없다 — 만드는 것은 세션 임시 객체(`pg_temp.…`)뿐이다.
-- 변수는 두 가지다 — 섞이지 않는다:
--   바인드 변수 `:V`   = 타입을 가진 값 · `$1 $2 …` 진짜 바인드로 간다 (VAR · EXEC · PRINT)
--   치환 변수   `&v`   = 글자 매크로 (DEFINE · ACCEPT · COLUMN NEW_VALUE · ${v:형식})
-- `::타입` 캐스트와 `:=`는 바인드로 읽지 않는다(psql 어휘 규칙). 값은 **클라이언트**에 살아서 ROLLBACK과 무관하다.
-- ═══════════════════════════════════════════════════════════════════════════════════════════════
SET AUTOCOMMIT ON

-- ── 1. 선언 · 리터럴 대입 (DB 왕복 0) ──────────────────────────────────────────────────────
VAR V_LIMIT INTEGER = 3
VAR V_NOTE  VARCHAR(40) = 'it''s a note'
EXEC :V_PRG_NM := 'sp_demo'
EXEC :V_KOR    := '홍길동'

-- ── 2. 서버 식 대입 — `SELECT (식) AS "V"`로 보내 1행을 잡는다 ─────────────────────────────────
EXEC :V_DB  := current_database()
EXEC :V_CNT := (SELECT count(*) FROM generate_series(1, 5))

-- ── 3. 한 행 → 여러 변수 — INTO를 떼고 보낸 뒤 **자리 순서로** 클라이언트가 받는다 ───────────────────
--   0행·여러 행 = 오류(Oracle과 같은 정책 · 설정 vars.into_policy = first면 첫 행)
EXEC
SELECT
	count(*)
,	max(g)::text || '!'
INTO
	:V_ROWS
,	:V_MAX_TXT
FROM
	generate_series(1, :V_LIMIT) g

-- ── 4. 살펴보기 ───────────────────────────────────────────────────────────────────────────
PRINT V_PRG_NM V_KOR V_DB V_CNT V_ROWS V_MAX_TXT
VARIABLE
SHOW VARIABLES

-- ── 5. 뒤 문장에서 바인드로 쓴다 — 선언한 타입이 있으면 그 타입으로 캐스트되어 간다 ─────────────────
SELECT :V_PRG_NM AS prg, :V_KOR AS kor, :V_ROWS AS n, :v_max_txt AS mx;
SELECT g, g::numeric / 2 AS half FROM generate_series(1, :V_LIMIT) g WHERE g >= :V_LIMIT - 1;

-- 집합 반환 함수(SRF) + 바인드
EXEC :V_LIST := 'a,b,c'
SELECT * FROM unnest(string_to_array(:V_LIST, ',')) AS t(item);

-- ── 6. 프로시저 — OUT/INOUT 은 서명이 말한 자리의 바인드가 받는다 ─────────────────────────────────
CREATE PROCEDURE pg_temp.nsql_vars_io(IN a int, INOUT b int, INOUT msg text) LANGUAGE plpgsql AS $$
BEGIN
  b := a * b;
  msg := msg || ' -> ' || b;
  RAISE NOTICE 'b is now %', b;          -- 서버 메시지로 보인다
END $$;
EXEC :X := 21
EXEC :Y := 'in'
-- 바인드 이름이 형식 인자 이름과 달라도 된다(pg_proc 서명 · 설정 vars.signature_lookup).
EXEC pg_temp.nsql_vars_io(3, :X, :Y)
PRINT X Y
-- 이름 표기
EXEC pg_temp.nsql_vars_io(msg => :Y, a => 2, b => :X)
PRINT X Y
-- 그냥 CALL 하면 OUT 값은 1행 결과로 온다.
CALL pg_temp.nsql_vars_io(2, 5, 'call');

-- ── 7. refcursor — 커서 **이름**이 아니라 **내용**이 결과로 온다(라벨 = 커서 이름 · 받자마자 닫는다) ─────
--   자동 커밋이면 드라이버가 호출과 FETCH를 한 트랜잭션으로 묶는다 · 수동 커밋이면 열린 트랜잭션 안에서.
CREATE FUNCTION pg_temp.nsql_vars_rc(n int) RETURNS refcursor LANGUAGE plpgsql AS $$
DECLARE c refcursor := 'items';
BEGIN
  OPEN c FOR SELECT g AS n, 'row ' || g AS label FROM generate_series(1, n) g;
  RETURN c;
END $$;
CREATE FUNCTION pg_temp.nsql_vars_rc2() RETURNS SETOF refcursor LANGUAGE plpgsql AS $$
DECLARE a refcursor := 'cur_a'; b refcursor := 'cur_b';
BEGIN
  OPEN a FOR SELECT 1 AS one;
  RETURN NEXT a;
  OPEN b FOR SELECT g AS n FROM generate_series(10, 12) g;
  RETURN NEXT b;
END $$;
SELECT pg_temp.nsql_vars_rc(:V_LIMIT);
-- 커서 여러 개 = 커서마다 결과 탭(`cur_a` · `cur_b`)
SELECT pg_temp.nsql_vars_rc2();

-- ── 8. 치환 변수 ─────────────────────────────────────────────────────────────────────────
DEFINE lim = 3
DEFINE tab = pg_catalog.pg_class
SELECT relname FROM &tab ORDER BY relname LIMIT &lim;
-- 스크립트 인자: `nsql run … pg.sql 5` → &1 = 5
SELECT g FROM generate_series(1, &1) g;
-- `${이름:형식}` — q = 글자 상수(psql `:'v'`) · id = "인용한 이름"(psql `:"v"`) · n = 수일 때만 · upper/lower
DEFINE who = "O'Neil"
DEFINE col = relname
SELECT ${who:q} AS quoted, ${lim:n} AS num, ${col:id} FROM pg_catalog.pg_class LIMIT 1;
-- 바인드를 못 받는 문장(유틸리티 · DO)에는 인용형 치환을 쓴다.
DO $$ BEGIN RAISE NOTICE 'hello %', ${who:q}; END $$;
-- 시스템 변수: _USER _CONNECT_IDENTIFIER _DIALECT _DATE _TIMESTAMP _ROW_COUNT _SQLCODE _ELAPSED_MS _FILE
SELECT '&_USER' AS usr, '&_DIALECT' AS dialect, '&_DATE' AS today, '&_ROW_COUNT' AS last_rows;
-- OS 환경 변수(설정 vars.env_subst · 없는 변수는 글자 그대로)
SELECT ${env:PATH:q} IS NOT NULL AS has_path;

-- 결과의 열 값 → 치환 변수(그 열의 마지막 행 값)
COLUMN today NEW_VALUE run_date
SELECT to_char(now(), 'YYYYMMDD') AS today;
PROMPT run date = &run_date
COLUMN today CLEAR

-- 값을 물어서 받기(GUI = 입력 창 · CLI = 터미널 · `--no-prompt`면 기본값). HIDE = 가려서 입력.
--   ACCEPT min_n NUMBER DEFAULT 2 PROMPT 'Minimum n: '
--   SELECT g FROM generate_series(1, 5) g WHERE g >= &min_n;

-- `&`가 든 자료를 넣을 때는 끈다.
SET DEFINE OFF
SELECT 'R&D' AS literal_amp;
SET DEFINE ON

-- ── 9. 범위 · 비밀 ───────────────────────────────────────────────────────────────────────
VAR V_PRG_NM SHARE
VAR V_PRG_NM LOCAL
-- 이름에 PASS · PWD · SECRET · TOKEN이 들면 비밀 — 창·로그·SHOW VARIABLES에서 ******, 파일에 저장하지 않는다(D-140).
EXEC :V_API_TOKEN := 'abc123'
SHOW VARIABLES
