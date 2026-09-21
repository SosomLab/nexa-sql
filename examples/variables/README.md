# 변수 사용법 샘플 (DBMS별)

Nexa SQL의 변수 기능을 DBMS마다 **같은 절 순서**로 보여 주는 실행 가능한 스크립트다. 설계·배경 = [docs/63](../../docs/63-variable-management.md).

| 파일 | 실행 | 서버에 남는 것 |
|---|---|---|
| [oracle.sql](oracle.sql) | `nsql run -c <프로필> examples/variables/oracle.sql SCOTT` | §8만 프로시저 `NSQLT_VARS_DEMO`를 만들고 끝에서 지운다(나머지 = 없음) |
| [mssql.sql](mssql.sql) | `nsql run -c <프로필> examples/variables/mssql.sql 5` | 없음(세션 임시 객체 `#…`) |
| [pg.sql](pg.sql) | `nsql run -c <프로필> examples/variables/pg.sql 5` | 없음(`pg_temp.…`) |
| [sqlite.sql](sqlite.sql) | `nsql run -c sqlite::memory: examples/variables/sqlite.sql KOREA` | 없음(메모리 · 서버 불필요) |

- 접속 없이 문장 분리·대입·방언별 재작성만 보려면 `nsql plan -d <oracle|mssql|postgres|sqlite> <파일> [인자]`.
- GUI에서는 해당 DBMS 접속 탭에서 열어 전체 실행(F5) → 값은 **View ▸ Variables** 창, 커서·다중 결과는 이름 붙은 결과 탭.
- MySQL/MariaDB · NoSQL은 아직 드라이버가 없어 샘플이 없다.

## 변수는 두 가지다 (섞이지 않는다 — 이름이 같아도 별개)

| | 바인드 변수 `:V` | 치환 변수 `&v` |
|---|---|---|
| 성격 | 타입을 가진 **값** — 서버에 진짜 바인드로 간다 | **글자 매크로** — 서버로 가기 전에 글에 끼워진다 |
| 만들기 | `VAR` · `EXEC :V := …` · `EXEC SELECT … INTO :A, :B` · 프로시저 OUT | `DEFINE` · `:setvar` · `ACCEPT` · `COLUMN … NEW_VALUE` · 인자 `&1` · CLI `-v 이름=값` |
| 보기 | `PRINT` · `VARIABLE` · `SHOW VARIABLES` · 변수 창 | `DEFINE`(인자 없음) |
| 쓸 수 있는 자리 | 값이 오는 자리만 | 어디나(식별자·조각 포함) — 값에는 안전 인용형 `${v:q}` 권장 |

## 절 구성 (네 파일 공통)

| 절 | 내용 | 방언 차이 |
|---|---|---|
| 선언·리터럴 대입 | `VAR n NUMBER = 3` · `EXEC :V := '글'` · `NULL` | 없음 — 전부 클라이언트에서 끝난다(DB 왕복 0) |
| 서버 식 대입 | `EXEC :V := 식` | Oracle `BEGIN :V := 식; END;` · SQL Server `SET @V = 식` · PG/SQLite `SELECT (식)` 1행 잡기 |
| 한 행 → 여러 변수 | `EXEC SELECT a, b INTO :A, :B FROM …` | Oracle OUT 바인드 · SQL Server `SELECT @A = a` + `@@ROWCOUNT` 검사 · PG/SQLite INTO를 떼고 자리 순서로 받음 · **0행/여러 행 = 오류**(설정 `vars.into_policy`) |
| 살펴보기 | `PRINT` · `VARIABLE` · `SHOW VARIABLES` | SQL Server의 `PRINT 'text'`/`PRINT @v`는 서버 문장 그대로 |
| 뒤 문장에서 바인드 | `WHERE col = :V` · 테이블 함수 | Oracle 이름 바인드 · SQL Server `sp_executesql` 인자(GO를 넘어 이어짐) · PG `$n` · SQLite `:x`만 |
| 프로시저·커서·다중 결과 | OUT/INOUT · REF CURSOR · 결과 집합 여러 개 | Oracle = `VAR rc REFCURSOR` · 선언 없는 호출(서명 추론) · 암묵 결과 / SQL Server = `OUTPUT` 보충 · 결과 집합마다 탭(커서 바인드 없음) / PG = 서명이 말한 자리의 바인드 · `refcursor`는 내용으로 풀림 / SQLite = 해당 없음 |
| 치환 변수 | `&v` `&tab..col` `&1` · `${v:q|id|n|upper|lower}` · 시스템 변수(`&_USER` `&_DATE` `&_ROW_COUNT` …) · `${env:이름}` · `COLUMN … NEW_VALUE` · `ACCEPT` · `SET DEFINE OFF` | `q` = SQL Server만 `N'…'` · `id` = SQL Server `[…]` / 그 밖 `"…"` |
| 범위·비밀 | `VAR x SHARE|LOCAL`(탭 층 ↔ 연결 공유 층) · 이름에 `PASS` `PWD` `SECRET` `TOKEN` = 가림·저장 안 함 | 없음 |

알려진 흠(SQLite `@x`·`$x` 미지원 · SQL Server의 FROM 없는 `SELECT … INTO` 등) = [docs/TODO.md](../../docs/TODO.md) T-162.
