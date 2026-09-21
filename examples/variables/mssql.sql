-- ═══════════════════════════════════════════════════════════════════════════════════════════════
-- 변수 사용법 — SQL Server (docs/63 · Oracle과 **같은 글**이 T-SQL로 재작성되어 돈다)
--   실행:  nsql run -c <프로필> examples/variables/mssql.sql 5
--   보기:  nsql plan -d mssql examples/variables/mssql.sql 5           (접속 없이 — 재작성된 T-SQL을 볼 수 있다)
--   GUI :  SQL Server 접속 탭에서 열고 전체 실행(F5) · 값 = View ▸ Variables 창
-- 서버에 남는 것은 없다 — 만드는 것은 세션 임시 객체(`#…`)뿐이다.
-- 변수는 두 가지다 — 섞이지 않는다:
--   바인드 변수 `:V`   = 클라이언트가 가진 값 · `sp_executesql`의 인자로 간다 (VAR · EXEC · PRINT)
--   치환 변수   `&v`   = 글자 매크로 (DEFINE · :setvar · ACCEPT · COLUMN NEW_VALUE · ${v:형식})
-- ⚠ `@이름`은 **T-SQL 자신의 변수**다 — 배치(GO)가 끝나면 사라진다. 배치를 넘겨 값을 들고 다니려면 `:이름`을 쓴다.
-- ═══════════════════════════════════════════════════════════════════════════════════════════════

CONNECT mssql://BISCM_MS@192.168.0.58:1433/M4PLAN_MS

-- ── 1. 선언 · 리터럴 대입 (DB 왕복 0) ──────────────────────────────────────────────────────
VAR V_LIMIT INT = 3
VAR V_NOTE  NVARCHAR(40) = 'it''s a note'
EXEC :V_PRG_NM := 'SP_DEMO'
EXEC :V_KOR    := '홍길동'

-- ── 2. 서버 식 대입 — `DECLARE @V … ; SET @V = 식; SELECT @V`(꼬리 행으로 값을 읽어 온다) ────────────
EXEC :V_DB  := DB_NAME()
EXEC :V_CNT := (SELECT COUNT(*) FROM sys.objects)

SELECT
	:V_LIMIT
,	:V_NOTE
,	:V_PRG_NM
,	:V_KOR
,	:V_DB
,	:V_CNT
;

-- ── 3. 한 행 → 여러 변수 — `SELECT @A = a, @B = b` + @@ROWCOUNT 검사로 재작성 ─────────────────────
--   T-SQL은 여러 행이면 말없이 마지막 행을 쓴다 → 여기서는 0행 = 오류 51403 · 여러 행 = 51422 (Oracle과 같은 정책)
EXEC
SELECT
	COUNT(*)
,	MAX(name)
INTO
	:V_OBJ_CNT
,	:V_MAX_NAME
FROM
	sys.objects
WHERE 1=1
AND	object_id		<	100

EXEC SELECT TOP 1 name INTO :V_FIRST FROM sys.objects ORDER BY name

-- ── 4. 살펴보기 — PRINT 뒤가 **이름 목록**이면 변수 출력 · `PRINT 'text'` / `PRINT @v`는 서버 문장 그대로 ──────
PRINT V_PRG_NM V_KOR V_DB V_CNT V_OBJ_CNT V_MAX_NAME V_FIRST
PRINT 'this goes to the server'
VARIABLE
SHOW VARIABLES

-- ── 5. 뒤 문장에서 바인드로 쓴다 — GO로 배치가 나뉘어도 값이 이어진다 ───────────────────────────
SELECT :V_PRG_NM AS prg, :V_KOR AS kor, :V_OBJ_CNT AS cnt, :v_max_name AS mx;
GO
IF OBJECT_ID('tempdb..#nsql_vars') IS NOT NULL DROP TABLE #nsql_vars
GO
CREATE TABLE #nsql_vars (id INT, name NVARCHAR(30))
GO
INSERT INTO #nsql_vars VALUES (1, N'홍길동'), (2, N'김철수'), (3, N'이영희');
EXEC :V_ID := 2
SELECT id, name FROM #nsql_vars WHERE id >= :V_ID;

-- 테이블 값 함수 + 바인드
EXEC :V_LIST := 'a,b,c'
SELECT value FROM STRING_SPLIT(:V_LIST, ',');

-- ── 6. 프로시저 — OUTPUT · 다중 결과 집합 ───────────────────────────────────────────────────
CREATE PROCEDURE #nsql_vars_io
  @a INT, @b INT OUTPUT, @msg NVARCHAR(100) OUTPUT, @d DATETIME2 = NULL OUTPUT
AS BEGIN
  SET @b = @a * @b;
  SET @msg = @msg + N' -> ' + CAST(@b AS NVARCHAR(20));
  SET @d = DATEADD(DAY, 1, @d);
  SELECT @a AS a, @b AS b;                      -- 결과 집합 1
  SELECT name FROM #nsql_vars ORDER BY id;      -- 결과 집합 2 → 결과 집합마다 탭
END
GO
EXEC :X := 21
EXEC :Y := 'in'
EXEC :D := '2026-09-21 13:45:10'
-- 호출 쪽에 OUTPUT을 **빠뜨려도** 서명(sys.parameters)이 OUTPUT이라 한 자리의 바인드는 값을 받는다(설정 vars.signature_lookup).
EXEC #nsql_vars_io 3, :X, :Y, :D
PRINT X Y D
-- 이름 표기 · OUTPUT을 직접 써도 된다(두 번 붙이지 않는다).
EXEC #nsql_vars_io @msg = :Y OUTPUT, @a = 2, @b = :X OUTPUT
PRINT X Y
-- ⚠ 커서 바인드는 없다 — `VAR c REFCURSOR`는 안내만 나온다. 여러 결과는 위처럼 결과 집합으로 돌려준다.

-- ── 7. 치환 변수 ─────────────────────────────────────────────────────────────────────────
-- sqlcmd `:setvar`와 `DEFINE`은 **같은 저장소**다. 참조는 `&이름`.
:setvar Top 3
DEFINE tab = sys.objects
SELECT TOP &Top name FROM &tab ORDER BY name
GO
-- 스크립트 인자: `nsql run … mssql.sql 5` → &1 = 5
SELECT TOP &1 name FROM sys.objects ORDER BY name DESC;
-- `${이름:형식}` — q = N'…' 유니코드 상수(따옴표 두 번) · id = [인용한 이름] · n = 수일 때만 · upper/lower
DEFINE who = "O'Neil"
DEFINE col = name
SELECT ${who:q} AS quoted, ${Top:n} AS num, ${col:id} FROM sys.objects WHERE object_id = 3;
-- 시스템 변수: _USER _CONNECT_IDENTIFIER _DIALECT _DATE _TIMESTAMP _ROW_COUNT _SQLCODE _ELAPSED_MS _FILE
SELECT '&_USER' AS usr, '&_DIALECT' AS dialect, '&_DATE' AS today, '&_ROW_COUNT' AS last_rows;
-- OS 환경 변수(sqlcmd `$(v)`의 환경 변수 폴백과 같은 쓰임 · 설정 vars.env_subst)
SELECT ${env:COMPUTERNAME:q} AS host;

-- 결과의 열 값 → 치환 변수(그 열의 마지막 행 값)
COLUMN max_id NEW_VALUE last_id
SELECT MAX(id) AS max_id FROM #nsql_vars;
SELECT name FROM #nsql_vars WHERE id = &last_id;
COLUMN max_id CLEAR

-- 값을 물어서 받기(GUI = 입력 창 · CLI = 터미널 · `--no-prompt`면 기본값). HIDE = 가려서 입력.
--   ACCEPT min_id NUMBER DEFAULT 2 PROMPT 'Minimum id: '
--   SELECT name FROM #nsql_vars WHERE id >= &min_id;

-- `&`가 든 자료를 넣을 때는 끈다.
SET DEFINE OFF
SELECT 'R&D' AS literal_amp;
SET DEFINE ON

-- ── 8. 범위 · 비밀 ───────────────────────────────────────────────────────────────────────
VAR V_PRG_NM SHARE
VAR V_PRG_NM LOCAL
-- 이름에 PASS · PWD · SECRET · TOKEN이 들면 비밀 — 창·로그·SHOW VARIABLES에서 ******, 파일에 저장하지 않는다(D-140).
EXEC :V_API_TOKEN := 'abc123'
SHOW VARIABLES

DROP PROCEDURE #nsql_vars_io
DROP TABLE #nsql_vars
GO
