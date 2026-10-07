# 변수 사용법 샘플 (공통 언어 + DBMS별)

Nexa SQL의 변수 기능을 **실행 가능한 스크립트**로 보여 준다. 설계·개념 = [docs/63](../../docs/63-variable-management.md)(§12 개념 총정리 · §11 층 · §9 확장 시점 · §10 내장 변수 · §3-1 방언별 기법).

| 파일 | 무엇 | 실행 | 서버에 남는 것 |
|---|---|---|---|
| [common.sql](common.sql) | ★ **우리 앱의 변수 언어 전부**(바인드·치환·시스템·내장·환경 변수 · 층 · DROP/CLEAR · 자동 타입 · 확장 시점 · 비밀) — 문장마다 `[§절] 기능 — 효과 · 방언 대체` 주석 | `nsql run -c sqlite::memory: examples/variables/common.sql KOREA` | 없음(메모리) |
| [oracle.sql](oracle.sql) | 같은 흐름을 Oracle 식으로(`FROM DUAL` · REF CURSOR · 서명 추론) + 부록 SQL*Plus/Golden 대조 | `nsql run -c <프로필> examples/variables/oracle.sql SCOTT` | §8만 프로시저 `NSQLT_VARS_DEMO`(끝에서 삭제) · §9 글로벌은 끝에서 DROP |
| [mssql.sql](mssql.sql) | T-SQL 재작성(`sp_executesql` · 꼬리 행 · `@@ROWCOUNT`) + 부록 T-SQL `@v`/sqlcmd 대조 | `nsql run -c <프로필> examples/variables/mssql.sql 5` | 없음(세션 임시 `#…`) |
| [pg.sql](pg.sql) | `$n` 바인드 · refcursor · `DO` 인용형 + 부록 psql 변수 대조 | `nsql run -c <프로필> examples/variables/pg.sql 5` | 없음(`pg_temp.…`) |
| [sqlite.sql](sqlite.sql) | 서버 없이 · `json_each` + sqlite3 `.parameter` 대조 | `nsql run -c sqlite::memory: examples/variables/sqlite.sql KOREA` | 없음 |

- 접속 없이 문장 분리·로컬 대입·방언별 재작성만 보려면 `nsql plan -d <oracle|mssql|postgres|sqlite> <파일> [인자]`.
- GUI = 해당 DBMS 접속 탭에서 열어 전체 실행(F5) → 값은 **View ▸ Variables** 창(가려진 줄 흐림) · `SHOW VARIABLES` = 결과 탭(모든 층 · `Active` 열) · 커서·다중 결과 = 이름 붙은 결과 탭.
- **확장 시점 절**(common §7 · 각 파일 끝)은 설정 `vars.expand_at`을 `assign`/`use`로 바꿔 **두 번** 실행해 비교한다 — GUI 환경 설정 ▸ 스크립트·변수 · CLI `nsql config set vars.expand_at use`.
- 글로벌 변수(`GLOBAL`)는 **앱 전역·디스크 보존**이라 예제가 만든 것은 끝에서 `DROP GLOBAL`로 지운다. 시험용 격리 홈을 쓰려면 `NSQL_HOME=<임시 폴더>`.
- MySQL/MariaDB · NoSQL은 아직 드라이버가 없어 샘플이 없다(개념은 common.sql 주석의 "방언 대체"에).

## 변수의 다섯 층위 (섞이지 않는다 — 이름이 같아도 별개)

| | 바인드 변수 `:V` | 치환 변수 `&v` | 시스템 변수 `&_USER` | 내장 변수 `${file}` | 환경 변수 `${env:PATH}` |
|---|---|---|---|---|---|
| 성격 | 타입을 가진 **값** — 진짜 바인드 | **글자 매크로** — 보내기 전에 끼움 | 읽기 전용 · 실행 시점 값 9종 | 앱이 아는 값(VS Code 이름) | 내장 별칭 `NSQL_*` → OS |
| 만들기 | `VAR` · `EXEC :V := …` · `EXEC SELECT … INTO` · OUT/커서 | `DEFINE` · `:setvar` · `ACCEPT` · `COLUMN NEW_VALUE` · `&1` · `-v` | 없음 | 없음(문맥) | 없음(OS) |
| 지우기 | `VAR x DROP [GLOBAL]` · `VAR CLEAR [GLOBAL\|ALL]` | `UNDEFINE` | — | — | — |
| 보기 | `PRINT` · `VARIABLE` · `SHOW VARIABLES` · 변수 창 | `DEFINE`(인자 없음) · 변수 창 | — | — | — |
| 층 | tab > shared > global > profile | 탭 | — | — | — |

## 절 구성 (다섯 파일 공통 · 번호는 파일마다 조금 다르다)

| 절 | 내용 | 방언 차이 |
|---|---|---|
| 선언·리터럴 대입 | `VAR n NUMBER = 3` · 타입 생략 `VAR t = 'x'` · 타입 없는 `VAR x`(탭 선언) · `EXEC :V := '글'` · `NULL` · 엄격 타입 | 없음 — 클라이언트에서 끝난다(DB 왕복 0) |
| 서버 식 대입 | `EXEC :V := 식` | Oracle `BEGIN :V := 식; END;` · SQL Server `SET @V = 식` + 꼬리 행(sql_variant 타입 복원) · PG/SQLite `SELECT (식)` 1행 잡기 |
| 한 행 → 여러 변수 | `EXEC SELECT a, b INTO :A, :B FROM …` | Oracle OUT 바인드 · SQL Server `SELECT @A = a` + `@@ROWCOUNT` · PG/SQLite INTO를 떼고 자리 순서 · **0행/여러 행 = 오류**(`vars.into_policy`) · FROM 없는 INTO = T-162 흠 |
| 살펴보기 | `PRINT` · `VARIABLE` · `SHOW VARIABLES`(모든 층 + `Active`) | SQL Server `PRINT 'text'`/`PRINT @v`는 서버 문장 |
| 뒤 문장에서 바인드 | `WHERE col = :V` · 테이블 함수 | Oracle 이름 바인드 · SQL Server `sp_executesql`(GO 넘어 이어짐) · PG `$n` · SQLite `:x @x $x` |
| 프로시저·커서·다중 결과 | OUT/INOUT · REF CURSOR · 결과 집합 여러 개 | Oracle `VAR rc REFCURSOR` · 서명 추론 · 암묵 결과 / SQL Server OUTPUT 보충 · 결과 집합마다 탭 / PG refcursor 내용으로 / SQLite 해당 없음 |
| 치환 변수 | `&v` `&tab..col` `&1` · `:setvar` · `UNDEFINE` · `${v:q\|id\|n\|upper\|lower}` · 시스템 변수 · `${env:}` · 내장 `${file}` · `COLUMN NEW_VALUE` · `ACCEPT` · `SET DEFINE OFF` | `q` = SQL Server `N'…'` · MySQL 역슬래시 · `id` = SQL Server `[…]` / MySQL `` ` `` / 그 밖 `"…"` |
| 층 · 범위 | `VAR x SHARE\|LOCAL\|GLOBAL` · 선언+층 `VAR x NUMBER = 1 GLOBAL` · **대입 = 사는 층**(D-264) · 가림 = 명시 선언 · `DROP [GLOBAL]` · `CLEAR [GLOBAL\|ALL]` | 없음(클라이언트) |
| 자동 타입 | 선언 없는 변수 = 값마다 타입 재추론 · 선언 타입 고정 | 없음 |
| 확장 시점 | `vars.expand_at` assign/use — 치환 변수 재귀 · 바인드 수식 dirty 재계산 | 수식은 **지금 세션 방언**으로 돈다 |
| 비밀·보존 | `PASS` `PWD` `SECRET` `TOKEN` 가림 · 파일별 보존 · 글로벌 보존 | 없음 |

알려진 흠(FROM 없는 `SELECT … INTO` · 리터럴 대입 타입 검사 없음 등) = [docs/TODO.md](../../docs/TODO.md) T-162 · T-304.
