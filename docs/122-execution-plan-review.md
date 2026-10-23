# 122 · 실행 계획 검토 — 설계 · 방언별 원문/파싱 · 검토 규칙 · 사용법 (137차 · 2026-10-23)

> **요청**(사용자 10-23): 실행 계획을 보고 문제를 짚어 주는 기능 + *"실행 계획에 대한 사용법은 별도 문서로 관리"*.
> **이 문서의 자리**: 설계와 사용법을 한곳에 둔다. CLI 사용 안내 [40 Step 10](40-cli-usage.md)에는 링크만 둔다.
> **구현**: 커밋 `a1fdfc5`(plan 모듈) · `17ebac4`(`Runner::explain` · CLI `--analyze`/`--raw`) · `a567086`(GUI 결과 탭 2 + 메뉴) · `dbf57ad`(방언별 통합 시험) — 브랜치 feat/137-plan-review.

---

## 1. 무엇

- 한 문장(캐럿이 있는 문장 또는 선택한 글)을 **방언별 EXPLAIN 관용**으로 돌리고, 결과를 **공통 트리**로 풀어 **자동 검토**(문제 짚기)를 붙인다.
- 모드 두 가지:
  - **추정 계획**(기본) — 문장을 실행하지 않는다.
  - **실제 실행 계획** — 문장을 **실제로 실행**해 실측 행 수·시간을 보여 준다. DML이면 데이터가 바뀌므로 GUI 상태줄과 CLI stderr에 경고가 나온다.
- 진입점:
  - GUI = 실행 ▸ **실행 계획**(Ctrl+Alt+X) · 실행 ▸ **실제 실행 계획**(메뉴·팔레트 · 단축키 없음).
  - CLI = `nsql explain -c <target> (-q <sql> | <file>) [--analyze] [--raw]`.
- 결과:
  - GUI = 결과 탭 2개 — **"실행 계획"**(트리 표) · **"검토"**(등급 · 노드 번호 · 검토 내용). 트리를 못 풀면 원문 한 탭.
  - CLI = 들여쓴 트리 + 검토 목록(텍스트). `--raw` = DBMS 원문 그대로(종전 동작 · 파싱·검토 없음).
- 원칙: 계획 원문을 **고치지 않는다**(힌트 자동 적용 없음) · 검토는 **판단 재료**이지 자동 수정이 아니다.

## 2. 어디(코드)

| 층 | 자리 | 내용 |
|---|---|---|
| nsql-script | `src/plan.rs`(의존 0 · 순수) | `PlanOptions{analyze}` · `explain_script_opts(dialect, stmt, opts)` · `parse_plan(dialect, columns, rows) -> Vec<PlanNode>` · `review(dialect, nodes, analyzed) -> Vec<Finding>` · `build_report` · `render_text` · 메시지 = `tr!` 두 언어 |
| nsql-run | `Runner::explain(stmt, opts) -> Result<PlanReport, DbError>` | 스크립트 실행 → 결과 집합 선택(SQL Server = `StmtText` 열이 있는 집합 · 그 밖 = 마지막) → 문자열 행 → `build_report` · 첫 오류 = Err |
| nexa-sql | `worker::Cmd::Explain{stmt, analyze}` · `plan_result_sets()` · `app/run.rs` `run_explain`/`run_explain_analyze` · 메뉴 `run.explain_analyze` | 결과 집합 둘로 돌려 **기존 결과 탭 경로 재사용**(그리드 · 복사 · 내보내기 그대로) |
| nsql-cli | `cmd_explain` + `--analyze`/`--raw` · `help.rs` | 텍스트 출력 |
| i18n | `MnExplainAnalyze` · `PlanTab*` · `PlanCol*` · `StExplainActualWarn` · `HlpOptAnalyze`/`Raw` | |

## 3. 방언별 원문과 파싱

| 방언 | 추정 | 실제 | 파싱 |
|---|---|---|---|
| PostgreSQL | `EXPLAIN (VERBOSE, COSTS)` | `EXPLAIN (ANALYZE, BUFFERS, VERBOSE, COSTS)` | `->` 들여쓰기 트리 · `(cost=a..b rows=N)` · `(actual time=.. rows=N loops=L)`(rows × loops) · Filter / Sort Method / Buffers 줄 = 상세 |
| Oracle | `EXPLAIN PLAN FOR …;` + `SELECT * FROM TABLE(DBMS_XPLAN.DISPLAY(NULL,NULL,'TYPICAL'))` | `SELECT /*+ GATHER_PLAN_STATISTICS */ …;` 실행 뒤 `DISPLAY_CURSOR(NULL,NULL,'ALLSTATS LAST')`(V$SQL_PLAN 읽기 권한 필요 · 없으면 오류 → 추정으로) | 표(`\| Id \| Operation \| Name \| Rows/E-Rows \| A-Rows \| Cost (%CPU) \| A-Time \|`) · Operation 들여쓰기 = 깊이 · Predicate Information의 `n - filter(...)/access(...)` = 상세 |
| SQL Server | `SET SHOWPLAN_ALL ON` + `DECLARE @nsql_plan BIT;`(배치 강제) | `SET STATISTICS PROFILE ON`(실행 + Rows/Executes) | 결과 집합 열 `StmtText`(`\|--` 깊이) · `PhysicalOp` · `Argument`의 `OBJECT:(…)` · `EstimateRows` · `TotalSubtreeCost` · `Warnings`(NO JOIN PREDICATE) |
| MySQL / MariaDB | `EXPLAIN` 표 | `EXPLAIN ANALYZE`(MySQL 8.0.18+ 트리 · MariaDB는 문법이 달라 오류 → 추정만 권장) | 표: `type`(ALL/index/range/ref…) · `table` · `possible_keys`/`key` · `rows`·`filtered` · `Extra`(Using filesort/temporary) · 트리 텍스트는 PG와 같은 파서 |
| SQLite | `EXPLAIN QUERY PLAN` | 같음(실측 없음) | `id · parent · detail` → 부모 사슬 깊이 · `SCAN t` / `SEARCH t USING …` |
| DuckDB | `EXPLAIN` | `EXPLAIN ANALYZE` | 상자 그림: 대문자 연산 줄(`SEQ_SCAN`) · 다음 줄 대상 · `~N Rows` · `EC:` 비용 · 상자 열 위치 = 깊이 |
| ODBC | `EXPLAIN` | 같음 | 뒤 DBMS를 모르니 행마다 노드(첫 열 = 연산 · 나머지 = 상세) |

> ★ **Oracle 실측에서 고친 것**(10-23 통합 시험): Thin 드라이버(`oracledb`)가 `EXPLAIN PLAN`을 파싱만 하고 실행하지 않았다 → `EXPLAIN PLAN`을 PL/SQL `EXECUTE IMMEDIATE`로 감싸 보낸다(Thin · OCI 두 드라이버 공통).

## 4. 검토 규칙(`review`)

| # | 규칙 | 등급 |
|---|---|---|
| 1 | **전체 스캔**(TABLE ACCESS FULL · Seq Scan · Table/Index Scan · type=ALL · SCAN · SEQ_SCAN) | Warn · 행 ≥ 10,000이면 High |
| 2 | **카티전 / 조인 조건 없음**(MERGE JOIN CARTESIAN · NO JOIN PREDICATE · Block Nested Loop · Nested Loop 100만 행↑) | High |
| 3 | **큰 집합 정렬·임시**(Sort Method: external · Disk: · Using filesort/temporary · SORT 10,000행↑) | Warn |
| 4 | **비용 집중**(노드 ≥ 3 · 한 노드가 최대 비용의 50 %↑ · 루트 제외) | Info |
| 5 | **추정 vs 실측 10배↑**(실제 계획 · 실측 ≥ 1,000) → 통계 갱신 권장 | Warn |
| 6 | **인덱스 후보를 안 씀**(possible_keys 있는데 key=NULL) · PG `Rows Removed by Filter` 대량 | Warn · Info |
| 7 | **필터의 컬럼 함수**(전체 스캔의 필터에 TO_CHAR/UPPER/LOWER/TRUNC/SUBSTR/NVL/CAST) → 함수 기반 인덱스·재작성 | Info |
| — | 트리를 못 풀면 "원문을 보세요" · 짚을 것이 없으면 "없음" 한 줄 | Info |

등급 표기 = CLI `[HIGH]` · `[WARN]` · `[INFO]` · GUI "검토" 탭 등급 열.

## 5. 사용법

### 5-1. GUI

1. 편집기에서 문장에 캐럿을 둔다(또는 문장을 선택한다).
2. 실행 ▸ **실행 계획**(Ctrl+Alt+X).
3. 결과 영역에 탭 둘이 생긴다.
   - **실행 계획** — 열 = 연산 · 대상 · 추정 행 · 실측 행 · 비용 · 시간 · 상세(Filter·Predicate 등).
   - **검토** — 열 = 등급 · 노드 번호 · 검토 내용.
4. 두 탭은 **노드 번호**로 맞춰 본다(검토의 `#3` = 계획 탭의 3번 줄).
5. 실측이 필요하면 실행 ▸ **실제 실행 계획** — 문장을 실제로 실행한다. 상태줄에 경고가 뜨고, **운영 보류 정책(DR-34 · 115)이 그대로 적용**된다. DML은 데이터가 바뀌므로 수동 커밋 모드에서 보고 ROLLBACK하는 것을 권한다.
6. 결과 탭은 일반 결과와 같아서 복사·내보내기·그리드 기능을 그대로 쓴다.

### 5-2. CLI

```sh
nsql explain -c prod -q "SELECT e.name, d.dname FROM emp e JOIN dept d ON d.id = e.dept ORDER BY e.sal"
nsql explain -c prod query.sql            # 파일의 첫 문장
nsql explain -c prod -q "…" --analyze     # 실제 실행 계획(문장을 실행 · stderr 경고)
nsql explain -c prod -q "…" --raw         # DBMS 원문 그대로(파싱·검토 없음)
```

실제 출력(SQLite · nsql 0.2.8 · main 28c31b3 · `ui.lang=ko` · 접속 줄의 경로는 줄임 · `--raw`·`--analyze`는 dbf57ad 출력과 같은 꼴):

```
$ nsql explain -c sqlite:emp.db -q "SELECT e.name, d.dname FROM emp e JOIN dept d ON d.id = e.dept WHERE upper(e.name) LIKE 'K%' ORDER BY e.sal"
접속: sqlite://…\emp.db (sqlite)
실행 계획
  1  SCAN [e]
     SCAN e
  2  SEARCH [d]
     SEARCH d USING INTEGER PRIMARY KEY (rowid=?)
  3  USE TEMP B-TREE FOR ORDER BY
     USE TEMP B-TREE FOR ORDER BY
검토
  [WARN] #1: e 전체 스캔 - 필터/조인 컬럼 인덱스를 검토하세요(작은 테이블이면 무시해도 됩니다)
```

```
$ nsql explain -c sqlite:emp.db -q "…같은 문장…" --raw
id  parent  notused  detail
--  ------  -------  --------------------------------------------
 4       0        0  SCAN e
10       0        0  SEARCH d USING INTEGER PRIMARY KEY (rowid=?)
20       0        0  USE TEMP B-TREE FOR ORDER BY
```

```
$ nsql explain -c sqlite:emp.db -q "SELECT * FROM emp WHERE dept = 3" --analyze
실행 계획 (실제)
  1  SEARCH [emp]
     SEARCH emp USING INDEX ix_emp_dept (dept=?)
검토
  [INFO] 자동 검토에서 짚을 것이 없습니다
실제 실행 계획: 문장을 실제로 실행합니다(DML이면 데이터가 바뀝니다)
```

영어(`ui.lang=en`) = "Execution plan" · "Review" · "No issues found by the automatic review". SQLite는 실측이 없어 `--analyze`도 같은 계획이다. 방언별 관용과 검토 항목 요약은 `nsql explain --help`에도 있다(28c31b3 · 7방언).

### 5-3. 해석 요령

- **노드 번호로 두 탭(또는 트리와 검토 목록)을 맞춰 본다.**
- 전체 스캔(규칙 1)은 **작은 표면 무시**해도 된다 — 행 수 열을 같이 본다(10,000행↑이면 High).
- 추정과 실측이 10배 넘게 다르면(규칙 5) 계획보다 **통계가 문제**다 → 통계 수집(Oracle `DBMS_STATS` · PG `ANALYZE` · SQL Server `UPDATE STATISTICS`).
- 필터에 컬럼 함수(규칙 7)가 있으면 인덱스를 못 탄다 → 함수 기반 인덱스나 조건 재작성(`UPPER(name) = 'K'` → 정규화 컬럼).
- 큰 정렬·임시(규칙 3)는 메모리 부족 신호 → ORDER BY/GROUP BY 컬럼 인덱스 또는 작업 메모리 설정.
- Oracle 실제 계획은 V$SQL_PLAN 읽기 권한이 필요하다 — 없으면 오류가 나고 추정 계획을 쓴다.

### 5-4. 한계

- 계획 원문을 고치지 않는다(힌트 자동 적용 없음).
- MariaDB 실제 계획 미지원(`EXPLAIN ANALYZE` 문법 차이).
- ODBC는 뒤 DBMS를 모르니 원문 행을 노드로만 보여 준다.
- 트리 파싱은 방언의 대표 출력 꼴 기준이다 — 특수한 출력은 원문 탭(`--raw`)으로 본다.

## 6. 시험

- **단위**: `nsql-script` `plan::tests` 7 — PG 트리·실측 · Oracle 표·ALLSTATS·Predicate · MSSQL SHOWPLAN_ALL · MySQL 표 · SQLite · DuckDB 상자 · 렌더.
- **통합**(`crates/nsql-drivers/tests/integration.rs` `explain_review_*` · 저장 프로필 `NSQL_*_PROFILE`로 자동):

| 방언 | 추정 | 실제 | 10-23 실측(개발 세션 · Windows) |
|---|---|---|---|
| SQLite | ✓ | (실측 없음) | ✓(항상) |
| PostgreSQL | ✓ | ✓ | ✓ 추정 + 실제(REPOSITORY_DEV) |
| SQL Server | ✓ | ✓ | ✓ 추정 + 실제(BISCM_MS_DEV) |
| Oracle | ✓ | ✓(DISPLAY_CURSOR · 권한 되면) | ✓ 추정 + 실제(BISCM_DEV · Thin) — Thin `EXPLAIN PLAN` 파싱만 문제 → `EXECUTE IMMEDIATE` 감싸기 |
| MySQL / MariaDB | ✓ | (MariaDB 미지원) | 로컬 = 생략(미도달) · CI integration = MariaDB EXPLAIN 표 |

- CI = `integration.yml`(MariaDB EXPLAIN 표 · sqlite · 확장 경로 dyn).
- GUI 실기(결과 탭 2 · 실제 실행 계획 경고) = 사용자(TODO T-409).

## 7. 관련 문서

- [40 CLI 사용법 Step 10](40-cli-usage.md) — `nsql explain` 기본 사용.
- [115 운영 모드 시나리오](115-prod-mode-scenario-nsql-guide.md) — 실제 실행 계획에도 보류 정책이 적용된다.
- [83 SQL Preview·객체 생성](83-object-explorer-dbms-trees-and-generate-sql.md) · [43 결과 탭](43-fetch-model-and-result-tabs.md) — 결과 탭 재사용.
