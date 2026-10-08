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
- `objects`는 지금은 기록·진단용(관계 자리 판정은 종전 `RELATION_AFTER`) — 다음 단계에서 이 값으로 컬럼/테이블/함수 후보를 켜고 끈다.

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
- `objects`로 컬럼/테이블/함수 후보 켜고 끄기(지금은 예약어만 절 기반) · 절 안 위치(예: `SELECT` 뒤 첫 항목 = `DISTINCT`/`TOP`만) · `CASE … END` 같은 식 내부 상태 · MySQL 파일 · 방언 문서(공식 문법 다이어그램)와의 대조표.

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
