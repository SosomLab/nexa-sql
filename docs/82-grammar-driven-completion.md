# 82. 문법 참조 기반 완성 — 절(clause)마다 올 수 있는 예약어·객체 · DBMS별 참조 파일 = 플러그인(100차 mac · 2026-09-24)

> 사용자(09-24): "SQLite에서 `SELECT avg` 자리에 `HAVING`이 나온다 — 각 DBMS 문법 구조(재귀 배치 포함)를 분석해 절별로 배치 가능한 예약어·객체를 정리해 구현 · DBMS별 설계 구조를 **참조 파일**로 두고 파일을 추가하면 그 DB를 지원하는 플러그인 구조".

## 0-1. SQL Server 테이블 반환 함수(10-02 ㉜)

FROM/JOIN 자리의 테이블 함수 판정(`table_function`)은 `extra`로 한다 — SQL Server는 코드(`IF`/`TF`/`FT` · L1 이름 인덱스 `NameEntry.extra`)와 `type_desc`(목록) 둘 다. L1만 있어도 판정되게 `load_names_x`가 부가를 함께 올린다.

## 1. 결론
- 예약어 후보는 **지금 절의 `next` 목록**에서만 낸다. 절은 캐럿의 괄호 깊이에서 뒤로 걸어 처음 만나는 절 이름(최대 세 낱말) · 서브쿼리 `( SELECT …` 안이면 그 안의 절 · 함수 괄호 안이면 "모름"(전체 표 폴백).
- 절·예약어·객체 종류는 코드가 아니라 **`.sqlg` 참조 파일**에 있다. 내장 5개(`ansi` + `oracle`·`mssql`·`postgres`·`sqlite`) · `NSQL_HOME/grammar/*.sqlg`를 두면 같은 이름은 내장을 **대신**하고 새 이름은 **새 방언**이 된다(재시작 때 읽음).

## 2. 파일 형식(`crates/nsql-script/grammar/*.sqlg`)
```
dialect = sqlite            # 방언 이름(소문자 · nsql-core Dialect 이름과 같게)
extends = ansi              # 부모(생략 = ansi)
[start]                     # 문장 시작에 올 수 있는 것
next += PRAGMA, ATTACH DATABASE
[statement SELECT]          # 감지용 절 목록(부모에 더함)
clauses += LIMIT, OFFSET, WINDOW
[clause SELECT]             # 절 규칙
next += GLOB, IIF           # `=` = 부모 것을 바꿈 · `+=` = 더함(순서 = 파일 순서 = 자주 쓰는 것부터)
objects = column, function, subquery   # 놓을 수 있는 객체 종류(`subquery` = 재귀)
[clause LEFT JOIN]
same = JOIN                 # 별칭
```
- 절 이름은 낱말 그대로(`GROUP BY` · `LEFT OUTER JOIN`) · 대소문자 무관 · `#` 주석.
- ★ **10-30 5차(T-200 절 안 위치 · D-415)** = 키 `first = …`/`first += …`/`first -= …` — **절 키워드 바로 뒤(첫 항목 자리)에만** 나오는 예약어(ansi `[clause SELECT] first = DISTINCT, ALL` · mssql `first += TOP`). 감지 = nsql-script `grammar_clause`가 `Context.clause_pos` = `ClausePos{First, AfterComma, Other}` · 후보 = `Grammar::next_at(clause, pos)` — First = `first` ∪ `next` − 절 전환 예약어(`SELECT FROM` 없음) · AfterComma = `next` − `first`(전환 예약어는 남김 = `SELECT a, fr` → FROM) · Other = `next` − `first`.
- ★ **상속 빼기 `-=`(10-30 5차 · 82 §10-5 1순위)** = `next -=` · `first -=` · `[start] next -=`(대소문자 무시) — 부모(`ansi`)에서 받은 예약어를 그 방언에서 뺀다(예: access `[clause FROM] next -= LIMIT, OFFSET`) · `clauses -=`는 형식 오류. **방언 파일에 넣는 빼기 목록 = §10 ⚠ 항목을 공식 문서로 확인한 것만**(협업 조사 → 개발 반영).
- `objects`는 지금은 기록·진단용(관계 자리 판정은 종전 `RELATION_AFTER`) — 다음 단계에서 이 값으로 컬럼/테이블/함수 후보를 켜고 끈다. ★ **10-30 1차(180차 · D-398 대기)** = GUI `intel.rs::objects_demote` — **식 자리(Expr)** 에서 절 `objects`에 없는 종류(Column·Alias→column · Table·Synonym·Sequence→table · View→view · Schema·Database→schema · Function·Routine·Package→function)는 **빼지 않고 점수 층 +1**(문법 파일 빈틈 보호) · 관계·종류(Want)·EXEC 인자 자리는 그대로.

## 3. 구조
| 층 | 위치 | 역할 |
|---|---|---|
| 참조 파일 | `crates/nsql-script/grammar/*.sqlg`(내장 · `include_str!`) · `NSQL_HOME/grammar/*.sqlg`(플러그인) | 절 → next/objects/same · 문장 시작 · 감지용 절 목록 |
| 해석·병합 | nsql-script `grammar.rs` — `parse` → `Raw` → `resolve(부모)` → `Grammar` · 레지스트리(`for_dialect`/`for_name` · `register` · `load_dir` · 캐시) | 부모 사슬(`extends` ≤ 4) · 덧입힘은 내장보다 먼저 |
| 감지 | nsql-script `intel::context_at` → `Context.clause`/`stmt_kind`(`grammar_clause` = 괄호 깊이 · 세 낱말까지) | 순수 함수 · 시험 |
| 소비 | nexa-sql `Intel::request`(Expr/Start) — 시작 = `[start]` · 절을 알면 `next` · 모르면 공통+방언 키워드 전체 | 객체 후보(컬럼·테이블·함수)는 종전 규칙 그대로 |
| 로더 | nexa-sql `main.rs` 기동 때 `grammar::load_dir(config_dir()/grammar)` · 실패는 stderr `[grammar] 파일: 이유` | 플러그인 = 파일 하나 |

재귀: `( SELECT` 안은 새 문장이라 감지가 안쪽 절에서 멈춘다 · 닫힌 `( … )`는 건너뛴다 · `IN (` · `EXISTS (` 뒤의 SELECT도 같다. 함수 인자 안(`NVL(a, |`)은 절이 없으므로 전체 표(식 예약어 포함).

## 4. 검증(시험)
- `grammar::tests` — 내장 5개 해석 · sqlite `SELECT` next에 `HAVING` 없음·`GROUP BY` next에 있음 · 별칭 · objects · 상속 · 덧입힘(`tibero extends oracle`) · 형식 오류.
- `intel::tests::clause_detection_follows_paren_depth` — 마지막 절 · 세 낱말 · 서브쿼리 안/뒤 · 함수 괄호 안 · INSERT.
- nexa-sql `keywords_follow_grammar_clause` — `SELECT a, ha` = HAVING 없음 · `GROUP BY a ha` = 있음 · 시작 `pra` = PRAGMA · 함수 괄호 안 = 전체 표.

## 5. 새 DBMS를 더하는 법
1. `NSQL_HOME/grammar/<이름>.sqlg`에 `dialect = <이름>` · `extends = <가까운 방언>` · 다른 절만 적는다.
2. 앱 재시작 → 그 방언(드라이버가 `Dialect`로 알려 주는 이름 또는 `for_name`)으로 붙은 탭이 그 문법을 쓴다. 이름이 내장과 같으면 내장을 대신한다.
3. 내장으로 올리려면 `crates/nsql-script/grammar/`에 파일을 두고 `grammar.rs`의 `BUILTIN`에 한 줄.

## 6. 남은 것(T-200 후속)
- ✅ `objects` 게이트 1차(10-30 · 층 내림 · 식 자리) · 절 안 위치(예: `SELECT` 뒤 첫 항목 = `DISTINCT`/`TOP`만) · `CASE … END` 같은 식 내부 상태 · MySQL 파일 · 방언 문서(공식 문법 다이어그램)와의 대조표.

## 7. 10-01 보완 — `CtxKind::Want`(종류 자리) · `ON`/`USING` 문장별 · MySQL 문법([journal §16](journal/2026-09-30.md))
- 문법 `objects =`는 여전히 읽지 않는다(§6). 대신 nsql-script `want_at`가 **종류 자리**를 판정한다: `USE |` · `ALTER SESSION SET CURRENT_SCHEMA = |` · `SET search_path TO |` · `EXEC|CALL |` · `DROP|ALTER <종류> |`. 호스트 `want_cands`가 그 종류만 낸다(키워드 없음).
- `ON`·`USING`은 `RELATION_AFTER`에서 빼고 문장별로(`relation_after_special`) — `JOIN … ON |`은 식 자리(별칭 컬럼). `mysql.sqlg` 신설 · `MYSQL_KEYWORDS`.
- 다음(T-274): `objects =`를 `Want`로 소비(문법에서 `[clause USE] objects = database`처럼 선언) — 지금은 코드 표(`WANT_KINDS`).

- **10-08 `Want::ProcParam`**(T-316 · c2cdb04): `EXEC [sys.]sp_x |` · `, |` · `@…` 입력 중 = 그 프로시저의 **아직 안 쓴 `@파라미터`**(타입 상세) · 값 자리(`= |`)는 제외 · 원천 = 내장 표 `MSSQL_SYSTEM_PROC_PARAMS` 30종(확장 속성 3 · sp_rename · sp_help* · sp_columns/tables · sp_executesql · 연결된 서버 · xp_cmdshell …) · 시험 nsql-script 13 · 호스트 19.

- **10-08 `CtxKind::ExecArgs`**(T-317 · 3af60cb): 사용자 프로시저 `EXEC [s.]proc |`·`, |` = 메타 루틴 인자(시스템 표는 `Want::ProcParam` 그대로) · **4bb512d** = Oracle·PG 괄호 호출 인자 시작에서 `이름 => ` 조각(이미 적은 이름 제외).

## 8. 10-01 보완 2 — SQL Server 3부 이름(`DB.스키마.객체` · [journal §21](journal/2026-09-30.md))
- 다른 DB의 객체 = 메타 복합 열쇠 `DB.스키마` 버킷(스키마 자리에 두 조각 글자) · `DB.` = `(DB, Schema)` 버킷. 별칭 표는 점 사슬 전부(`schema = "DB.스키마"`). 호스트는 열쇠를 `split_db_key`로 풀어 메타 세션을 `USE`로 옮긴 뒤 읽는다. bare 이름은 현재 DB의 현재 스키마(dbo) — `USE` 뒤 재읽기로 갱신.
- 컬럼 없는 종류(Database·Schema·사용자·역할·연결된 서버)는 별칭 해석(테이블로 보는 단계)에서 제외.
- 진단: `NSQL_TRACE_META=1` → 메타 요청 전부(결과 상태) + `[intel] ctx` · 자체 시험 `editor.caret` · `intel.probe` · `intel.dump` → `win-intel-3part-e2e.sh`.


## 9. 10-08 보완 — `ALTER TABLE` 절(T-315 · bb81c51)
- ansi `[clause ALTER TABLE]` 신설 · DROP objects에 function · Oracle ALTER 종류 + `ALTER TABLE` MODIFY/ADD`(` · mssql ALTER COLUMN · pg ADD/ALTER COLUMN · 시험 nsql-script 119 · 같은 커밋의 링크 쪽 = [96 §10](96-object-links.md).

## 10. 공식 문법 대조표(10-30 · T-200 · D-415 · 협업 초안)

> ⚠ **초안** — 하위 조사가 공식 문서를 **기억으로** 대조(웹 확인 안 함). 판정 ⚠ 항목과 §10-6 불확실 목록은 `.sqlg`를 고치기 전에 공식 문서로 한 번 더 확인한다. 절 안 위치 `first` 키는 작업 트리에 이미 구현됨(nsql-script `grammar.rs`) — §10-4는 그 위에 `item` · `after_item` 추가 제안. ★ 5차 구현 메모: `-=`는 `next`·`first`·`[start] next`만 — **`clauses -=`는 형식 오류**이므로 아래 표의 `clauses -= …` 제안은 해당 절의 `next -=`(그 절로 넘어가는 예약어 빼기)로 바꿔 적용한다.

> 대상: `crates/nsql-script/grammar/*.sqlg` 9개(작업 트리 기준 · 10-30 T-200 `first` 키 반영본) · 작성 = 협업 하위 에이전트 초안 · 저장소 파일은 고치지 않음.

### 10-1. 요약 결론

- **골격은 쓸 만하다** — 문장 시작어·주요 절·방언 특유 절(Oracle 계층 질의 · T-SQL APPLY/OUTPUT · PG ON CONFLICT · CH PREWHERE/LIMIT BY · Access TRANSFORM/PIVOT)은 대부분 잡혀 있다.
- **가장 큰 구조적 결함 = 상속에 "빼기"가 없다**(`=` 전체 교체 · `+=` 더하기만). 그래서 ansi의 `LIMIT`·`FETCH FIRST`·`RETURNING`·`DELETE … USING`·`MERGE`·`WITH RECURSIVE`·`UNION DISTINCT`·`NULLS FIRST`·`DEFAULT VALUES`가 **그 문법이 없는 방언으로 그대로 새어 든다**(✖ 다수의 근원). `-=` 연산자 하나로 대부분 정리된다.
- **ansi 자체의 오배치**: `ROWS ONLY`가 FROM·WHERE·ORDER BY 다음에 단독으로 나온다(실제는 `FETCH FIRST n` 뒤에만). `ORDER BY` 다음에 `OFFSET`이 없다(표준 `ORDER BY … OFFSET n ROWS`). `GROUP BY`에 `ROLLUP/CUBE/GROUPING SETS`(SQL:1999)가 없어 mssql·clickhouse가 못 받는다.
- 방언별 최대 구멍: **Oracle** = 테이블 별칭 `AS` 불가인데 제안됨 · `ORDER BY SIBLINGS` 오배치 · MERGE `AND`/`THEN DELETE` 형태 불일치 · `INSERT ALL/FIRST` 누락 / **MSSQL** = `FOR XML/JSON`·`OPTION` 절·`OUTPUT` 절·`GROUP BY ROLLUP` 누락, `OFFSET`을 ORDER BY 없이 제안, `RETURNING`·`NULLS FIRST`·`NATURAL JOIN`·`CREATE OR REPLACE` 상속 / **PG** = `DISTINCT ON`·`FOR NO KEY UPDATE/KEY SHARE`·`CALL`·`FOR UPDATE` 꼬리 / **MySQL** = `WHERE … FOR UPDATE`·`FOR SHARE`·`WITH ROLLUP`·`INTO OUTFILE`·인덱스 힌트 누락, `FULL JOIN`·`FETCH`·`NULLS FIRST` 상속 / **SQLite** = `FETCH`·`MERGE`·`GRANT`·`TRUNCATE` 상속 / **Db2** = `FETCH NEXT`·`SKIP LOCKED DATA`·`FOR FETCH ONLY` / **ClickHouse** = SELECT 목록에 FROM 뒤 절(FINAL·PREWHERE·JOIN 종류) 오배치, `ALTER TABLE … UPDATE/DELETE` 누락 / **Access** = LIMIT·OFFSET·FETCH·WITH·INTERSECT·FULL/CROSS/NATURAL JOIN 상속(파일 주석은 "없음"이라 적었으나 상속으로 살아 있음).
- 함수 이름이 `next`에 섞여 있다(db2·clickhouse·access SELECT next 수십 개) — 예약어 후보 목록을 흐린다. 함수는 카탈로그/별도 키로 분리 권장(범위 밖 · 메모만).

### 10-2. 방법

- 비교 대상: ① `[start] next`(문장 시작어) ② `[statement SELECT/INSERT/UPDATE/DELETE/MERGE] clauses` ③ 각 `[clause X] next`/`first` — `extends` 상속을 풀어 **방언별 실효 목록**(ansi + 덧붙임)으로 판정했다(`grammar.rs::resolve` = `=` 교체 · `+=` 더함 · `same` 별칭).
- 공식 기준: Oracle SQL Language Reference(19c/23ai) · T-SQL reference(2019/2022) · PostgreSQL 16/17 SQL Commands · SQLite 문법 도표(3.4x) · MySQL 8.4 Reference · Db2 LUW 11.5 SQL Reference · ClickHouse SELECT 문서(24.x) · Access(ACE) SQL 참조 — 기억 기반, 웹 확인 안 함(불확실은 ⚠).
- 범위 밖: 함수·데이터 형식·DDL 세부 옵션(저장 매개변수 등)·PL/SQL·T-SQL 제어문 내부·SQL*Plus 명령.
- 판정: ✅ 일치 · ➕ 누락 · ✖ 잘못(그 방언에 없음/그 자리에 오지 않음) · ⚠ 확인 필요(버전·모드 의존).

### 10-3. 방언별 대조

#### 10-3-1. ansi(모든 방언의 부모)

| 항목 | .sqlg 현재 | 공식 문법(SQL:2016) | 판정 | 제안 |
|---|---|---|---|---|
| `[clause FROM]`·`WHERE`·`ORDER BY` next `ROWS ONLY` | 단독 제안 | `FETCH {FIRST\|NEXT} n {ROW\|ROWS} ONLY` 뒤에만 | ✖ | 세 절에서 `ROWS ONLY` 삭제(FETCH FIRST 절에만) |
| `[clause ORDER BY]` next | ASC…, FETCH FIRST, ROWS ONLY | `ORDER BY … [OFFSET n ROWS] [FETCH …]` | ➕ | `next`에 `OFFSET, FETCH NEXT` 추가 |
| `[statement SELECT]` clauses | FETCH FIRST, OFFSET, LIMIT | FETCH FIRST/NEXT 둘 다 · LIMIT은 표준 아님 | ➕/⚠ | `FETCH NEXT` 추가 · `LIMIT`은 ansi에서 빼고 pg·mysql·sqlite·ch·db2로 이동 |
| `[clause GROUP BY]` next | HAVING, ORDER BY… | `ROLLUP( )`·`CUBE( )`·`GROUPING SETS( )`(SQL:1999) | ➕ | `next += ROLLUP, CUBE, GROUPING SETS`(ansi로 올리고 방언 중복 제거) |
| `[clause FROM]` next 조인 | LEFT OUTER JOIN만 OUTER 형 | RIGHT/FULL OUTER JOIN · LATERAL · WINDOW | ➕ | `RIGHT OUTER JOIN, FULL OUTER JOIN, LATERAL, WINDOW` 추가 |
| `[clause JOIN]` next | 조인 종류 일부 | 위와 동일 + NATURAL | ➕ | `LEFT OUTER JOIN, RIGHT OUTER JOIN, FULL OUTER JOIN, NATURAL JOIN` 추가 |
| `[clause OFFSET]` next | ROWS, FETCH FIRST, FETCH NEXT | `ROW \| ROWS` | ➕ | `ROW` 추가 |
| `[clause FETCH FIRST]` | ROWS ONLY…PERCENT, WITH TIES | 표준 ✅(방언별로 PERCENT/WITH TIES 차이) | ✅ | 방언에서 `-=`로 정리 |
| SELECT/ON/WHERE/HAVING/SET next `WHEN, THEN, ELSE, END` | 절 예약어로 상시 제안 | CASE 식 안에서만 | ✖ | 절 next에서 빼고 §4-3 CASE 상태로 |
| `[start]` | DESCRIBE, EXPLAIN, BEGIN, DECLARE, MERGE INTO | 표준 아님(EXPLAIN·DESCRIBE) · 표준에 CALL·SET·START TRANSACTION·VALUES·TABLE | ⚠/➕ | `CALL, START TRANSACTION, RELEASE SAVEPOINT` 추가 · EXPLAIN/DESCRIBE는 방언으로 |
| `[clause DELETE FROM]` next `USING` | 있음 | 표준 DELETE에 USING 없음(PG·MySQL 확장) | ✖ | ansi에서 빼고 postgres·mysql에 |
| `RETURNING`(INSERT/UPDATE/DELETE) | ansi에 있음 | 표준 아님(PG·SQLite·Oracle PL/SQL·MariaDB) | ✖ | ansi에서 빼고 해당 방언에 |
| `[clause WHEN MATCHED]` | THEN UPDATE SET, THEN DELETE, AND | ✅(SQL:2008 + AND 조건) | ✅ | — |
| 그 밖(WITH·UNION·INSERT INTO·UPDATE·SET·MERGE INTO·USING) | | | ✅ | — |

#### 10-3-2. Oracle

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| FROM·JOIN next `AS`(상속) | 테이블 별칭에 AS 제안 | 테이블 별칭에 `AS` 불가(열 별칭만) | ✖ | `[clause FROM] next -= AS` · `[clause JOIN] next -= AS` |
| `[clause ORDER BY]` next `SIBLINGS` | ORDER BY 뒤 | `ORDER SIBLINGS BY` = ORDER와 BY 사이 | ✖ | 삭제 · `[clause ORDER SIBLINGS BY] same = ORDER BY` 추가(CONNECT BY next에는 이미 있음 ✅) |
| `[clause FROM]` next `WHERE CURRENT OF` | FROM 뒤 | `WHERE CURRENT OF 커서`(UPDATE/DELETE의 WHERE 첫 자리) | ✖ | FROM에서 삭제 · `[clause WHERE] first += CURRENT OF` |
| `[clause WHERE]` next `SKIP LOCKED`, `FOR UPDATE NOWAIT` | WHERE 뒤 | `FOR UPDATE [OF …] [NOWAIT \| WAIT n \| SKIP LOCKED]` | ✖(배치) | `[clause FOR UPDATE] next = OF, NOWAIT, WAIT, SKIP LOCKED` 신설 · statement SELECT `clauses += FOR UPDATE` · WHERE에서 `SKIP LOCKED`·`FOR UPDATE NOWAIT` 삭제 |
| FROM next `FOR UPDATE` | 없음 | `SELECT … FROM t FOR UPDATE` 가능 | ➕ | `[clause FROM] next += FOR UPDATE, OFFSET, FETCH NEXT` |
| FROM 표 참조 | SAMPLE, PARTITION, LATERAL… | `AS OF {TIMESTAMP\|SCN}` · `VERSIONS BETWEEN` · `MATCH_RECOGNIZE` · `CROSS APPLY`/`OUTER APPLY`(12c+) | ➕ | `next += AS OF TIMESTAMP, AS OF SCN, VERSIONS BETWEEN, MATCH_RECOGNIZE, CROSS APPLY, OUTER APPLY` |
| `[clause SET]` next `RETURNING INTO` | 연속어로 제안 | `RETURNING 식 INTO 변수`(사이에 식) | ✖ | `RETURNING INTO` 삭제(`[clause RETURNING] next`에 INTO 있음 ✅) |
| `[start]` | MERGE INTO 중복 … | `INSERT ALL` · `INSERT FIRST`(다중 테이블) · `LOCK TABLE` · `CALL` · `FLASHBACK TABLE` · `PURGE` · `SET TRANSACTION` | ➕ | `next += INSERT ALL, INSERT FIRST, LOCK TABLE, CALL, FLASHBACK TABLE, SET TRANSACTION` |
| `[clause WHEN MATCHED]`(상속) `AND`, `THEN DELETE` | ansi형 | `WHEN MATCHED THEN UPDATE SET … [WHERE …] [DELETE WHERE …]` — AND 조건·독립 THEN DELETE 없음 | ✖ ⚠(23ai 재확인) | `[clause WHEN MATCHED] next = THEN UPDATE SET` · `[clause WHEN NOT MATCHED] next = THEN INSERT` · SET next `+= DELETE WHERE` |
| `[clause WITH]` `RECURSIVE`(상속) | 제안 | Oracle은 RECURSIVE 낱말 없음(재귀 CTE는 열 목록 + `SEARCH`/`CYCLE`) | ✖ | `next = AS, SELECT, SEARCH DEPTH FIRST BY, SEARCH BREADTH FIRST BY, CYCLE` |
| LIMIT(상속) | 절로 인식 | Oracle에 LIMIT 없음 | ✖ | statement SELECT `clauses -= LIMIT` |
| `INSERT INTO` next `DEFAULT VALUES`(상속) | 제안 | 없음 | ✖ | `-= DEFAULT VALUES` |
| `DELETE FROM` next `USING`(상속) | 제안 | 없음 | ✖ | `-= USING` |
| UNION next `DISTINCT` · MINUS `ALL` | 상속 | UNION DISTINCT 없음 · MINUS ALL/INTERSECT ALL/EXCEPT = 21c+ | ✖/⚠ | `[clause UNION] next = SELECT, ALL` |
| `[clause SELECT] first` | DISTINCT, ALL | DISTINCT \| UNIQUE \| ALL | ➕ | `first += UNIQUE` |
| 그 밖(CONNECT BY·START WITH·PIVOT·MODEL·ROLLUP·NULLS FIRST·RETURNING BULK COLLECT) | | | ✅ | — |

#### 10-3-3. SQL Server(T-SQL)

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| SELECT 꼬리 절 | OFFSET, FETCH NEXT, OPTION | `FOR XML {RAW\|AUTO\|PATH\|EXPLICIT}` · `FOR JSON {AUTO\|PATH}` · `FOR BROWSE` · `WINDOW`(2022) | ➕ | statement SELECT `clauses += FOR XML, FOR JSON, WINDOW` · `[clause FOR XML] next = RAW, AUTO, PATH, EXPLICIT, ROOT, TYPE, ELEMENTS` · `[clause FOR JSON] next = AUTO, PATH, ROOT, INCLUDE_NULL_VALUES, WITHOUT_ARRAY_WRAPPER` · FROM/WHERE/GROUP BY/HAVING/ORDER BY next `+= FOR XML, FOR JSON` |
| `[clause OPTION]` | 절 이름만(정의 없음) | `OPTION (RECOMPILE \| MAXDOP n \| OPTIMIZE FOR … \| HASH JOIN \| LOOP JOIN \| FORCE ORDER …)` | ➕ | `[clause OPTION] next = RECOMPILE, MAXDOP, OPTIMIZE FOR, OPTIMIZE FOR UNKNOWN, HASH JOIN, MERGE JOIN, LOOP JOIN, FORCE ORDER, FAST` |
| `SELECT … INTO 새표` | INTO = SELECT next 낱말 | INTO는 FROM 앞 절 | ➕ | statement SELECT `clauses += INTO` · `[clause INTO] next = FROM` · `objects = table, schema` |
| `[clause GROUP BY]` | ansi 그대로 | `ROLLUP( )`·`CUBE( )`·`GROUPING SETS( )` · `WITH ROLLUP`·`WITH CUBE` | ➕ | `next += ROLLUP, CUBE, GROUPING SETS, WITH ROLLUP, WITH CUBE, OPTION` |
| FROM·WHERE next `OFFSET`, `FETCH NEXT`(mssql 추가) · `FETCH FIRST`(상속) | ORDER BY 없이 제안 | `OFFSET`은 `ORDER BY` 뒤에만 · FETCH는 OFFSET 뒤에만 | ✖ | FROM `-= OFFSET, FETCH NEXT, FETCH FIRST, ROWS ONLY` · WHERE `-= OFFSET, FETCH FIRST, ROWS ONLY` |
| ORDER BY next `FETCH NEXT`, `ROWS ONLY`, `NULLS FIRST/LAST`(상속) | 제안 | FETCH는 OFFSET 뒤 · NULLS FIRST/LAST 없음 | ✖ | `-= FETCH NEXT, FETCH FIRST, ROWS ONLY, NULLS FIRST, NULLS LAST` |
| `[clause FETCH FIRST]` `PERCENT ROWS ONLY`, `WITH TIES` | 상속 | OFFSET-FETCH에 PERCENT·WITH TIES 없음(TOP 전용) | ✖ | `[clause FETCH FIRST] next = ROWS ONLY, ROW ONLY` |
| INSERT/UPDATE/DELETE `RETURNING`(상속) | 제안 | 없음 · `OUTPUT inserted.*/deleted.* [INTO @t]` | ✖/➕ | 세 statement `clauses -= RETURNING` `+= OUTPUT` · `[clause OUTPUT] next = INSERTED, DELETED, INTO, VALUES, SELECT, FROM, WHERE` |
| `[clause DELETE FROM]` `TOP` | FROM 뒤 | `DELETE TOP (n) FROM` = FROM 앞 · `DELETE FROM t FROM t JOIN …`(두 번째 FROM) | ✖/➕ | `-= TOP, USING` · `+= FROM` · `[start] next += DELETE TOP` |
| JOIN `USING`·`NATURAL JOIN`(상속) | 제안 | 둘 다 없음 | ✖ | FROM·JOIN `-= USING, NATURAL JOIN` |
| `[start]` `WITH (NOLOCK)` | 문장 시작어 | 표 힌트(FROM 표 뒤) | ✖ | start에서 삭제(FROM에는 있음 ✅) |
| `[start]` SAVEPOINT·DESCRIBE·EXPLAIN(상속) | 제안 | `SAVE TRANSACTION` · DESCRIBE/EXPLAIN 없음(Synapse 제외) | ✖/➕ | `-= SAVEPOINT, DESCRIBE, EXPLAIN` · `+= SAVE TRANSACTION, COMMIT TRANSACTION, ROLLBACK TRANSACTION, DBCC, WAITFOR, OPEN, FETCH NEXT FROM, CLOSE, DEALLOCATE, RETURN` |
| `[clause WITH]` `RECURSIVE` · UNION `DISTINCT` | 상속 | 없음(재귀 CTE는 낱말 없이) · UNION/UNION ALL만 | ✖ | WITH `next = AS, SELECT` · UNION `next = SELECT, ALL` |
| MERGE | ansi | `WHEN NOT MATCHED [BY TARGET]` · `WHEN NOT MATCHED BY SOURCE` · `OUTPUT` · 끝 `;` 필수 | ➕ | statement MERGE `clauses += WHEN NOT MATCHED BY SOURCE, WHEN NOT MATCHED BY TARGET, OUTPUT` · 두 절 `same = WHEN MATCHED`/`WHEN NOT MATCHED` |
| CREATE `OR REPLACE`(상속) · ALTER TABLE `RENAME TO/COLUMN`(상속) | 제안 | 없음(`OR ALTER` · `sp_rename`) | ✖ | CREATE `-= OR REPLACE` · ALTER TABLE `-= RENAME TO, RENAME COLUMN` |
| TOP 꼬리 | first += TOP | `TOP (n) [PERCENT] [WITH TIES]` | ⚠ | §4 `after_item`(TOP 뒤 PERCENT/WITH TIES)은 절이 아니라 처리 방법 별도 검토 |
| 그 밖(CROSS/OUTER APPLY·PIVOT·TABLESAMPLE·OUTPUT(INSERT)·EXEC·OR ALTER·DROP IF EXISTS) | | | ✅ | — |

#### 10-3-4. PostgreSQL

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| `[clause SELECT] first` | DISTINCT, ALL | `DISTINCT ON (식)` | ➕ | `first += DISTINCT ON` |
| 잠금 절 | FOR UPDATE, FOR SHARE | `FOR {UPDATE\|NO KEY UPDATE\|SHARE\|KEY SHARE} [OF t] [NOWAIT\|SKIP LOCKED]` | ➕ | clauses·FROM·WHERE·ORDER BY·LIMIT next `+= FOR NO KEY UPDATE, FOR KEY SHARE` · `[clause FOR UPDATE] next = OF, NOWAIT, SKIP LOCKED` · `FOR SHARE … same = FOR UPDATE` |
| `[clause FETCH FIRST]` `PERCENT ROWS ONLY`(상속) | 제안 | PERCENT 없음 · WITH TIES(13+) ✅ | ✖ | `next = ROWS ONLY, ROW ONLY, WITH TIES` |
| `[clause LIMIT]` next | OFFSET, ALL | `LIMIT {n\|ALL}` · 뒤에 OFFSET·FOR UPDATE | ⚠ | ALL은 `first = ALL`로 · `next = OFFSET, FOR UPDATE, FOR SHARE` |
| `[clause OFFSET]` next | ROWS, LIMIT, FETCH FIRST | + ROW · FETCH NEXT · FOR UPDATE | ➕ | `next += ROW, FETCH NEXT, FOR UPDATE` |
| `[start]` | DO, COPY … | `CALL`(11+) · `START TRANSACTION` · `LOCK TABLE` · `REFRESH MATERIALIZED VIEW` · `COMMENT ON` · `PREPARE`/`EXECUTE`/`DEALLOCATE` · `TABLE 이름` · `VALUES` · `RELEASE SAVEPOINT` · `REINDEX` · `CLUSTER` · `MERGE INTO`(15+ ✅) | ➕ | `next += CALL, START TRANSACTION, LOCK TABLE, REFRESH MATERIALIZED VIEW, COMMENT ON, PREPARE, EXECUTE, DEALLOCATE, TABLE, VALUES, RELEASE SAVEPOINT, REINDEX` · `-= DESCRIBE` |
| FROM 표 함수 | UNNEST, LATERAL … | `WITH ORDINALITY` · `ROWS FROM ( )` | ➕ | `next += WITH ORDINALITY, ROWS FROM` |
| `[clause INSERT INTO]` `ON CONFLICT` | INSERT INTO 바로 뒤 | VALUES/SELECT 뒤에만 | ⚠(배치) | §4 `after_item`로 VALUES 뒤로(현재 VALUES에도 있음 ✅) · `OVERRIDING USER VALUE` 추가 |
| `[clause ON CONFLICT]` | DO NOTHING, DO UPDATE SET, ON CONSTRAINT | + `WHERE`(인덱스 술어) | ➕ | `next += WHERE` |
| MERGE | ansi | `THEN DO NOTHING`(15+) · `WHEN NOT MATCHED BY SOURCE`·`RETURNING`(17+) | ➕ | WHEN MATCHED·NOT MATCHED `next += THEN DO NOTHING` · `clauses += WHEN NOT MATCHED BY SOURCE, RETURNING`(⚠ 17+) |
| `[clause GROUP BY]` | ROLLUP… | `GROUP BY [ALL\|DISTINCT]`(14+) | ➕ | `first = ALL, DISTINCT` |
| 그 밖(LATERAL·TABLESAMPLE·ONLY·RETURNING·ON CONFLICT·USING·WINDOW·MATERIALIZED·ILIKE·SIMILAR TO·IS DISTINCT FROM·NULLS FIRST·UNION DISTINCT) | | | ✅ | — |

#### 10-3-5. SQLite

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| `FETCH FIRST`/`ROWS ONLY`(상속, clauses·FROM·WHERE·ORDER BY) | 제안 | 없음(LIMIT/OFFSET만) | ✖ | clauses·각 절 `-= FETCH FIRST, ROWS ONLY` |
| `[start]` MERGE INTO·GRANT·REVOKE·TRUNCATE TABLE·DESCRIBE·DECLARE(상속) | 제안 | 모두 없음 | ✖ | `-= MERGE INTO, GRANT, REVOKE, TRUNCATE TABLE, DESCRIBE, DECLARE` |
| `[start]` | INSERT OR REPLACE/IGNORE … | `INSERT OR {ROLLBACK\|ABORT\|FAIL}` · `UPDATE OR …` · `SAVEPOINT`·`RELEASE` · `BEGIN {DEFERRED\|IMMEDIATE\|EXCLUSIVE}` | ➕ | `next += RELEASE SAVEPOINT, BEGIN IMMEDIATE, BEGIN EXCLUSIVE, UPDATE OR REPLACE, UPDATE OR IGNORE` |
| `[clause WITH]` | RECURSIVE | `AS [NOT] MATERIALIZED`(3.35+) | ➕ | `next += MATERIALIZED, NOT MATERIALIZED` |
| `DELETE FROM` `USING`(상속) · `LIMIT` | 제안 | USING 없음 · DELETE/UPDATE LIMIT은 `SQLITE_ENABLE_UPDATE_DELETE_LIMIT` 빌드 한정 | ✖/⚠ | `-= USING` · LIMIT은 주석으로 조건 표시 |
| UNION `DISTINCT`(상속) | 제안 | 없음 | ✖ | `[clause UNION] next = SELECT, ALL` |
| CREATE `OR REPLACE`·SEQUENCE·PROCEDURE·FUNCTION·SCHEMA·TYPE(상속) | 제안 | 없음 | ✖ | `[clause CREATE] next = TABLE, VIEW, INDEX, UNIQUE INDEX, TRIGGER, TEMP TABLE, VIRTUAL TABLE …`(`=` 교체) · DROP 같음 |
| RIGHT/FULL JOIN(상속) | 제안 | 3.39+만 | ⚠ | 유지(버전 주석) |
| 그 밖(PRAGMA·INDEXED BY·RETURNING 3.35+·ON CONFLICT·UPDATE FROM 3.33+·WINDOW·NULLS FIRST 3.30+·GLOB·REGEXP) | | | ✅ | — |

#### 10-3-6. MySQL 8.4

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| `[clause SELECT] first` | (ansi) DISTINCT, ALL · `SQL_CALC_FOUND_ROWS`·`STRAIGHT_JOIN`은 next | `[ALL\|DISTINCT\|DISTINCTROW] [HIGH_PRIORITY] [STRAIGHT_JOIN] [SQL_SMALL_RESULT] [SQL_BIG_RESULT] [SQL_BUFFER_RESULT] [SQL_NO_CACHE] [SQL_CALC_FOUND_ROWS]` | ➕/✖(배치) | `first += DISTINCTROW, HIGH_PRIORITY, STRAIGHT_JOIN, SQL_SMALL_RESULT, SQL_BIG_RESULT, SQL_BUFFER_RESULT, SQL_NO_CACHE, SQL_CALC_FOUND_ROWS` · next에서 앞 둘 삭제 |
| 잠금 절 위치 | FROM next에만 FOR UPDATE | `… WHERE … ORDER BY … LIMIT … FOR {UPDATE\|SHARE} [OF t] [NOWAIT\|SKIP LOCKED]` · `LOCK IN SHARE MODE` | ➕ | WHERE·GROUP BY·HAVING·ORDER BY·LIMIT next `+= FOR UPDATE, FOR SHARE` · clauses `+= FOR SHARE` · `[clause FOR UPDATE] next = OF, NOWAIT, SKIP LOCKED` |
| `[clause GROUP BY]` | ansi | `WITH ROLLUP`(ROLLUP() 형식 없음) | ➕/✖ | `next += WITH ROLLUP, LIMIT` · (ansi에 ROLLUP을 올리면) `-= ROLLUP, CUBE, GROUPING SETS` |
| `INTO OUTFILE/DUMPFILE/@var` | 없음 | FROM 앞 또는 끝(8.0.20+ 끝 권장) | ➕ | clauses `+= INTO` · `[clause INTO] next = OUTFILE, DUMPFILE, FROM` · LIMIT·ORDER BY next `+= INTO` |
| FROM 인덱스 힌트 | 없음 | `USE INDEX`·`FORCE INDEX`·`IGNORE INDEX` · `PARTITION (p)` · `LATERAL`(8.0.14+) | ➕ | FROM·JOIN `next += USE INDEX, FORCE INDEX, IGNORE INDEX, PARTITION, LATERAL` |
| `WINDOW` 절 | 없음 | 8.0+ ✅ | ➕ | clauses·FROM·WHERE·GROUP BY·HAVING `+= WINDOW` · `[clause WINDOW]` 정의(pg와 같음) |
| `FULL JOIN`·`FETCH FIRST`·`ROWS ONLY`·`NULLS FIRST/LAST`·`RETURNING`·`DEFAULT VALUES`·`MERGE INTO`(상속) | 제안 | 모두 없음(MariaDB는 RETURNING 일부) | ✖ | 각 절 `-=` · INSERT는 `INSERT INTO t () VALUES ()` |
| `[clause INSERT INTO]` | ON DUPLICATE KEY UPDATE | `INSERT … SET col=…` · `VALUE` · `AS 별칭`(8.0.19 행 별칭) · ON DUPLICATE는 VALUES/SELECT 뒤 | ➕/⚠ | `next += SET, VALUE, AS` · ON DUPLICATE는 §4 after_item |
| `[clause UPDATE]` `IGNORE` | next | `UPDATE [LOW_PRIORITY] [IGNORE] 표` = 표 앞 | ✖(배치) | `first = LOW_PRIORITY, IGNORE` · next에서 IGNORE 삭제 |
| `[clause ALTER]` next ADD COLUMN·MODIFY COLUMN·CHANGE COLUMN·ENGINE | ALTER 바로 뒤 | `ALTER TABLE t ADD …` | ✖(배치) | 이 항목을 `[clause ALTER TABLE] next +=`로 옮김 |
| `[start]` | USE, SHOW… | `LOAD DATA`·`HANDLER`·`DO`·`PREPARE`/`EXECUTE`·`TABLE`/`VALUES ROW`(8.0.19)·`EXPLAIN ANALYZE`·`ANALYZE/OPTIMIZE/CHECK TABLE`·`KILL`·`FLUSH`·`SHOW COLUMNS/INDEX/PROCESSLIST/VARIABLES/STATUS/GRANTS` | ➕ | 위 낱말 `next +=` · `-= MERGE INTO` |
| 그 밖(LIMIT/OFFSET·REPLACE INTO·INSERT IGNORE·DELETE USING/LIMIT·WITH RECURSIVE·INTERSECT/EXCEPT 8.0.31+ ⚠·LOCK IN SHARE MODE) | | | ✅ | — |

#### 10-3-7. Db2 LUW 11.5

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| `[clause SELECT]` next `FETCH FIRST` | SELECT 목록 안 | FETCH는 FROM…ORDER BY 뒤 | ✖ | 삭제 |
| FETCH 형태 | FETCH FIRST만 | `FETCH {FIRST\|NEXT} n {ROW\|ROWS} ONLY` · `OFFSET n ROWS` · `LIMIT n [OFFSET m]`(11.1+) | ➕ | clauses·각 절 `+= FETCH NEXT` · `[clause FETCH NEXT] same = FETCH FIRST` |
| 잠금/커서 꼬리 | FOR UPDATE, FOR READ ONLY, OPTIMIZE FOR | + `FOR FETCH ONLY` · `SKIP LOCKED DATA` · `WITH RS/RR USE AND KEEP {SHARE\|UPDATE\|EXCLUSIVE} LOCKS` | ➕ | `+= FOR FETCH ONLY, SKIP LOCKED DATA` · `[clause WITH RS] next = USE AND KEEP` |
| `[clause UPDATE]` `WHERE CURRENT OF` | UPDATE 바로 뒤 | SET 뒤 WHERE 자리 | ✖(배치) | 삭제 · `[clause WHERE] first += CURRENT OF` |
| RETURNING·DELETE USING·DEFAULT VALUES(상속) | 제안 | 없음(`SELECT … FROM FINAL TABLE (INSERT …)`) | ✖ | 각 절 `-=` |
| `[clause WITH]` `RECURSIVE`(상속) | 제안 | 재귀 CTE에 RECURSIVE 낱말 없음 | ✖ ⚠ | `next = AS, SELECT` |
| `[start]` `EXPLAIN PLAN SELECTION FOR` | | `EXPLAIN {PLAN SELECTION\|ALL\|PLAN} [FOR] …` | ⚠ | `EXPLAIN PLAN FOR, EXPLAIN ALL FOR` 추가 |
| FROM | TABLE, LATERAL, FINAL/NEW/OLD TABLE | + `TABLESAMPLE {BERNOULLI\|SYSTEM}` | ➕ | `next += TABLESAMPLE` |
| MERGE | ansi | + `ELSE IGNORE` · `SIGNAL` | ➕(낮음) | statement MERGE `clauses += ELSE IGNORE` |
| SELECT next 함수·형식 이름 70여 개 | | 함수는 카탈로그 몫 | ⚠ | 함수 목록 분리(별도 키) 권장 |
| 그 밖(WITH UR/CS/RS/RR·OPTIMIZE FOR·VALUES·CALL·ROLLUP/CUBE·NULLS FIRST ⚠·IF [NOT] EXISTS 11.5 ⚠) | | | ✅ | — |

#### 10-3-8. ClickHouse

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| `[clause SELECT]` next `FINAL, PREWHERE, ARRAY JOIN, LEFT ARRAY JOIN, SAMPLE, WITH TOTALS, LIMIT BY, ASOF/ANY/ALL/GLOBAL/SEMI/ANTI JOIN` | SELECT 목록 뒤 | 전부 FROM 뒤(또는 GROUP BY 뒤) | ✖(배치) | SELECT next에서 삭제(FROM에는 이미 있음 ✅) |
| `[clause SELECT] first` | DISTINCT, ALL | + `DISTINCT ON (…)` · `TOP n [WITH TIES]` | ➕ | `first += DISTINCT ON, TOP` |
| 조인 종류 | FROM next 낱말만 | `[GLOBAL] [ANY\|ALL\|ASOF] [INNER\|LEFT\|RIGHT\|FULL\|CROSS] JOIN` · `LEFT/RIGHT SEMI\|ANTI JOIN` · `PASTE JOIN` | ➕/⚠ | `[clause ASOF JOIN] same = JOIN` 등 각 종류 별칭 · `LEFT SEMI JOIN, LEFT ANTI JOIN, PASTE JOIN, ASOF LEFT JOIN` 추가 · 단독 `SEMI JOIN` ⚠ |
| 꼬리 절 | … SETTINGS, FORMAT | + `QUALIFY`(24.x) · `INTO OUTFILE` · `WITH FILL … INTERPOLATE` | ➕ | clauses `+= QUALIFY, INTO OUTFILE, INTERPOLATE` |
| `[clause GROUP BY]` | WITH TOTALS/ROLLUP/CUBE | + `ROLLUP( )`·`CUBE( )`·`GROUPING SETS( )` · `GROUP BY ALL` | ➕ | ansi로 올린 ROLLUP 상속 · `first = ALL` · ORDER BY `first = ALL` |
| 변경(UPDATE/DELETE) | DELETE FROM(상속) | `ALTER TABLE t {UPDATE … WHERE \| DELETE WHERE}` 뮤테이션 · 경량 `DELETE FROM`(22.8+) · 경량 `UPDATE`(25.7+ ⚠) | ➕/⚠ | `[clause ALTER TABLE] next += UPDATE, DELETE WHERE, ADD COLUMN, MODIFY COLUMN, DROP PARTITION, MATERIALIZE INDEX, ON CLUSTER` |
| MERGE·SAVEPOINT·DECLARE·RETURNING·DELETE USING(상속) | 제안 | 없음 | ✖ | `-=` |
| `[clause INSERT INTO]` | VALUES, SELECT, FORMAT, SETTINGS | + `FUNCTION`(INSERT INTO FUNCTION) | ➕ | `next += FUNCTION` · `-= DEFAULT VALUES, RETURNING` |
| `ON CLUSTER` | 없음 | DDL·DELETE 공통 | ➕ | CREATE·ALTER·DROP·TRUNCATE next `+= ON CLUSTER` |
| SELECT next 함수 100여 개 | | 카탈로그 몫 | ⚠ | 분리 권장 |
| 그 밖(PREWHERE·SAMPLE OFFSET·ARRAY JOIN·LIMIT BY·WITH FILL·SETTINGS·FORMAT·OFFSET FETCH·FINAL·SYSTEM·KILL QUERY) | | | ✅ | — |

#### 10-3-9. MS Access(ACE)

| 항목 | .sqlg 현재 | 공식 문법 | 판정 | 제안 |
|---|---|---|---|---|
| `TOP`·`DISTINCTROW`를 절로 등록 | statement SELECT clauses += TOP, DISTINCTROW | `SELECT [ALL\|DISTINCT\|DISTINCTROW] [TOP n [PERCENT]] …` = SELECT 첫 자리 낱말 | ⚠(우회) | `first += DISTINCTROW, TOP`로 바꾸고 clauses·[clause TOP/DISTINCTROW] 제거(TOP 뒤 PERCENT는 §4 after_item) |
| `[clause TOP]` next `DISTINCT` | TOP 뒤 | 술어(DISTINCT)는 TOP 앞 | ✖ | `DISTINCT` 삭제 |
| LIMIT·OFFSET·FETCH FIRST·ROWS ONLY(상속) | 살아 있음(파일 주석과 모순) | 없음 | ✖ | clauses·각 절 `-=` |
| WITH·MERGE INTO·TRUNCATE TABLE·SAVEPOINT·EXPLAIN·DESCRIBE·DECLARE(상속) | 시작어 | 없음 | ✖ | `[start] -=` |
| GRANT/REVOKE·BEGIN/COMMIT/ROLLBACK | 상속 | ANSI-92 모드에서만(`BEGIN TRANSACTION`) | ⚠ | 유지 + 주석 |
| FULL JOIN·CROSS JOIN·NATURAL JOIN·USING·INTERSECT·EXCEPT·UNION DISTINCT·NULLS FIRST/LAST(상속) | 제안 | 없음(INNER/LEFT/RIGHT [OUTER]만 · UNION [ALL]) | ✖ | 각 절 `-=` |
| `SELECT … INTO 새표 [IN 외부DB]`(테이블 생성 쿼리) | 없음 | ✅ | ➕ | clauses `+= INTO` · `[clause INTO] next = IN, FROM` |
| `DELETE * FROM` / `DELETE 표.* FROM` | 없음 | ✅ | ➕ | `[start] next += DELETE * FROM` |
| RETURNING·DEFAULT VALUES(상속) | 제안 | 없음 | ✖ | `-=` |
| CREATE/DROP `SEQUENCE, TRIGGER, FUNCTION, SCHEMA, TYPE, OR REPLACE`(상속) | 제안 | 없음 · VIEW/PROCEDURE는 ANSI-92 모드 | ✖/⚠ | `[clause CREATE] next = TABLE, INDEX, UNIQUE INDEX, VIEW, PROCEDURE`(`=` 교체) · DROP 같음 |
| 그 밖(TRANSFORM/PIVOT IN·PARAMETERS·IN 외부DB·WITH OWNERACCESS OPTION·ALIKE·LEFT OUTER JOIN) | | | ✅ | — |

### 10-4. 절 안 위치 키 제안

> 현황: 작업 트리에 `first` 키가 이미 들어 있다(T-200 · ansi `[clause SELECT] first = DISTINCT, ALL` · mssql `first += TOP` · `Grammar::next_at` = First/AfterComma/Other). 아래는 **값 채우기 + 키 2개 추가** 제안.

#### 10-4-1. 키 정의(제안)

| 키 | 뜻 | `next_at` 자리 First / AfterComma / Other |
|---|---|---|
| `first`(있음) | 절 머리 바로 뒤에만 | 보임 / 숨김 / 숨김 |
| `item`(신규) | 항목이 시작하는 모든 자리(첫 자리 + 콤마 뒤) — `GROUP BY a, ROLLUP(b)` 같은 것 | 보임 / 보임 / 숨김 |
| `after_item`(신규) | 항목이 하나 이상 나온 뒤에만(항목 꼬리) — `ASC`·`NULLS LAST`·`WITH ROLLUP`·`VALUES` | 숨김 / 숨김 / 보임 |
| `-=`(신규 연산자) | 부모 목록에서 빼기(`next`·`first`·`clauses`·`[start] next` 공통) | — |

`next`에 남는 것 = 자리 무관(현행 규칙 그대로). `after_item`·`item`에 적은 낱말은 `next`에 다시 적지 않는다(파서가 합쳐서 판정).

#### 10-4-2. 방언별 값

| 절 | ansi | 방언 덧붙임 |
|---|---|---|
| SELECT `first` | DISTINCT, ALL | oracle `+= UNIQUE` · mssql `+= TOP` · postgres `+= DISTINCT ON` · mysql `+= DISTINCTROW, HIGH_PRIORITY, STRAIGHT_JOIN, SQL_SMALL_RESULT, SQL_BIG_RESULT, SQL_BUFFER_RESULT, SQL_NO_CACHE, SQL_CALC_FOUND_ROWS` · clickhouse `+= DISTINCT ON, TOP` · access `+= DISTINCTROW, TOP` · sqlite·db2 = ansi |
| SELECT `item` | CASE, CAST, NOT, EXISTS, NULL, TRUE, FALSE | (함수명은 카탈로그) |
| SELECT `after_item` | AS, OVER, IN, BETWEEN, LIKE, IS NULL, IS NOT NULL, AND, OR, COLLATE | mssql TOP 꼬리 = 별도(아래 ⚠) |
| ORDER BY `first` | (없음) | clickhouse `= ALL` · oracle은 `ORDER SIBLINGS BY` 별도 절 |
| ORDER BY `after_item` | ASC, DESC, NULLS FIRST, NULLS LAST | postgres `+= USING` · clickhouse `+= WITH FILL` · 전 방언 `COLLATE` · mssql·mysql·access `-= NULLS FIRST, NULLS LAST` |
| GROUP BY `first` | (없음) | postgres `= ALL, DISTINCT`(14+) · clickhouse `= ALL` |
| GROUP BY `item` | ROLLUP, CUBE, GROUPING SETS | mysql·sqlite·access `-=`(없음) · clickhouse ⚠(콤마 뒤 혼용 가능 여부) |
| GROUP BY `after_item` | (없음) | mysql `= WITH ROLLUP` · mssql `= WITH ROLLUP, WITH CUBE` · clickhouse `= WITH TOTALS, WITH ROLLUP, WITH CUBE` |
| INSERT INTO `after_item`(표 이름 뒤) | VALUES, SELECT, WITH, DEFAULT VALUES | mssql `+= OUTPUT, EXEC` · postgres `+= OVERRIDING SYSTEM VALUE, OVERRIDING USER VALUE` · mysql `+= SET, VALUE` · clickhouse `+= FORMAT, SETTINGS` · access `+= IN` · ON CONFLICT/ON DUPLICATE KEY UPDATE는 `[clause VALUES]`·`SELECT` 뒤로만 |
| UPDATE `first` | (없음) | mysql `= LOW_PRIORITY, IGNORE` · mssql `= TOP` · postgres `= ONLY` |
| UPDATE `after_item` | SET | mssql·mysql `+= AS`(별칭) ⚠ · postgres `+= AS` |
| DELETE FROM `first` | (없음) | postgres `= ONLY` · (mssql TOP·mysql LOW_PRIORITY/QUICK/IGNORE·access `*`는 FROM 앞이므로 `[start]` 낱말로) |
| DELETE FROM `after_item` | WHERE | postgres `+= USING, RETURNING` · mysql `+= USING, ORDER BY, LIMIT` · mssql `+= OUTPUT, FROM` · sqlite `+= RETURNING` |
| FETCH FIRST `after_item`(n 뒤) | ROWS ONLY, ROW ONLY, PERCENT, WITH TIES | mssql·postgres·db2 `-= PERCENT` · mssql `-= WITH TIES` |
| LIMIT `first` | (없음) | postgres `= ALL` |
| LIMIT `after_item` | OFFSET | clickhouse `+= BY, WITH TIES` · mysql·pg `+= FOR UPDATE` · mysql `+= INTO` |
| OFFSET `after_item` | ROWS, ROW, FETCH FIRST, FETCH NEXT | sqlite·mysql = (없음) |
| UNION `first` | ALL, DISTINCT | oracle·mssql·sqlite·access `-= DISTINCT` |
| WITH `first` | RECURSIVE | oracle·mssql·db2·access `-= RECURSIVE` |
| WHERE `first` | (없음) | oracle·db2 `= CURRENT OF` |

⚠ **TOP 꼬리**(mssql `TOP (n) PERCENT WITH TIES` · access `TOP n PERCENT`): TOP은 절이 아니라 SELECT의 `first` 낱말이라 `after_item`로는 "TOP n 뒤"를 표현할 수 없다. 안 = `[modifier TOP] next = PERCENT, WITH TIES`처럼 **낱말 단위 꼬리** 섹션을 두고 엔진이 직전 낱말이 TOP+숫자일 때만 낸다.

#### 10-4-3. CASE 식 안 상태(절이 아닌 별도 제안)

- 현재 `WHEN/THEN/ELSE/END`가 SELECT·ON·WHERE·HAVING·SET의 `next`에 상시 들어 있어 CASE 밖에서도 뜬다.
- 제안: 식 내부 상태를 절과 따로 둔다.
  ```
  [expression CASE]
  open  = CASE            # 진입 · 깊이 +1(중첩 허용)
  after_open = WHEN       # 검색형은 곧바로 WHEN · 단순형은 식 뒤 WHEN
  after_when = THEN
  after_then = WHEN, ELSE, END
  after_else = END
  close = END             # 깊이 -1 · 깊이 0이면 바깥 절로 복귀
  ```
- 엔진: 커서 앞 토큰을 거꾸로 훑어 짝 안 맞는 CASE가 있으면(괄호·문자열·주석 제외) 이 상태가 절 상태보다 우선 · 바깥 절의 `objects`(컬럼·함수)는 그대로 유지.
- 그 뒤 모든 절 `next`에서 `WHEN, THEN, ELSE, END` 삭제(`CASE`만 `item`에). MERGE의 `WHEN MATCHED`는 절 이름이라 영향 없음.

### 10-5. 우선순위(사용자 체감 순 · 상위 10)

1. **`-=` 연산자 도입**(grammar.rs 파서 한 분기 + resolve) — 아래 대부분의 ✖ 정리 전제. 없으면 `=` 전체 재기술로 대체(유지보수 부담 큼).
2. **ansi: FROM·WHERE·ORDER BY next에서 `ROWS ONLY` 삭제 · ORDER BY next `+= OFFSET, FETCH NEXT`** — 모든 방언에 즉시 효과.
3. **CASE 상태 분리 + 절 next에서 `WHEN/THEN/ELSE/END` 삭제**(§4-3) — 가장 자주 보이는 잡음.
4. **ORDER BY `after_item = ASC, DESC, NULLS FIRST, NULLS LAST`** — `ORDER BY |`에 ASC가 뜨는 현상 제거 · mssql·mysql·access는 NULLS 빼기.
5. **mysql: WHERE·ORDER BY·LIMIT next `+= FOR UPDATE, FOR SHARE` · SELECT `first` 수식어 · GROUP BY `after_item = WITH ROLLUP`.**
6. **mssql: `FOR XML`/`FOR JSON`·`OPTION` 절 정의 · `OUTPUT` 절 · GROUP BY ROLLUP/WITH ROLLUP · OFFSET/FETCH를 ORDER BY 뒤로 한정 · RETURNING/NULLS/NATURAL/USING 빼기.**
7. **oracle: FROM·JOIN `-= AS` · `ORDER SIBLINGS BY` 절 · `FOR UPDATE` 절(OF/NOWAIT/WAIT/SKIP LOCKED) · MERGE WHEN MATCHED 형태 · `INSERT ALL/FIRST`.**
8. **access: LIMIT·OFFSET·FETCH·WITH·FULL/CROSS/NATURAL·INTERSECT/EXCEPT 빼기 · TOP/DISTINCTROW를 `first`로 전환 · `[clause TOP]`의 DISTINCT 삭제.**
9. **postgres: SELECT `first += DISTINCT ON` · `FOR NO KEY UPDATE/KEY SHARE` · `[start] += CALL, START TRANSACTION, LOCK TABLE, REFRESH MATERIALIZED VIEW` · FETCH `-= PERCENT`.**
10. **clickhouse: SELECT next에서 FROM 뒤 절(FINAL·PREWHERE·JOIN 종류 등) 삭제 · `ALTER TABLE` 뮤테이션(UPDATE·DELETE WHERE) · 조인 종류 `same = JOIN`** (+ sqlite: FETCH·MERGE·GRANT·TRUNCATE 빼기 · db2: FETCH NEXT·SELECT next의 FETCH FIRST 삭제는 같은 묶음으로).

### 10-6. 불확실 항목(웹 확인 안 함)

- Oracle 23ai: MERGE `WHEN MATCHED AND …`·독립 `THEN DELETE` 지원 여부 · 21c+ `EXCEPT`/`MINUS ALL`/`WINDOW` 절 · 테이블 별칭 `AS` 여전히 불가인지.
- Db2 LUW: 재귀 CTE에 `RECURSIVE` 낱말 허용 여부 · `UNION DISTINCT` 허용 · ORDER BY `NULLS FIRST/LAST` 지원 시작 버전 · `CREATE TABLE IF NOT EXISTS`/`DROP … IF EXISTS`(11.5 기준으로 기억) · `EXPLAIN` 정확한 낱말 순서.
- ClickHouse: 단독 `SEMI JOIN`/`ANTI JOIN` 허용 · `GROUP BY a, ROLLUP(b)` 혼용 · 경량 `UPDATE` 정식화 버전(25.x) · `QUALIFY` 도입 버전 · `WITH RECURSIVE`(24.4+ 기억) · 트랜잭션(BEGIN/COMMIT) 실험 기능 상태.
- SQLite: DELETE/UPDATE `LIMIT` = 컴파일 옵션 의존(배포 빌드별) · RIGHT/FULL JOIN 3.39+.
- MySQL: `INTERSECT`/`EXCEPT` 8.0.31+ · `CREATE OR REPLACE VIEW` 외 OR REPLACE 불가 · `SQL_CALC_FOUND_ROWS` 8.0.17 폐지 예고(제안은 유지하되 낮은 순위).
- Access: GRANT/REVOKE·CREATE VIEW/PROCEDURE·`BEGIN TRANSACTION`·`ALIKE`는 ANSI-92 모드 한정 — 연결 모드에 따라 달라짐.
- MSSQL: `UPDATE t AS a` 별칭 형태(MSSQL은 `UPDATE a SET … FROM t AS a`가 정석) · `IS [NOT] DISTINCT FROM`(2022) 반영 여부.
- 엔진 쪽: `INSERT INTO` 절에서 "표 이름 뒤"를 `after_item`로 판정할 수 있는지(현재 ClausePos는 콤마 기준) — 열 목록 괄호·표 이름을 항목 1개로 세는 규칙이 필요.

### 10-7. 빼기(`-=`) 제안 웹 확인 결과(10-30 · 협업 하위 조사 · 공식 문서 대조)

> 개발 반영 기준 = 아래 "바로 적용 가능한 `.sqlg` 줄(✅만)". ⚠(버전 의존: Oracle 19c vs 23ai 등)은 기준 판을 정한 뒤 · CASE 낱말 삭제는 CASE 상태(§10-4-3) 구현과 **같은 커밋에서만**(먼저 지우면 CASE 안 WHEN/THEN 회귀) · 반박 ✖ 4건(Oracle `FOR UPDATE NOWAIT` 유지 · MySQL `ROLLUP` 유지 · ClickHouse `INSERT … RETURNING` 일부 유지 · ClickHouse `SETTINGS`/`FORMAT` 유지)은 빼지 않는다.

> 대상: `D:\Projects\kiros33\nexa-sql\docs\82-grammar-driven-completion.md` §10-3-x의 ✖/⚠ 중 **낱말을 빼자는 제안**, 그리고 ansi 수준 수정 3건(`ROWS ONLY` 단독 · ORDER BY 다음 OFFSET · CASE 낱말).
> 제약: `-=`는 `next` · `first` · `[start] next`만 된다. **`clauses -=`는 형식 오류** → "그 절로 넘어가는 다른 절들의 `next`에서 빼기"로 바꿔 적었다.
> 저장소 파일은 고치지 않았다. 웹 확인은 2026-10-30 기준이다(Oracle 19c/23ai · MS Learn(SQL Server 2025 = ver17 기준) · PostgreSQL 18 · sqlite.org · MySQL 8.4 · Db2 11.5 · ClickHouse 현행 · Access 2013+ SQL 참조).

#### 1. 요약

1. 빼자는 제안은 **대부분 공식 문법으로 확인됐다(✅)**. 그대로 적용해도 되는 `-=` 줄은 §3에 파일별로 모아 두었다.
2. **반박(✖) 4건**: ① Oracle WHERE 다음 `FOR UPDATE NOWAIT`는 문법상 맞는 연속어다(빼지 말 것 · `SKIP LOCKED` 단독만 뺀다) ② MySQL 8.4는 `GROUP BY ROLLUP(…)` 형식을 **지원한다**(10-3-6 "ROLLUP() 형식 없음"은 틀림 · ROLLUP은 남기고 CUBE/GROUPING SETS만 뺀다) ③ ClickHouse는 `INSERT … RETURNING (SELECT …) VALUES …`가 있다(뺄 것은 VALUES **뒤** RETURNING뿐) ④ ClickHouse SELECT 다음 `SETTINGS`·`FORMAT`은 FROM 없는 SELECT에서 유효하다(초안도 이 둘은 빼자고 하지 않았음 · 확인만).
3. **버전 의존(⚠)**: Oracle **23ai는 `DELETE … USING`(from_using_clause)과 `EXCEPT`·`MINUS ALL`을 지원한다**(19c는 없음). 지원 기준을 19c로 볼지 23ai로 볼지 정해야 한다.
4. **새로 찾은 잘못**: sqlite는 FROM·WHERE·ORDER BY 다음에 `OFFSET`을 자기 파일에서 `+=`로 넣었지만, SQLite의 OFFSET은 `LIMIT n OFFSET m` 안에서만 온다. 그리고 FULL/CROSS JOIN을 빼려면 `[clause ON] next`에서도 빼야 한다(ON 다음에도 조인 낱말이 있음).
5. 같은 파일이 `+=`로 넣은 낱말(mssql `WITH (NOLOCK)`·OFFSET/FETCH NEXT, oracle `SIBLINGS`·`SKIP LOCKED`·`RETURNING INTO`·`WHERE CURRENT OF`, db2 SELECT의 `FETCH FIRST`·UPDATE의 `WHERE CURRENT OF`, clickhouse SELECT의 FROM 뒤 절)은 `-=`가 아니라 **그 `+=` 줄에서 지운다**. `resolve`가 같은 섹션 안에서 `+=`와 `-=`를 어떤 순서로 적용하는지에 기대지 않기 위해서다.

---

#### 2. 방언별 대조

표기: ✅ 확인됨(빼도 됨) · ✖ 반박(빼면 안 됨) · ⚠ 버전 의존 / 구현 의존. "자기 줄 수정" = 해당 방언 파일의 `+=`/`=` 줄에서 낱말을 지운다(`-=` 아님).

##### 2-0. ansi(부모) 수준

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| FROM·WHERE·ORDER BY 다음 `ROWS ONLY` 단독 삭제 | Oracle row_limiting_clause `FETCH { FIRST \| NEXT } [ rowcount \| percent PERCENT ] { ROW \| ROWS } { ONLY \| WITH TIES }` (https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/img_text/row_limiting_clause.html) · PG `FETCH { FIRST \| NEXT } [ count ] { ROW \| ROWS } { ONLY \| WITH TIES }` (https://www.postgresql.org/docs/current/sql-select.html) · T-SQL `FETCH { FIRST \| NEXT } … { ROW \| ROWS } ONLY`는 OFFSET 안에서만 (https://learn.microsoft.com/en-us/sql/t-sql/queries/select-order-by-clause-transact-sql) — `ROWS ONLY`는 어느 방언에서도 FETCH 뒤에만 온다 | ✅ | ansi 자기 줄 수정: `[clause FROM]`·`[clause WHERE]`·`[clause ORDER BY]`의 `next`에서 `ROWS ONLY` 지우기(부모라 `-=` 불필요) |
| ORDER BY 다음 `OFFSET`(+ `FETCH NEXT`) 추가 | 위 세 문서 모두 `[ORDER BY …] [OFFSET n {ROW\|ROWS}] [FETCH …]` · T-SQL은 "FETCH는 OFFSET 뒤에만"(`<offset_fetch> ::= OFFSET … [FETCH …]`) | ✅(더하기) — 단 방언 정리 필요 | ansi `[clause ORDER BY] next`에 `OFFSET, FETCH NEXT` 추가. 그러면 **sqlite·mysql·access**에 `[clause ORDER BY] next -= OFFSET, FETCH NEXT`가 필요하다(SQLite/MySQL의 OFFSET은 LIMIT 안에서만 · Access는 없음 · §2-5·2-6·2-9) |
| SELECT·ON·WHERE·HAVING·SET 다음 `WHEN, THEN, ELSE, END` 삭제 | CASE 식 `CASE WHEN condition THEN result [WHEN …] [ELSE result] END` (https://www.postgresql.org/docs/current/functions-conditional.html · T-SQL CASE 동일) — 네 낱말은 CASE 식 안에서만 문법상 뜻이 있다 | ⚠ 구현 의존 | 문법상 맞다. 다만 82 §10-4-3 **CASE 상태를 구현하기 전에 지우면 CASE 안에서 WHEN/THEN이 안 뜨는 회귀**가 생긴다. CASE 상태와 **같은 커밋에** ansi 자기 줄 수정: `[clause SELECT]`·`[clause ON]`·`[clause WHERE]`·`[clause HAVING]`·`[clause SET]` `next`에서 `WHEN, THEN, ELSE, END` 지우기(`CASE`는 남김) |
| `[statement SELECT] clauses`의 `LIMIT`(ansi에서 빼기) | Oracle·T-SQL·Access 문법에 LIMIT 없음(위 문서 · Access SELECT https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/select-statement-microsoft-access-sql) | ✅(단 `clauses -=` 불가) | ansi `next` 목록에는 LIMIT이 없으므로 `-=`로 할 일이 없다. **ansi `[statement SELECT] clauses`에서 LIMIT을 지우고**, LIMIT이 있는 방언에 `clauses += LIMIT` — postgres·sqlite·mysql은 이미 있음. **clickhouse는 없으니 추가 필요**(db2는 §4 미확인) |

##### 2-1. Oracle(oracle.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| FROM·JOIN 다음 `AS`(테이블 별칭) | table_reference `… [ t_alias ]`에 AS가 없다 — 19c (https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/img_text/table_reference.html) · 23ai도 같다 (…/23/sqlrf/img_text/table_reference.html). 열 별칭만 "The AS keyword is optional"(SELECT 문서) | ✅ | `[clause FROM]` 아래 `next -= AS` · `[clause JOIN]` 아래 `next -= AS` |
| ORDER BY 다음 `SIBLINGS` | `ORDER [ SIBLINGS ] BY { expr \| position \| c_alias } [ ASC \| DESC ] [ NULLS FIRST \| NULLS LAST ]` (…/19/sqlrf/img_text/order_by_clause.html) | ✅ | 자기 줄 수정: oracle `[clause ORDER BY] next +=`에서 `SIBLINGS` 지우기. (`ORDER SIBLINGS BY` 절 신설은 더하기 제안이라 범위 밖) |
| FROM 다음 `WHERE CURRENT OF` | Oracle SQL 참조의 SELECT/DELETE에는 없다(`WHERE CURRENT OF`는 PL/SQL 커서 DML) · 19c DELETE 문법 `… [ t_alias ] [ where_clause ] [ returning_clause ]` (…/19/sqlrf/img_text/delete.html) | ✅ | 자기 줄 수정: oracle `[clause FROM] next +=`에서 `WHERE CURRENT OF` 지우기 |
| WHERE 다음 `SKIP LOCKED` | for_update_clause `FOR UPDATE [ OF … ] [ { NOWAIT \| WAIT integer \| SKIP LOCKED } ]` (…/19/sqlrf/img_text/for_update_clause.html) — SKIP LOCKED는 FOR UPDATE 뒤에만 | ✅ | 자기 줄 수정: oracle `[clause WHERE] next +=`에서 `SKIP LOCKED` 지우기 |
| WHERE 다음 `FOR UPDATE NOWAIT` | 같은 문법에서 `… WHERE … FOR UPDATE NOWAIT`는 유효한 연속어 | ✖ 빼면 안 됨 | 그대로 둔다(나중에 `[clause FOR UPDATE] next = OF, NOWAIT, WAIT, SKIP LOCKED`가 생기면 그때 정리) |
| SET 다음 `RETURNING INTO`(연속어) | `{ RETURN \| RETURNING } ( [OLD\|NEW] expr … ) INTO data_item` — 23ai (…/23/sqlrf/img_text/returning_clause.html) · RETURNING과 INTO 사이에 식이 온다 | ✅ | 자기 줄 수정: oracle `[clause SET] next +=`에서 `RETURNING INTO` 지우기(`RETURNING`은 남김) |
| WHEN MATCHED 다음 `AND`·`THEN DELETE`, WHEN NOT MATCHED 다음 `AND` | merge_update_clause `WHEN MATCHED THEN UPDATE SET … [ where_clause ] [ DELETE where_clause ]` — 23ai (…/23/sqlrf/img_text/merge_update_clause.html) · MERGE 본문 `… ON ( condition ) [ merge_update_clause ] [ merge_insert_clause ] …` (…/23/sqlrf/img_text/merge.html). `WHEN MATCHED AND …`도, 독립 `THEN DELETE`도 없다 | ✅(19c·23ai 모두) | `[clause WHEN MATCHED]` 아래 `next -= AND, THEN DELETE` · `[clause WHEN NOT MATCHED]` 아래 `next -= AND` |
| WITH 다음 `RECURSIVE` | subquery_factoring_clause `query_name ( c_alias [, c_alias]… ) AS { ( [ with_clause ] subquery ) [search_clause] [cycle_clause] \| values_clause }` — 23ai (…/23/sqlrf/img_text/subquery_factoring_clause.html). 19c 본문도 재귀를 "query_name을 자기 정의에서 참조"하는 구조로만 설명한다 | ✅ | `[clause WITH]` 아래 `next -= RECURSIVE` |
| UNION 다음 `DISTINCT` | 19c `{ UNION [ALL] \| INTERSECT \| MINUS }` (…/19/sqlrf/img_text/subquery.html) · 23ai `{ UNION \| INTERSECT \| MINUS \| EXCEPT } [ALL]` (…/23/sqlrf/img_text/subquery.html). DISTINCT는 어느 판에도 없다 | ✅ | `[clause UNION]` 아래 `next -= DISTINCT` |
| MINUS/INTERSECT 다음 `ALL` | 19c에는 없고 23ai(21c+)에는 있다(위 두 문서) | ⚠ 21c+ | 그대로 둔다(`ALL` 유지 · 19c 기준이면 MINUS·INTERSECT에 따로 별칭 섹션이 필요해 `-=`로는 안 됨) |
| LIMIT(상속) | row_limiting_clause가 유일한 행 제한(위) · LIMIT 낱말 없음 | ✅ | `next`에는 LIMIT이 없어 `-=`로 할 일이 없다 → §2-0 ansi `clauses` 정리로 해결 |
| INSERT INTO 다음 `DEFAULT VALUES` | insert_values_clause `VALUES ( { expr \| DEFAULT } … )` — 23ai (…/23/sqlrf/img_text/insert_values_clause.html) · single_table_insert에 DEFAULT VALUES 없음 | ✅ | `[clause INSERT INTO]` 아래 `next -= DEFAULT VALUES` |
| DELETE FROM 다음 `USING` | **19c**: `DELETE … [ t_alias ] [ where_clause ] [ returning_clause ]` (…/19/sqlrf/img_text/delete.html) → 없음. **23ai**: `… [ t_alias ] [ from_using_clause ] [ where_clause ] …`, `from_using_clause = { FROM \| USING } { table_reference \| join_clause … }` (…/23/sqlrf/img_text/delete.html · …/23/sqlrf/img_text/from_using_clause.html) → **있음** | ⚠ 23ai+ | 기준이 19c면 `[clause DELETE FROM]` 아래 `next -= USING`. 23ai 기준이면 남긴다(초안의 ✖ 판정은 23ai에서 틀림) |
| FROM·WHERE·ORDER BY 다음 `ROWS ONLY` | §2-0 | ✅ | ansi에서 해결(oracle 줄 불필요) |

##### 2-2. SQL Server(mssql.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| FROM·WHERE 다음 `OFFSET`, `FETCH NEXT`, `FETCH FIRST`, `ROWS ONLY` | `ORDER BY order_by_expression … [ <offset_fetch> ]`, `<offset_fetch> ::= OFFSET … { ROW \| ROWS } [ FETCH { FIRST \| NEXT } … { ROW \| ROWS } ONLY ]` — OFFSET은 ORDER BY 안의 부분절이고, FETCH는 OFFSET 뒤에만 온다 (https://learn.microsoft.com/en-us/sql/t-sql/queries/select-order-by-clause-transact-sql) | ✅ | 상속분: `[clause FROM]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause WHERE]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause GROUP BY]`·`[clause HAVING]` 아래 `next -= FETCH FIRST`(초안에 없던 같은 결함). 자기 줄 수정: mssql `[clause FROM] next +=`에서 `OFFSET, FETCH NEXT` 지우기 · `[clause WHERE] next +=`에서 `OFFSET` 지우기 |
| ORDER BY 다음 `FETCH NEXT`, `FETCH FIRST`, `ROWS ONLY`, `NULLS FIRST`, `NULLS LAST` | 같은 문서: ORDER BY 항목은 `[ COLLATE … ] [ ASC \| DESC ]`뿐이고 NULLS FIRST/LAST가 없다(SQL Server 2025까지) · FETCH는 OFFSET 뒤에만 | ✅ | `[clause ORDER BY]` 아래 `next -= FETCH FIRST, ROWS ONLY, NULLS FIRST, NULLS LAST`. 자기 줄 수정: mssql `[clause ORDER BY] next +=`에서 `FETCH NEXT, ROWS ONLY` 지우기(`OFFSET`은 남김) |
| FETCH FIRST 다음 `PERCENT ROWS ONLY`, `WITH TIES` | 같은 문서: OFFSET-FETCH에는 PERCENT도 WITH TIES도 없다(`… { ROW \| ROWS } ONLY`) | ✅ | `[clause FETCH FIRST]` 아래 `next -= PERCENT ROWS ONLY, WITH TIES` |
| VALUES/SET/DELETE FROM 다음 `RETURNING`(상속) | T-SQL에는 RETURNING이 없고 `OUTPUT` 절을 쓴다(DELETE: `… [ <OUTPUT Clause> ] [ FROM table_source ] [ WHERE … ]` — https://learn.microsoft.com/en-us/sql/t-sql/statements/delete-transact-sql) | ✅ | `[clause VALUES]` 아래 `next -= RETURNING` · `[clause SET]` 아래 `next -= RETURNING` · `[clause DELETE FROM]` 아래 `next -= RETURNING`. (statement 쪽 `clauses -= RETURNING`은 불가 → 위 세 줄로 대신) |
| DELETE FROM 다음 `TOP`, `USING` | DELETE: `DELETE [ TOP ( expression ) [ PERCENT ] ] [ FROM ] …` — TOP은 FROM 앞 · USING 없음 · 두 번째 `FROM table_source`는 T-SQL 확장(같은 문서 · 구판 2008 R2 ms189835에서도 같은 구조) | ✅ | `[clause DELETE FROM]` 아래 `next -= USING` · 자기 줄 수정: mssql `[clause DELETE FROM] next +=`에서 `TOP` 지우기 |
| FROM·JOIN 다음 `USING`, `NATURAL JOIN` | `<joined_table> ::= <table_source> <join_type> <table_source> ON <search_condition> \| … CROSS JOIN … \| … { CROSS \| OUTER } APPLY …` · `<join_type> ::= [ { INNER \| { { LEFT \| RIGHT \| FULL } [ OUTER ] } } [ <join_hint> ] ] JOIN` — NATURAL·USING 없음 (https://learn.microsoft.com/en-us/sql/t-sql/queries/from-transact-sql) | ✅ | `[clause FROM]` 아래 `next -= USING, NATURAL JOIN` · `[clause JOIN]` 아래 `next -= USING` |
| `[start]`의 `WITH (NOLOCK)` | 같은 문서: `WITH ( <table_hint> )`는 `table_or_view_name … [ [AS] table_alias ]` 뒤의 표 힌트 | ✅ | 자기 줄 수정: mssql `[start] next +=`에서 `WITH (NOLOCK)` 지우기(FROM next에는 남김) |
| `[start]`의 `SAVEPOINT`, `DESCRIBE`, `EXPLAIN` | 세이브포인트는 `SAVE { TRAN \| TRANSACTION }` (https://learn.microsoft.com/en-us/sql/t-sql/language-elements/save-transaction-transact-sql) · `EXPLAIN`은 "Azure Synapse Analytics dedicated SQL pool only" (https://learn.microsoft.com/en-us/sql/t-sql/queries/explain-transact-sql) · DESCRIBE 문장 없음 | ✅(SQL Server/Azure SQL 기준) | `[start]` 아래 `next -= SAVEPOINT, DESCRIBE, EXPLAIN` |
| WITH 다음 `RECURSIVE` | `<common_table_expression> ::= expression_name [ ( column_name [ , ...n ] ) ] AS ( CTE_query_definition )` — 재귀는 자기 참조 구조로만 (https://learn.microsoft.com/en-us/sql/t-sql/queries/with-common-table-expression-transact-sql) | ✅ | `[clause WITH]` 아래 `next -= RECURSIVE` |
| UNION 다음 `DISTINCT` | `{ UNION [ ALL ] { <query_specification> \| ( <query_expression> ) } [ ...n ] }` (https://learn.microsoft.com/en-us/sql/t-sql/language-elements/set-operators-union-transact-sql) | ✅ | `[clause UNION]` 아래 `next -= DISTINCT` |
| CREATE 다음 `OR REPLACE` · ALTER TABLE 다음 `RENAME TO`, `RENAME COLUMN` | T-SQL은 `CREATE OR ALTER`(이미 mssql에 있음) · 이름 변경은 `sp_rename` — ALTER TABLE 문법에 RENAME 없음 (https://learn.microsoft.com/en-us/sql/t-sql/statements/alter-table-transact-sql · 이번에 본문은 열지 않음 → §4) | ✅(문서 대조는 일부만) | `[clause CREATE]` 아래 `next -= OR REPLACE` · `[clause ALTER TABLE]` 아래 `next -= RENAME TO, RENAME COLUMN` |
| `DEFAULT VALUES`(상속) | T-SQL INSERT에 `DEFAULT VALUES`가 있다 | ✖ 빼면 안 됨(초안도 빼자고 하지 않았음 · 확인만) | 그대로 |

##### 2-3. PostgreSQL(postgres.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| FETCH FIRST 다음 `PERCENT ROWS ONLY` | PG 18 SELECT 개요: `[ FETCH { FIRST \| NEXT } [ count ] { ROW \| ROWS } { ONLY \| WITH TIES } ]` — PERCENT 없음 · WITH TIES 있음 (https://www.postgresql.org/docs/current/sql-select.html) | ✅ | `[clause FETCH FIRST]` 아래 `next -= PERCENT ROWS ONLY` |
| LIMIT 다음 `ALL`(자리 문제) | 같은 문서: `[ LIMIT { count \| ALL } ]` — ALL은 LIMIT 바로 뒤 자리 | ✅(자리 옮김) | 자기 줄 수정: `[clause LIMIT] next = OFFSET, FOR UPDATE, FOR SHARE` + `first = ALL` |
| `[start]`의 `DESCRIBE` | SQL 명령 목록에 DESCRIBE 없음(D = DEALLOCATE, DECLARE, DELETE, DISCARD, DO, DROP …) (https://www.postgresql.org/docs/current/sql-commands.html) | ✅ | `[start]` 아래 `next -= DESCRIBE` |
| FROM·WHERE·ORDER BY 다음 `ROWS ONLY` | §2-0 | ✅ | ansi에서 해결 |
| UNION DISTINCT · NULLS FIRST · RETURNING · DELETE USING | 모두 PG에 있음 | ✖ 빼면 안 됨(초안도 ✅로 둠) | 그대로 |

##### 2-4. SQLite(sqlite.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| `FETCH FIRST`·`ROWS ONLY`(상속) | select-stmt의 행 제한은 `LIMIT expr [ OFFSET expr \| , expr ]`뿐이고 FETCH는 문서에 없다 (https://www.sqlite.org/lang_select.html) | ✅ | `[clause FROM]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause WHERE]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause GROUP BY]` 아래 `next -= FETCH FIRST` · `[clause HAVING]` 아래 `next -= FETCH FIRST` · `[clause ORDER BY]` 아래 `next -= FETCH FIRST, ROWS ONLY` (statement `clauses`의 FETCH FIRST는 `-=` 불가 → 남음 · 감지용이라 해는 작음) |
| (신규) FROM·WHERE·ORDER BY 다음 `OFFSET` | 같은 문서: OFFSET은 `LIMIT expr OFFSET expr` 안에서만 | ✅ | 자기 줄 수정: sqlite `[clause FROM]`·`[clause WHERE]`·`[clause ORDER BY]`의 `next +=`에서 `OFFSET` 지우기. ansi ORDER BY에 OFFSET을 더하면 `[clause ORDER BY]` 아래 `next -= OFFSET, FETCH NEXT`도 |
| `[start]`의 MERGE INTO·GRANT·REVOKE·TRUNCATE TABLE·DESCRIBE·DECLARE | SQL 문 목록(ALTER TABLE · ANALYZE · ATTACH · BEGIN · COMMIT · CREATE … · DELETE · DETACH · DROP … · END · EXPLAIN · INSERT · PRAGMA · REINDEX · RELEASE · REPLACE · ROLLBACK · SAVEPOINT · SELECT · UPDATE · VACUUM)에 여섯 다 없음 (https://www.sqlite.org/lang.html) | ✅ | `[start]` 아래 `next -= MERGE INTO, GRANT, REVOKE, TRUNCATE TABLE, DESCRIBE, DECLARE` |
| DELETE FROM 다음 `USING` | `DELETE FROM qualified-table-name [WHERE expr] [returning-clause] [ORDER BY … LIMIT …]` — USING 없음 (https://www.sqlite.org/lang_delete.html) | ✅ | `[clause DELETE FROM]` 아래 `next -= USING` |
| DELETE FROM 다음 `LIMIT`(자기 줄) | 같은 문서: "If SQLite is compiled with the SQLITE_ENABLE_UPDATE_DELETE_LIMIT compile-time option…" | ⚠ 빌드 의존 | 남기되 주석으로 조건을 단다(빼지 않음) |
| UNION 다음 `DISTINCT` | compound-operator = `UNION` · `UNION ALL` · `INTERSECT` · `EXCEPT` (lang_select.html) | ✅ | `[clause UNION]` 아래 `next -= DISTINCT` |
| CREATE/DROP의 OR REPLACE·SEQUENCE·PROCEDURE·FUNCTION·SCHEMA·TYPE | 문 목록에 CREATE는 INDEX·TABLE·TRIGGER·VIEW·VIRTUAL TABLE뿐, DROP은 INDEX·TABLE·TRIGGER·VIEW뿐 (lang.html) | ✅ | `[clause CREATE]` 아래 `next -= OR REPLACE, SEQUENCE, PROCEDURE, FUNCTION, SCHEMA, TYPE` · `[clause DROP]` 아래 `next -= SEQUENCE, PROCEDURE, FUNCTION, SCHEMA, TYPE` |
| (추가 · 초안 밖) ALTER 다음 TABLE 외 낱말 | 문 목록의 ALTER는 ALTER TABLE뿐 (lang.html) | ✅ | `[clause ALTER]` 아래 `next -= VIEW, INDEX, SEQUENCE, TRIGGER, PROCEDURE, FUNCTION, SCHEMA` |
| RIGHT/FULL JOIN | 3.39.0+ (join-operator에 RIGHT·FULL 있음 · lang_select.html) | ⚠ 3.39+ | 남긴다 |
| NATURAL JOIN · JOIN USING · NULLS FIRST/LAST · WITH RECURSIVE | join-operator에 NATURAL · join-constraint `USING ( column-name, … )` · ordering-term `NULLS FIRST \| NULLS LAST` (lang_select.html) | ✖ 빼면 안 됨(초안도 ✅) | 그대로 |

##### 2-5. MySQL 8.4(mysql.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| `FULL JOIN`(상속) | `joined_table: … {[INNER \| CROSS] JOIN \| STRAIGHT_JOIN} … \| … {LEFT\|RIGHT} [OUTER] JOIN … \| … NATURAL [INNER \| {LEFT\|RIGHT} [OUTER]] JOIN …` — FULL 없음 (https://dev.mysql.com/doc/refman/8.4/en/join.html) | ✅ | `[clause FROM]` 아래 `next -= FULL JOIN` · `[clause JOIN]` 아래 `next -= FULL JOIN` · `[clause ON]` 아래 `next -= FULL JOIN` |
| `FETCH FIRST`·`ROWS ONLY`(상속) | SELECT 개요의 행 제한은 `[LIMIT {[offset,] row_count \| row_count OFFSET offset}]`뿐 (https://dev.mysql.com/doc/refman/8.4/en/select.html) | ✅ | `[clause FROM]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause WHERE]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause GROUP BY]` 아래 `next -= FETCH FIRST` · `[clause HAVING]` 아래 `next -= FETCH FIRST` · `[clause ORDER BY]` 아래 `next -= FETCH FIRST, ROWS ONLY` |
| ORDER BY 다음 `NULLS FIRST/LAST` | 같은 문서: `[ORDER BY {col_name \| expr \| position} [ASC \| DESC], … [WITH ROLLUP]]` — NULLS 없음 | ✅ | `[clause ORDER BY]` 아래 `next -= NULLS FIRST, NULLS LAST` (ansi ORDER BY에 OFFSET을 더하면 여기에 `, OFFSET, FETCH NEXT`도) |
| `RETURNING`(상속) | INSERT 개요에 없음 (https://dev.mysql.com/doc/refman/8.4/en/insert.html) · UPDATE/DELETE에도 없음 | ✅ | `[clause VALUES]` 아래 `next -= RETURNING` · `[clause SET]` 아래 `next -= RETURNING` · `[clause DELETE FROM]` 아래 `next -= RETURNING` |
| `DEFAULT VALUES`(상속) | INSERT 개요에 없음 · 대신 `INSERT INTO tbl_name () VALUES();` (insert.html) | ✅ | `[clause INSERT INTO]` 아래 `next -= DEFAULT VALUES` |
| `MERGE INTO`(상속) | MySQL에 MERGE 문 없음(문 목록 · INSERT … ON DUPLICATE KEY UPDATE로 대체) | ✅ | `[start]` 아래 `next -= MERGE INTO` |
| GROUP BY에서 `ROLLUP, CUBE, GROUPING SETS` 빼기(ansi로 올렸을 때) | SELECT 개요: `[GROUP BY [ {col_name \| expr \| position}, … [WITH ROLLUP] \| ROLLUP ({col_name \| expr \| position}, …)] ]` — **ROLLUP( ) 형식이 있다** · CUBE/GROUPING SETS는 없다 | ✖(ROLLUP) / ✅(CUBE·GROUPING SETS) | ansi에 올린 뒤라면 `[clause GROUP BY]` 아래 `next -= CUBE, GROUPING SETS`(ROLLUP은 남김) |
| `[clause UPDATE] next`의 `IGNORE`(자리) | `UPDATE [LOW_PRIORITY] [IGNORE] table_reference SET …` — 표 앞 | ✅(자리 옮김) | 자기 줄 수정: mysql `[clause UPDATE] next +=`에서 `IGNORE` 지우고 `first = LOW_PRIORITY, IGNORE` |
| UNION DISTINCT · DELETE USING · NATURAL JOIN · JOIN USING | `join_specification: { ON search_condition \| USING (join_column_list) }`(join.html) · 다중 표 DELETE `… FROM tbl … USING table_references`(delete 문서) · UNION [ALL \| DISTINCT] | ✖ 빼면 안 됨 | 그대로 |

##### 2-6. Db2 LUW 11.5(db2.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| SELECT 다음 `FETCH FIRST`(자기 줄) | fetch-clause는 select-statement 끝(order-by 뒤)에 온다 · searched DELETE에서도 `… WHERE … order-by-clause offset-clause fetch-clause` 순서 (https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-delete) | ✅ | 자기 줄 수정: db2 `[clause SELECT] next +=`에서 `FETCH FIRST` 지우기 |
| UPDATE 다음 `WHERE CURRENT OF`(자기 줄) | positioned UPDATE: `… correlation-clause SET assignment-clause WHERE CURRENT OF cursor-name` — SET 뒤 (https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-update) | ✅ | 자기 줄 수정: db2 `[clause UPDATE] next +=`에서 `WHERE CURRENT OF` 지우기 |
| DELETE FROM 다음 `USING`, `RETURNING` | searched·positioned DELETE 둘 다 USING·RETURNING이 없다 (statements-delete) | ✅ | `[clause DELETE FROM]` 아래 `next -= USING, RETURNING` |
| VALUES/SET 다음 `RETURNING` | INSERT 문서에 RETURNING 없음 — 변경 행은 `SELECT … FROM FINAL/NEW TABLE (INSERT …)` (https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-insert) | ✅ | `[clause VALUES]` 아래 `next -= RETURNING` · `[clause SET]` 아래 `next -= RETURNING` |
| INSERT INTO 다음 `DEFAULT VALUES` | INSERT 문서: DEFAULT는 VALUES 목록 안 낱말로만 · "DEFAULT VALUES" 없음 | ✅ | `[clause INSERT INTO]` 아래 `next -= DEFAULT VALUES` **그리고** 자기 줄 수정: db2 `[clause INSERT INTO] next +=`에서 `DEFAULT VALUES` 지우기(db2가 스스로 다시 넣고 있음) |
| WITH 다음 `RECURSIVE` | common-table-expression `table-name (column-name, …) AS ( [WITH common-table-expression] fullselect )` — RECURSIVE 없음 · 재귀는 FROM에서 자기 참조 (https://www.ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.sql.ref.doc/doc/r0059217.html) | ✅ | `[clause WITH]` 아래 `next -= RECURSIVE` |
| FROM·WHERE·ORDER BY 다음 `ROWS ONLY` | §2-0 | ✅ | ansi에서 해결 |

##### 2-7. ClickHouse(clickhouse.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| SELECT 다음 `FINAL, PREWHERE, ARRAY JOIN, LEFT ARRAY JOIN, SAMPLE, WITH TOTALS, LIMIT BY, ASOF/ANY/ALL/GLOBAL/SEMI/ANTI JOIN`(자기 줄) | SELECT 개요 순서: `SELECT … expr_list` → `[FROM … ] [FINAL]` → `[SAMPLE …]` → `[ARRAY JOIN …]` → `[GLOBAL] [ANY\|ALL\|ASOF] [INNER\|LEFT\|RIGHT\|FULL\|CROSS] [OUTER\|SEMI\|ANTI] JOIN …` → `[PREWHERE expr]` → `[WHERE]` → `[GROUP BY …] [WITH ROLLUP\|WITH CUBE] [WITH TOTALS]` → … → `[LIMIT … BY columns]` → `[SETTINGS …]` → `[UNION …]` → `[INTO OUTFILE …]` → `[FORMAT format]` (https://clickhouse.com/docs/sql-reference/statements/select) — 전부 FROM 또는 GROUP BY 뒤 | ✅ | 자기 줄 수정: clickhouse `[clause SELECT] next +=`에서 `FINAL, PREWHERE, ARRAY JOIN, LEFT ARRAY JOIN, SAMPLE, WITH TOTALS, LIMIT BY, ASOF JOIN, ANY JOIN, ALL JOIN, GLOBAL JOIN, SEMI JOIN, ANTI JOIN` 지우기. **`SETTINGS`, `FORMAT`은 남긴다**(FROM 없는 `SELECT 1 FORMAT JSON`이 유효) |
| `[start]`의 MERGE INTO, SAVEPOINT, DECLARE | 트랜잭션 문서: BEGIN TRANSACTION(START TRANSACTION)·COMMIT·ROLLBACK만 있고 SAVEPOINT 없음 · "This is an experimental feature" (https://clickhouse.com/docs/guides/developer/transactional) · INSERT 문서: "Other queries for modifying data parts are not supported: UPDATE, DELETE, REPLACE, MERGE, UPSERT, INSERT UPDATE" (https://clickhouse.com/docs/sql-reference/statements/insert-into) · DECLARE 문 없음 | ✅ | `[start]` 아래 `next -= MERGE INTO, SAVEPOINT, DECLARE` (BEGIN/COMMIT/ROLLBACK은 실험 기능이라 ⚠ 유지) |
| DELETE FROM 다음 `USING`, `RETURNING` | 경량 DELETE: `DELETE FROM [db.]table [ON CLUSTER cluster] [IN PARTITION …] WHERE expr;` (https://clickhouse.com/docs/sql-reference/statements/delete) | ✅ | `[clause DELETE FROM]` 아래 `next -= USING, RETURNING` |
| VALUES/SET 다음 `RETURNING` | INSERT RETURNING은 **있다**. 다만 자리가 `INSERT INTO [TABLE] t [(c…)] RETURNING (SELECT …) [SETTINGS …] VALUES …`, 즉 **VALUES 앞**이다(insert-into 문서) · UPDATE에는 문서가 없다 | ✅(VALUES·SET 뒤 RETURNING만) · ✖("CH에 RETURNING 없음"이라는 초안 판단) | `[clause VALUES]` 아래 `next -= RETURNING` · `[clause SET]` 아래 `next -= RETURNING` · (더하기 · 버전 미확인) `[clause INSERT INTO] next += RETURNING` 검토 |
| INSERT INTO 다음 `DEFAULT VALUES` | insert-into 문서: VALUES·FORMAT·SELECT·FUNCTION·SETTINGS 형식만 · DEFAULT는 VALUES 안 낱말로만 | ✅ | `[clause INSERT INTO]` 아래 `next -= DEFAULT VALUES` |
| (참고) LIMIT이 ansi `clauses`에서 빠질 때 | SELECT 개요 `[LIMIT [n, ]m] [WITH TIES]` | — | `[statement SELECT] clauses += LIMIT` 필요(§2-0) |

##### 2-8. MS Access(access.sqlg)

| 제안 | 공식 문서 근거 | 판정 | `.sqlg`에 넣을 줄 |
|---|---|---|---|
| LIMIT·OFFSET·FETCH FIRST·ROWS ONLY(상속) | `SELECT [predicate] { * \| … } FROM tableexpression [, …] [IN externaldatabase] [WHERE…] [GROUP BY…] [HAVING…] [ORDER BY…] [WITH OWNERACCESS OPTION]` — predicate = ALL, DISTINCT, DISTINCTROW, TOP (https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/select-statement-microsoft-access-sql) | ✅ | `[clause FROM]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause WHERE]` 아래 `next -= FETCH FIRST, ROWS ONLY` · `[clause GROUP BY]`·`[clause HAVING]` 아래 `next -= FETCH FIRST` · `[clause ORDER BY]` 아래 `next -= FETCH FIRST, ROWS ONLY` (LIMIT·OFFSET은 ansi `next`에 없어 `clauses` 정리로 해결) |
| INTERSECT·EXCEPT·UNION DISTINCT | `[TABLE] query1 UNION [ALL] [TABLE] query2 [UNION [ALL] …]` — INTERSECT/EXCEPT/DISTINCT 없음 (https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/union-operation-microsoft-access-sql) | ✅ | `[clause FROM]`·`[clause WHERE]`·`[clause GROUP BY]`·`[clause HAVING]`·`[clause ORDER BY]` 아래 각각 `next -= INTERSECT, EXCEPT` · `[clause UNION]` 아래 `next -= DISTINCT` |
| FULL JOIN·CROSS JOIN·NATURAL JOIN·USING | 조인 문서는 `FROM table1 [ LEFT \| RIGHT ] JOIN table2 ON …`(…/left-join-right-join-operations-microsoft-access-sql)과 INNER JOIN뿐 — FULL·CROSS·NATURAL·USING 없음 | ✅ | `[clause FROM]` 아래 `next -= FULL JOIN, CROSS JOIN, NATURAL JOIN, USING` · `[clause JOIN]` 아래 `next -= FULL JOIN, CROSS JOIN, USING` · `[clause ON]` 아래 `next -= FULL JOIN, CROSS JOIN` |
| ORDER BY 다음 `NULLS FIRST/LAST` | SELECT 문서와 ORDER BY 절에 NULLS 없음 | ✅ | `[clause ORDER BY]` 아래 `next -= NULLS FIRST, NULLS LAST` |
| `[start]`의 WITH·MERGE INTO·TRUNCATE TABLE·SAVEPOINT·EXPLAIN·DESCRIBE·DECLARE | Access SQL 참조는 DDL(CREATE/ALTER/DROP · CREATE INDEX/PROCEDURE/VIEW/USER/GROUP · GRANT/REVOKE)과 DML(SELECT · SELECT…INTO · INSERT INTO · UPDATE · DELETE · TRANSFORM · PARAMETERS · UNION)로만 나뉜다 (https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/microsoft-access-sql-reference) | ✅(목록 페이지 기준 · 각 하위 페이지는 일부만 열어 봄) | `[start]` 아래 `next -= WITH, MERGE INTO, TRUNCATE TABLE, SAVEPOINT, EXPLAIN, DESCRIBE, DECLARE` |
| GRANT/REVOKE·BEGIN/COMMIT/ROLLBACK | ANSI-92 모드 한정 | ⚠ 모드 의존 | 남긴다 |
| `[clause TOP]` 다음 `DISTINCT`(자기 줄) | predicate 순서 `[ALL \| DISTINCT \| DISTINCTROW] [TOP n [PERCENT]]` — DISTINCT는 TOP 앞 (SELECT 문서의 predicate 링크) | ✅ | 자기 줄 수정: access `[clause TOP] next =`에서 `DISTINCT` 지우기 |
| RETURNING·DEFAULT VALUES·DELETE USING | Access INSERT/DELETE 문법에 없음 | ✅ | `[clause VALUES]` 아래 `next -= RETURNING` · `[clause SET]` 아래 `next -= RETURNING` · `[clause DELETE FROM]` 아래 `next -= USING, RETURNING` · `[clause INSERT INTO]` 아래 `next -= DEFAULT VALUES` |
| CREATE/DROP의 SEQUENCE·TRIGGER·FUNCTION·SCHEMA·TYPE·OR REPLACE | DROP: `DROP {TABLE table \| INDEX index ON table \| PROCEDURE procedure \| VIEW view}` (https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/drop-statement-microsoft-access-sql) — IF EXISTS도 없음 | ✅ | `[clause CREATE]` 아래 `next -= SEQUENCE, TRIGGER, FUNCTION, OR REPLACE, SCHEMA, TYPE` · `[clause DROP]` 아래 `next -= SEQUENCE, TRIGGER, FUNCTION, SCHEMA, TYPE, IF EXISTS` |

---

#### 3. 바로 적용 가능한 `.sqlg` 줄(✅만)

> ★ **반영됨(10-31 · 191차 17차 · 개발)** — 아래 ✅ 줄을 9개 `.sqlg`에 반영(ansi 자기 줄 + 방언 파일 끝 `-=` 블록 · 같은 `[clause X]` 머리줄 반복 = 파서가 같은 섹션으로 합침) · **보류 3** = CASE 낱말 삭제(CASE 상태 §10-4-3과 같은 커밋) · Oracle `DELETE … USING`(19c/23ai 기준 판 미정) · `MINUS`/`INTERSECT ALL`(같은 이유) · V1(협업 · 기본 방언 Oracle · 접속 없음) = `SELECT * FROM t ` 뒤 후보에 `AS`·`ROWS ONLY` 없음 · `ORDER BY a ` 뒤 `OFFSET`·`FETCH NEXT` 있음 · `SIBLINGS` 없음 ✓.

> `-=` 줄은 그 방언 파일의 해당 섹션 안에 넣는다. 섹션이 없으면 `[clause X]` 머리줄도 새로 만든다. "자기 줄 수정"은 `-=`가 아니라 기존 `+=`/`=` 줄을 고치는 것이다.

##### ansi.sqlg(자기 줄 수정)
```
[clause FROM]
# next에서 `ROWS ONLY` 지우기
[clause WHERE]
# next에서 `ROWS ONLY` 지우기
[clause ORDER BY]
next = ASC, DESC, NULLS FIRST, NULLS LAST, OFFSET, FETCH FIRST, FETCH NEXT, UNION, UNION ALL, INTERSECT, EXCEPT
# (CASE 상태(§10-4-3) 구현과 같은 커밋에서만) SELECT·ON·WHERE·HAVING·SET의 next에서 WHEN, THEN, ELSE, END 지우기
# [statement SELECT] clauses에서 LIMIT 지우기 → clickhouse에 clauses += LIMIT
```

##### oracle.sqlg
```
[clause FROM]
next -= AS
[clause JOIN]
next -= AS
[clause WITH]
next -= RECURSIVE
[clause UNION]
next -= DISTINCT
[clause INSERT INTO]
next -= DEFAULT VALUES
[clause WHEN MATCHED]
next -= AND, THEN DELETE
[clause WHEN NOT MATCHED]
next -= AND
# 자기 줄 수정: [clause FROM] next += 에서 WHERE CURRENT OF 지우기
# 자기 줄 수정: [clause WHERE] next += 에서 SKIP LOCKED 지우기(FOR UPDATE NOWAIT는 남김)
# 자기 줄 수정: [clause ORDER BY] next += 에서 SIBLINGS 지우기
# 자기 줄 수정: [clause SET] next += 에서 RETURNING INTO 지우기
# ⚠ 19c 기준일 때만: [clause DELETE FROM] next -= USING (23ai는 있음)
```

##### mssql.sqlg
```
[start]
next -= SAVEPOINT, DESCRIBE, EXPLAIN
[clause FROM]
next -= FETCH FIRST, ROWS ONLY, USING, NATURAL JOIN
[clause JOIN]
next -= USING
[clause WHERE]
next -= FETCH FIRST, ROWS ONLY
[clause GROUP BY]
next -= FETCH FIRST
[clause HAVING]
next -= FETCH FIRST
[clause ORDER BY]
next -= FETCH FIRST, ROWS ONLY, NULLS FIRST, NULLS LAST
[clause FETCH FIRST]
next -= PERCENT ROWS ONLY, WITH TIES
[clause VALUES]
next -= RETURNING
[clause SET]
next -= RETURNING
[clause DELETE FROM]
next -= USING, RETURNING
[clause WITH]
next -= RECURSIVE
[clause UNION]
next -= DISTINCT
[clause CREATE]
next -= OR REPLACE
[clause ALTER TABLE]
next -= RENAME TO, RENAME COLUMN
# 자기 줄 수정: [start] next += 에서 WITH (NOLOCK) 지우기
# 자기 줄 수정: [clause FROM] next += 에서 OFFSET, FETCH NEXT 지우기
# 자기 줄 수정: [clause WHERE] next += 에서 OFFSET 지우기
# 자기 줄 수정: [clause ORDER BY] next += 에서 FETCH NEXT, ROWS ONLY 지우기(OFFSET 유지)
# 자기 줄 수정: [clause DELETE FROM] next += 에서 TOP 지우기
```

##### postgres.sqlg
```
[start]
next -= DESCRIBE
[clause FETCH FIRST]
next -= PERCENT ROWS ONLY
# 자기 줄 수정: [clause LIMIT] next = OFFSET, FOR UPDATE, FOR SHARE  +  first = ALL
```

##### sqlite.sqlg
```
[start]
next -= MERGE INTO, GRANT, REVOKE, TRUNCATE TABLE, DESCRIBE, DECLARE
[clause FROM]
next -= FETCH FIRST, ROWS ONLY
[clause WHERE]
next -= FETCH FIRST, ROWS ONLY
[clause GROUP BY]
next -= FETCH FIRST
[clause HAVING]
next -= FETCH FIRST
[clause ORDER BY]
next -= FETCH FIRST, ROWS ONLY
[clause DELETE FROM]
next -= USING
[clause UNION]
next -= DISTINCT
[clause CREATE]
next -= OR REPLACE, SEQUENCE, PROCEDURE, FUNCTION, SCHEMA, TYPE
[clause DROP]
next -= SEQUENCE, PROCEDURE, FUNCTION, SCHEMA, TYPE
[clause ALTER]
next -= VIEW, INDEX, SEQUENCE, TRIGGER, PROCEDURE, FUNCTION, SCHEMA
# 자기 줄 수정: [clause FROM]·[clause WHERE]·[clause ORDER BY] next += 에서 OFFSET 지우기
# ansi ORDER BY에 OFFSET, FETCH NEXT를 더하면: [clause ORDER BY] next -= 에 OFFSET, FETCH NEXT 추가
```

##### mysql.sqlg
```
[start]
next -= MERGE INTO
[clause FROM]
next -= FULL JOIN, FETCH FIRST, ROWS ONLY
[clause JOIN]
next -= FULL JOIN
[clause ON]
next -= FULL JOIN
[clause WHERE]
next -= FETCH FIRST, ROWS ONLY
[clause GROUP BY]
next -= FETCH FIRST
[clause HAVING]
next -= FETCH FIRST
[clause ORDER BY]
next -= FETCH FIRST, ROWS ONLY, NULLS FIRST, NULLS LAST
[clause INSERT INTO]
next -= DEFAULT VALUES
[clause VALUES]
next -= RETURNING
[clause SET]
next -= RETURNING
[clause DELETE FROM]
next -= RETURNING
# 자기 줄 수정: [clause UPDATE] next += 에서 IGNORE 지우고 first = LOW_PRIORITY, IGNORE
# ansi ORDER BY에 OFFSET, FETCH NEXT를 더하면: [clause ORDER BY] next -= 에 OFFSET, FETCH NEXT 추가
# ansi GROUP BY에 ROLLUP/CUBE/GROUPING SETS를 올리면: [clause GROUP BY] next -= CUBE, GROUPING SETS (ROLLUP 유지)
```

##### db2.sqlg
```
[clause DELETE FROM]
next -= USING, RETURNING
[clause VALUES]
next -= RETURNING
[clause SET]
next -= RETURNING
[clause INSERT INTO]
next -= DEFAULT VALUES
[clause WITH]
next -= RECURSIVE
# 자기 줄 수정: [clause SELECT] next += 에서 FETCH FIRST 지우기
# 자기 줄 수정: [clause UPDATE] next += 에서 WHERE CURRENT OF 지우기
# 자기 줄 수정: [clause INSERT INTO] next += 에서 DEFAULT VALUES 지우기
```

##### clickhouse.sqlg
```
[start]
next -= MERGE INTO, SAVEPOINT, DECLARE
[clause INSERT INTO]
next -= DEFAULT VALUES
[clause VALUES]
next -= RETURNING
[clause SET]
next -= RETURNING
[clause DELETE FROM]
next -= USING, RETURNING
# 자기 줄 수정: [clause SELECT] next += 에서 FINAL, PREWHERE, ARRAY JOIN, LEFT ARRAY JOIN, SAMPLE, WITH TOTALS, LIMIT BY, ASOF JOIN, ANY JOIN, ALL JOIN, GLOBAL JOIN, SEMI JOIN, ANTI JOIN 지우기(SETTINGS, FORMAT 유지)
# ansi clauses에서 LIMIT을 빼면: [statement SELECT] clauses += LIMIT
```

##### access.sqlg
```
[start]
next -= WITH, MERGE INTO, TRUNCATE TABLE, SAVEPOINT, EXPLAIN, DESCRIBE, DECLARE
[clause FROM]
next -= FULL JOIN, CROSS JOIN, NATURAL JOIN, USING, INTERSECT, EXCEPT, FETCH FIRST, ROWS ONLY
[clause JOIN]
next -= FULL JOIN, CROSS JOIN, USING
[clause ON]
next -= FULL JOIN, CROSS JOIN
[clause WHERE]
next -= INTERSECT, EXCEPT, FETCH FIRST, ROWS ONLY
[clause GROUP BY]
next -= INTERSECT, EXCEPT, FETCH FIRST
[clause HAVING]
next -= INTERSECT, EXCEPT, FETCH FIRST
[clause ORDER BY]
next -= INTERSECT, EXCEPT, FETCH FIRST, ROWS ONLY, NULLS FIRST, NULLS LAST
[clause UNION]
next -= DISTINCT
[clause INSERT INTO]
next -= DEFAULT VALUES
[clause VALUES]
next -= RETURNING
[clause SET]
next -= RETURNING
[clause DELETE FROM]
next -= USING, RETURNING
[clause CREATE]
next -= SEQUENCE, TRIGGER, FUNCTION, OR REPLACE, SCHEMA, TYPE
[clause DROP]
next -= SEQUENCE, TRIGGER, FUNCTION, SCHEMA, TYPE, IF EXISTS
# 자기 줄 수정: [clause TOP] next = 에서 DISTINCT 지우기
# ansi ORDER BY에 OFFSET, FETCH NEXT를 더하면: [clause ORDER BY] next -= 에 OFFSET, FETCH NEXT 추가
```

> 주의 ① `ROWS ONLY`는 ansi에서 지우면 각 방언의 `-= ROWS ONLY`는 필요 없다(남아 있어도 해는 없음 · 빼는 대상이 없을 때 오류로 보는지는 `grammar.rs` 구현 확인). ② `-=` 대상 낱말이 부모에 없을 때 경고/오류인지 → 구현에 맞춰 위 줄을 다듬는다.

---

#### 4. 아직 확인 못 한 것과 이유

- **Oracle 23ai의 `LIMIT`** — 23ai row_limiting_clause를 직접 열지 않았다(19c만 열었다). 23ai에 LIMIT이 생겼다는 근거는 찾지 못했다.
- **Oracle 19c와 23ai 중 어느 판을 기준으로 할지** — DELETE USING과 EXCEPT/MINUS ALL의 판정이 갈린다. 사용자(개발 세션)가 정할 일이다.
- **T-SQL ALTER TABLE에 RENAME이 없음 · CREATE OR ALTER · SAVE TRANSACTION** — URL만 확인했고 본문은 열지 않았다(널리 알려진 사실 · DELETE는 구판 2008 R2 문서 검색 요약으로 확인). EXPLAIN은 "Azure Synapse dedicated SQL pool only"를 검색 요약으로 확인했다.
- **MySQL DELETE … USING** — 다중 표 DELETE 문서는 직접 열지 않았다(기억 · 8.4 delete.html).
- **Db2 11.5 `UNION DISTINCT` · `LIMIT`(11.1+) · `NULLS FIRST/LAST`** — 빼자는 제안이 아니어서 이번 범위에서는 열지 않았다. Db2 ansi `clauses`에서 LIMIT을 뺄 때 db2에 다시 `+= LIMIT`이 필요한지는 미확인이다.
- **ClickHouse INSERT RETURNING 도입 버전** — 문서에 버전 표시가 없다. 단독 `SEMI JOIN`/`ANTI JOIN`(LEFT/RIGHT 없이)이 되는지도 개요만으로는 판단할 수 없다(빼자는 제안은 아님).
- **ClickHouse 문 목록 전체** — 목록 페이지의 발췌가 불완전했다. DECLARE가 없다는 것은 문 목록이 아니라 문서 어디에도 언급이 없다는 데에 근거한다.
- **Access의 각 하위 페이지(CROSS JOIN 불가 · ORDER BY 절 · INSERT/DELETE · CREATE TABLE)** — SELECT·UNION·LEFT/RIGHT JOIN·DROP과 목록 페이지만 열었다. CROSS JOIN이 없다는 판정은 조인 문서에 INNER/LEFT/RIGHT만 있다는 데 근거한다.
- **SQLite ALTER TABLE 안의 `ADD CONSTRAINT`/`DROP CONSTRAINT`** — lang_altertable.html을 열지 않아 표에서 뺐다(SQLite는 RENAME TO · RENAME COLUMN · ADD COLUMN · DROP COLUMN만 있는 것으로 기억 · 다음에 확인).
- **CASE 낱말 삭제** — 문법은 확인했다. 적용 시점은 CASE 상태 구현과 묶여 있다(구현 의존).
- **`grammar.rs`의 `-=` 의미** — 같은 섹션 안 `+=`/`-=`의 적용 순서와 "없는 낱말 빼기"를 어떻게 처리하는지는 코드를 열지 않아 모른다. 그래서 자기 줄 수정을 따로 표시했다.
