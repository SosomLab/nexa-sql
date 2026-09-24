# 82. 문법 참조 기반 완성 — 절(clause)마다 올 수 있는 예약어·객체 · DBMS별 참조 파일 = 플러그인(100차 mac · 2026-09-24)

> 사용자(09-24): "SQLite에서 `SELECT avg` 자리에 `HAVING`이 나온다 — 각 DBMS 문법 구조(재귀 배치 포함)를 분석해 절별로 배치 가능한 예약어·객체를 정리해 구현 · DBMS별 설계 구조를 **참조 파일**로 두고 파일을 추가하면 그 DB를 지원하는 플러그인 구조".

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
