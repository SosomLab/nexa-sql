# 코드 완성과 아웃라인

편집기에서 타이핑할 때 뜨는 후보 팝업(IntelliSense)과, 현재 문서의 심볼 목록(아웃라인)입니다.

## 3분 사용법(시험용 요약)

1. 접속한 뒤 새 탭에서 `SELECT * FROM ` 까지 치고 잠깐 기다리거나 **Ctrl+Space** → 현재 스키마의 테이블·뷰 · 스키마 이름 · 사전 뷰(`ALL_TABLES` · `sys.tables` · `pg_catalog.pg_class` …)가 뜹니다. `sc`처럼 약어를 쳐도 `SALES_CUSTOMER`가 걸립니다(퍼지).
2. 테이블을 고르고 ` e`(alias)를 붙인 뒤 `SELECT e.`을 치면 **`.`을 치는 즉시** 그 테이블의 컬럼(오른쪽 열 = `PK NUMBER(4)` · `FK … NOT NULL`)이 뜹니다. 탐색기가 아직 안 읽은 테이블이면 "컬럼 불러오는 중…"이 잠깐 보이고 곧 채워집니다.
3. `SELECT nv`를 치면 **내장 함수** `NVL`이 시그니처(`NVL(expr1, expr2)`)와 함께 뜹니다. Enter → `NVL()`이 들어가고 캐럿은 괄호 안 · 상태줄에 `ƒ NVL(expr1, expr2)`(시그니처 도움). `SYSDATE`처럼 인자가 없는 것은 괄호 없이 들어갑니다.
4. Oracle: `DBMS_OUTPUT.`을 치면 `PUT_LINE` 등 패키지 멤버가 뜹니다. SQL Server: `sys.` · `INFORMATION_SCHEMA.` · PostgreSQL: `pg_catalog.` · `information_schema.` 뒤에 카탈로그 뷰.
5. `INSERT INTO emp (`를 치면 첫 후보가 **전체 컬럼**(`EMPNO, ENAME, …`)입니다 — Enter 한 번으로 컬럼 목록 완성.
6. 후보가 많으면 팝업이 **스크롤**됩니다(보이는 행 수 = 설정 `intel.popup_rows` 12 · 휠 · PgUp/PgDn/Home/End).
7. Preferences ▸ Editors ▸ **Code completion**에서 `Insert table alias`를 켜면 FROM 뒤 테이블을 고를 때 `SALES_CUSTOMER sc`처럼 alias까지 들어갑니다(문장 안 alias와 겹치면 `sc2`).
8. **Ctrl+R** = Goto Symbol(문서 심볼 팔레트) · 활동 막대의 아웃라인 아이콘 = 심볼 패널.

## 코드 완성

| 언제 | 무엇 |
|---|---|
| `.`을 치면 즉시 | `alias.` · `table.` = 그 테이블의 컬럼(타입 · PK/FK/UQ · NOT NULL) · `schema.` = 그 스키마의 객체 · `DBMS_OUTPUT.` 같은 시스템 패키지 = 멤버 · `sys.`/`INFORMATION_SCHEMA.`/`pg_catalog.` = 카탈로그 뷰 |
| 식별자 2자 + 잠깐(250 ms) | 문맥에 맞는 후보(아래 표) |
| **Ctrl+Space** | 언제든 수동으로(설정과 무관) |
| `(`를 치면 | 내장 함수·패키지 멤버의 **시그니처가 상태줄에**(`ƒ NVL(expr1, expr2)`) |

문맥별 후보:

| 자리 | 후보 |
|---|---|
| `FROM` · `JOIN` · `UPDATE` · `INSERT INTO` 뒤(콤마 목록 포함) | 현재 스키마의 테이블·뷰·시노님 · 스키마 이름 · 이 문장의 CTE · **사전 뷰**(`ALL_TABLES` · `USER_OBJECTS` · `V$SESSION` · `sys.tables` · `pg_catalog.pg_stat_activity` · `sqlite_master` …) |
| `alias.` 뒤 | 그 테이블의 컬럼 — 탐색기에서 아직 안 읽은 테이블이면 "컬럼 불러오는 중…"이 보이고, 곧 다시 열면 채워집니다 |
| SELECT 목록 · WHERE · ON … | 문장 alias와 그 컬럼(읽어 둔 것) · **내장 함수**(방언별 · 시그니처) · **시스템 패키지 이름** · 문서 심볼(DEFINE·VARIABLE·선언 변수·프로시저) · 키워드 · 이 문서의 단어 |
| `INSERT INTO 테이블 (` 바로 뒤 | 첫 후보 = **그 테이블의 전체 컬럼**(한 조각) |
| 문장 시작 | 키워드 · 문서 심볼 |

- 키: ↑/↓ 고르기 · PgUp/PgDn/Home/End · 휠 · **Enter 또는 Tab** 확정 · Esc 닫기 · 계속 타이핑하면 그대로 걸러집니다 · Backspace로 접두가 없어지면 닫힙니다.
- 확정 뒤: 함수는 `NAME()` + 캐럿 안(`intel.insert_parens`) · 키워드 뒤 공백(`intel.insert_space` · 기본 끔) · FROM 뒤 테이블에 alias(`intel.insert_alias` · 기본 끔).
- 일치 규칙(설정 `intel.match`): 퍼지(기본 · `sc` → `SALES_CUSTOMER`) · 포함 · 접두. 한글은 조합 중 자모로도 걸립니다.
- 순서: 정확 > 접두 > 단어 경계(`_` 뒤) > 포함 > 약어. 같은 등급이면 **최근에 확정한 것**이 먼저입니다(20개). 컬럼 목록 조각은 늘 맨 위.
- 문자열·주석 안에서는 뜨지 않습니다. 큰 파일(L1 이상) · SQL 구문이 아닌 탭 · 이진 내용에서는 꺼집니다.

### 설정(Preferences ▸ Editors ▸ Code completion)

| 키 | 기본 | 뜻 |
|---|---|---|
| `intel.enabled` | on | 인텔리센스 전체(끄면 아웃라인·Goto Symbol도) |
| `intel.auto_activation` | on | 타이핑 중 자동으로 열기(Ctrl+Space는 늘 됨) |
| `intel.delay_ms` | 250 | 식별자를 친 뒤 팝업까지 지연 |
| `intel.trigger_chars` | `.` | 치는 즉시 여는 글자 |
| `intel.min_chars` | 2 | 자동 완성을 시작하는 식별자 길이 |
| `intel.match` | fuzzy | prefix · contains · fuzzy |
| `intel.recent_boost` | on | 최근 확정 우선 |
| `intel.max_items` | 200 | 랭킹 뒤 남기는 후보 수 |
| `intel.popup_rows` | 12 | 보이는 행 수(넘치면 스크롤) |
| `intel.keywords` | on | 키워드 후보 |
| `intel.document_words` | on | 문서 단어 후보 |
| `intel.insert_case` | default | 삽입 대소문자(default · upper · lower · match) |
| `intel.show_types` | on | 오른쪽 열(타입 · 종류 · 시그니처) |
| `intel.functions` | on | 내장 함수 · 시스템 패키지 · 사전 뷰(정적 표 · 서버 접속 0) |
| `intel.insert_parens` | on | 함수 확정 = `NAME()` + 캐럿 안 |
| `intel.insert_alias` | off | FROM 뒤 테이블 확정 = alias까지 |
| `intel.insert_space` | off | 키워드 확정 = 공백 하나 |
| `intel.insert_columns` | on | `INSERT INTO t (` 뒤 전체 컬럼 조각 |
| `intel.signature_help` | on | `(` 뒤 상태줄 시그니처 |
| `intel.budget_ms` | 30 | 후보 조립이 이보다 오래 걸리면 로그 창에 한 줄 |

> 메타(테이블·컬럼)는 왼쪽 객체 탐색기가 읽어 둔 것과 **같은 저장소**입니다 — 탐색기에서 스키마·테이블을 펼칠수록 후보가 풍부해지고, 접속을 끊으면 비워집니다. 완성이 서버에 따로 접속하지는 않습니다. 내장 함수·사전 뷰 표는 앱에 내장된 정적 표라 접속 없이도 뜹니다(빠진 함수는 문서 단어로 보완).

## 아웃라인

활동 막대의 **아웃라인** 아이콘(또는 View ▸ Outline)을 켜면 현재 탭의 심볼이 줄 번호와 함께 나열됩니다: 문장 머리(흐리게) · `DEFINE`/`VARIABLE` · CTE · `CREATE …` 대상 · PL/SQL `PROCEDURE`/`FUNCTION`(굵게 · 패키지 안이면 들여쓰기)/`PACKAGE [BODY]`/`TRIGGER`/`TYPE`/`CURSOR`/`<<라벨>>` · 선언부 변수 · T-SQL `DECLARE @v`. 클릭·Enter = 그 자리로 · 필터 틀은 다른 패널과 같습니다(한글 자모). 편집하면 잠시 뒤(유휴 때) 갱신되고 탭을 바꾸면 그 탭 것으로 바뀝니다.

**Goto Symbol — Ctrl+R(맥 ⌘R)**: 같은 목록을 팔레트에 띄워 이름을 치고 Enter로 바로 이동합니다(Sublime과 같은 키).

## 아직 없는 것(예정 · T-178 후속)

JOIN 조건 완성(FK 대상 정보가 메타에 들어온 뒤) · hover 카드(객체 정보 · T-179) · Ctrl+클릭 바로가기(T-180) · 후보 계산 워커(예산 초과가 실측되면) · 시그니처 도움을 상태줄 대신 캐럿 옆 카드로.
