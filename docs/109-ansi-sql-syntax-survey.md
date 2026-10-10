# 109 · ANSI/ISO SQL 문법 전수 — SQL:2016 Part 2 Foundation + SQL:2023 추가분 (사용자 10-10)

> **요구**(사용자 10-10): "Ansi SQL 문법에 대해서도 전체 Syntax에 대한 전수 Study를 별도로 하고 문서화 · 이 문서를 기준으로 기능 점검도 진행하고 자동 완성 정확도 개선에 쓸 것".
> **원천 규칙**: 표준 본문(ISO/IEC 9075-2)은 유료라 직접 인용하지 않는다. 공개 원천(ISO 공개 목차 · 표준 문법 공개 정리 · 각 DBMS 문서의 "표준 준수" 장 · Wikipedia SQL 문법 · 공개 BNF 정리)을 쓰고 출처 URL을 단다. 원천에서 확인하지 못한 것은 **"추정"**.
> **짝 문서**: DBMS별 사실 = [108](108-dbms-syntax-and-objects-survey.md) · 문법 참조 기반 완성 = [82](82-grammar-driven-completion.md)(`crates/nsql-script/grammar/ansi.sqlg`).

## §0. 원천 · 기호

> 표준 본문은 유료(ISO)라 공개 원천으로 정리했다 — PostgreSQL 부록 D(표준 기능 ID 표)·부록 C(키워드 표, SQL:2023/SQL:2016/SQL-92 열) · Ronald Savage의 SQL-2003 BNF · modern-sql.com · Wikipedia.
> 기능 ID 규칙: `E`/`F`(Core 포함 기본) · `T`(선택 기능) · `S`(객체 관계) · `B`(내장·모듈·PTF) · `R`(행 패턴) · `X`(SQL/XML, Part 14) · `M`(SQL/MED, Part 9). 표준이 정한 "Core" 묶음 = E011~E182, F021~F501 중 일부, S011, T321, T631.
> "추정" = 공개 원천에서 문구를 직접 확인하지 못한 항목. "2023" = SQL:2023에서 새로 들어온 기능.

공통 출처(아래 표에서 [PG-F] 등으로 줄여 씀):
- [PG-F] https://www.postgresql.org/docs/current/features-sql-standard.html (지원 기능 ID·이름)
- [PG-U] https://www.postgresql.org/docs/current/unsupported-features-sql-standard.html (미지원 기능 ID·이름)
- [PG-K] https://www.postgresql.org/docs/current/sql-keywords-appendix.html (키워드 표 · SQL:2023/SQL:2016 열)
- [PG-L] https://www.postgresql.org/docs/current/sql-syntax-lexical.html (어휘 · 표준과의 차이 주석)
- [BNF03] https://ronsavage.github.io/SQL/sql-2003-2.bnf.html (SQL:2003 Part 2 BNF — 2016 판 BNF 공개본은 확인 못 함)
- [W23] https://en.wikipedia.org/wiki/SQL:2023 · [W16] https://en.wikipedia.org/wiki/SQL:2016 · [WS] https://en.wikipedia.org/wiki/SQL_syntax
- [MS] https://modern-sql.com/ (기능별 해설 · 표준 판 표시)
- [ISO] https://www.iso.org/standard/76584.html (ISO/IEC 9075-2:2023 카탈로그)


## §1. 어휘 요소

| 요소 | 표준 꼴 | 비고 | 기능 ID |
|---|---|---|---|
| 토큰 | `<token>` = 비구분자 토큰(정규 식별자 · 키워드 · 숫자 리터럴 · 문자열 리터럴 일부) + 구분자 토큰(구분 식별자 · 연산자 · 구두점) | 두 비구분자 토큰 사이엔 `<separator>`(공백·주석·줄바꿈) 필요 | [BNF03] |
| 정규 식별자 | 글자로 시작 + 글자·숫자·`_` | 미인용 이름은 **대문자로 접힘**(case-insensitive). PG는 소문자로 접어 표준과 다름 | E031 · E031-02 · E031-03 |
| 구분 식별자 | `"…"` (안의 `"` = `""`) | 대소문자 보존 · 키워드도 이름으로 쓸 수 있음 | E031-01 |
| 유니코드 구분 식별자 | `U&"d\0061t\+000061"` [`UESCAPE '!'`] | `\XXXX` 4자리 · `\+XXXXXX` 6자리 | F392 |
| 식별자 길이 | Core = 18자 · 긴 이름 = 128자 | | F391 |
| 키워드 | 예약어 / 비예약어(§14) | 예약어는 인용해야 이름으로 씀 · 표준은 숫자 포함·`_`로 시작/끝나는 키워드를 정의하지 않음 | — |
| 문자열 리터럴 | `'abc'` (안의 `'` = `''`) · `_charset'…'`(문자 집합 지정) | 공백·줄바꿈으로 나눈 이웃 리터럴은 이어 붙음(`'ab' 'cd'`) | E021-03 · F271 |
| 국가 문자 리터럴 | `N'…'` | | F421 |
| 유니코드 문자열 | `U&'d\0061t'` [`UESCAPE '!'`] | | F393 |
| 이진 리터럴 | `X'4142'` | 공백 허용·이어 붙음 | T021 · T023 · T024 |
| 정확 수 | `123` · `12.5` · `.5` | | E011 |
| 근사 수 | `1.5E10` | | E011-02 |
| 비10진 정수 | `0x1F` · `0o17` · `0b1010` | 2023 (추정: T661이 2023 신규인지 공개 원천마다 표기 차) | T661 |
| 숫자 밑줄 | `1_000_000` | 2023 | T662 |
| 날짜·시간 | `DATE '2026-10-10'` · `TIME '12:34:56.789'` · `TIMESTAMP '2026-10-10 12:34:56'` · `TIME '…+09:00'`(WITH TIME ZONE) | 글자 부분 형식은 고정 | F051-01~03 · F411 |
| 간격 | `INTERVAL '3' DAY` · `INTERVAL '1-2' YEAR TO MONTH` · `INTERVAL -'5' MINUTE` | 한정자 필수(§2) | F052 |
| 불리언 | `TRUE` · `FALSE` · `UNKNOWN` | `UNKNOWN` = 불리언 NULL | T031 |
| NULL | `NULL` | 문맥 타입 · `CAST(NULL AS t)` | E131 |
| 주석 | `-- 줄 끝까지` · `/* … */` | **괄호 주석은 중첩된다**(표준 · C와 다름) | E161 · T351 |
| 호스트 매개변수 | `:name` [`INDICATOR`] `:ind` | 임베디드 SQL·모듈 | B0xx · E182 |
| 동적 매개변수 | `?` | PREPARE된 문장의 자리표 | B031 |
| SQL 매개변수 참조 | `routine.param` | 루틴 안 | T325 |
| 구분·연산 기호 | `( ) , . ; : + - * / || = <> < > <= >= ? [ ] { } -> =>` (`=>` 이름 인자 · `->`/`[]`는 참조·배열) | 2023: JSON 단순 접근자 `j.a.b`·`j[0]` | T524 · T860~T864 |

출처: [PG-F] [PG-L] [BNF03] [W23] https://modern-sql.com/feature/literals (추정 — 주소 미확인)

## §2. 자료형

| 분류 | 형 | 표준 꼴·비고 | 판 | 기능 ID |
|---|---|---|---|---|
| 문자 | `CHARACTER(n)`/`CHAR` · `CHARACTER VARYING(n)`/`VARCHAR(n)` · `CHARACTER LARGE OBJECT`/`CLOB(n[K|M|G|T|P])` | `CHARACTER SET cs` · `COLLATE c` · 길이 단위 `CHARACTERS`/`OCTETS` | 92/99 · T/P 승수 2016 | E021 · T046 · T062 · T043 · T044 |
| VARCHAR 길이 생략 | `VARCHAR` (최대 길이 없이) | 2023 | 2023 | T081 |
| 국가 문자 | `NATIONAL CHARACTER`/`NCHAR` · `NCHAR VARYING` · `NCLOB`(`NATIONAL CHARACTER LARGE OBJECT`) | | 92/99 | F421 |
| 이진 | `BINARY(n)` · `BINARY VARYING`/`VARBINARY(n)` · `BINARY LARGE OBJECT`/`BLOB(n)` | | BINARY/VARBINARY 2008 · BLOB 99 | T021 · T022 · T045 |
| 정확 수 | `NUMERIC(p,s)` · `DECIMAL`/`DEC(p,s)` · `SMALLINT` · `INTEGER`/`INT` · `BIGINT` | BIGINT 2003 | | E011-01/03 · T071 |
| 10진 부동 | `DECFLOAT(p)` | IEEE 754 10진 | 2016 | T076 |
| 근사 수 | `FLOAT(p)` · `REAL` · `DOUBLE PRECISION` | | | E011-02 |
| 불리언 | `BOOLEAN` | `TRUE/FALSE/UNKNOWN` | 99 | T031 |
| 날짜·시간 | `DATE` · `TIME(p) [WITH|WITHOUT TIME ZONE]` · `TIMESTAMP(p) [WITH|WITHOUT TIME ZONE]` | 기본 WITHOUT · TIMESTAMP 기본 정밀도 6 · TIME 0 | 92 | F051 · F411 · F555 |
| 간격 | `INTERVAL <qualifier>` | 연-월 계열: `YEAR` · `MONTH` · `YEAR TO MONTH` / 일-시 계열: `DAY` · `HOUR` · `MINUTE` · `SECOND(p)` 와 `DAY TO HOUR/MINUTE/SECOND` · `HOUR TO MINUTE/SECOND` · `MINUTE TO SECOND` · 선행 필드 정밀도 `DAY(3)` | 92 | F052 |
| XML | `XML[(DOCUMENT|CONTENT|SEQUENCE [(ANY|UNTYPED|XMLSCHEMA …)])]` | Part 14 SQL/XML | 2003+ | X010 · X181~X192 · X231 |
| JSON | `JSON` | 2016은 문자열 기반 JSON(T803)뿐 · 2023에 고유 형 | 2023 | T801 · T802 · T803 |
| 행 | `ROW(f1 t1, f2 t2, …)` | | 99 | T051 |
| 배열 | `t ARRAY[n]` · `t ARRAY` | 최대 원소 수 | 99 | S090 · S091 · S096 |
| 멀티셋 | `t MULTISET` | 순서 없는 중복 허용 모음 | 2003 | S271 · S275 |
| 참조 | `REF(udt) [SCOPE tbl]` | 구조 형 행 참조 | 99 | S041 · S043 |
| 사용자 정의 – 구별 형 | `CREATE TYPE t AS INTEGER FINAL` | 원본 형과 강타입 구별 · 배열/멀티셋 기반 2016 | 99 | S011 · S401 · S402 |
| 사용자 정의 – 구조 형 | `CREATE TYPE t AS (a INT, …) [UNDER super] [NOT] INSTANTIABLE [NOT] FINAL` + 메서드 | 상속·메서드 | 99 | S023~S028 |
| 도메인 | `CREATE DOMAIN d AS t [DEFAULT …] [CHECK (…)]` | 형 아님(제약 붙은 별칭) | 92 | F251 · F711 |
| 로케이터 | 호스트 쪽 LOB·UDT·배열 핸들 | 임베디드 전용 | 99 | T039 · T041 · S231~S233 |
| DATALINK | `DATALINK` | Part 9 SQL/MED | 2003 | M001 |

출처: [PG-F] [PG-U] [BNF03] [W16] [W23] https://www.postgresql.org/docs/current/datatype.html

## §3. 값 식

| 식 | 표준 꼴 | 기능 ID |
|---|---|---|
| CAST | `CAST(x AS t)` · 2016: `CAST(x AS t FORMAT '…')`(날짜·문자열 서식 변환) | F201 · T839 |
| 단순 CASE | `CASE x WHEN v1 THEN r1 [WHEN …] [ELSE r] END` · 2003+: `WHEN v1, v2`(콤마 나열)·`WHEN < 5`(술어 조각) | F261-01 · F262 · F263 |
| 검색 CASE | `CASE WHEN cond THEN r … [ELSE r] END` | F261-02 |
| NULLIF / COALESCE | `NULLIF(a,b)` · `COALESCE(a,b,…)` (CASE 약식) | F261-03 · F261-04 |
| GREATEST / LEAST | `GREATEST(a,b,…)` · `LEAST(…)` (2023) | T054 |
| 행 값 생성자 | `(a, b, c)` · `ROW(a, b)` · 행 비교 `(a,b) < (c,d)` | F641 · T051 |
| 필드 참조 | `(row_expr).field` · `r.*`(별칭 지정 `AS (x,y)` T053) | T051 · T053 |
| 스칼라 부질의 | `(SELECT …)` 1행 1열 | F471 |
| 배열 생성자 | `ARRAY[1,2,3]` · `ARRAY(SELECT …)` · 원소 `a[i]` · 연결 `a || b` | S091 · S095 · S099 |
| 배열 함수 | `CARDINALITY(a)` · `ARRAY_MAX_CARDINALITY(a)` · `TRIM_ARRAY(a, n)` | S091 · S403 · S404 |
| 멀티셋 생성자 | `MULTISET[1,2]` · `MULTISET(SELECT …)` · `TABLE(multiset)` | S271 |
| 멀티셋 연산 | `m1 MULTISET UNION [ALL|DISTINCT] m2` · `MULTISET EXCEPT` · `MULTISET INTERSECT` · `SET(m)` · `ELEMENT(m)` · `CARDINALITY(m)` | S271 · S275 |
| 수 식 | 단항 `+ -` · `* /` · `+ -` | E011-04 |
| 문자열 식 | `a || b` (CHAR·VARCHAR·CLOB·BINARY·BLOB) · `COLLATE c` | E021-07 · T040 · T048 |
| 날짜·시간 식 | `dt + interval` · `dt - interval` · `dt AT TIME ZONE tz` · `dt AT LOCAL` | F052 · F411 |
| 간격 식 | `(dt1 - dt2) DAY TO SECOND` · `interval * n` · `interval / n` · `ABS(interval)` | F052 |
| 불리언 식 | `NOT` · `AND` · `OR` · `x IS [NOT] TRUE|FALSE|UNKNOWN` | F571 |
| 서브타입 처리 | `TREAT(x AS subtype)` · 참조 `DEREF(r)` · `r -> attr` | S161 · S162 · S043 |
| 메서드 호출 | `x.method(…)` · `NEW t(…)` (생성자) | S023 |
| 다음 시퀀스 값 | `NEXT VALUE FOR seq` | T176 |
| 기본값 | `DEFAULT` (INSERT/UPDATE 값 자리) | F221 |
| JSON 단순 접근자 | `j.key` · `j."Key"` · `j[0]` · `j.*` (2023) | T860~T864 |

**연산자 우선순위**(높음 → 낮음, 표준 BNF 구조에서 도출 — 표로 명시한 원문은 없음 · 추정 정리):

| 순위 | 연산 |
|---|---|
| 1 | `.` 필드·메서드 · `[ ]` 원소 · `::`(표준 아님) |
| 2 | 단항 `+ -` |
| 3 | `* /` |
| 4 | `+ -` · `||` (표준 BNF에서 `||`는 문자 값 식 층으로 수 식과 섞이지 않음) |
| 5 | 술어(비교 `= <> < > <= >=` · `BETWEEN` · `IN` · `LIKE` · `IS …`) |
| 6 | `IS [NOT] TRUE/FALSE/UNKNOWN` |
| 7 | `NOT` |
| 8 | `AND` |
| 9 | `OR` |

출처: [PG-F] [PG-U] [BNF03] https://www.postgresql.org/docs/current/sql-syntax-lexical.html#SQL-PRECEDENCE https://modern-sql.com/feature/case

## §4. 술어 전수

| 술어 | 표준 꼴 | 판 | 기능 ID |
|---|---|---|---|
| 비교 | `a {= | <> | < | > | <= | >=} b` · 행 비교 | 86 | E061-01 |
| BETWEEN | `a [NOT] BETWEEN [ASYMMETRIC | SYMMETRIC] x AND y` | SYMMETRIC 99 | E061-02 · T461 |
| IN | `a [NOT] IN (v1, …)` · `a IN (SELECT …)` | | E061-03 · E061-11 · T631 |
| LIKE | `a [NOT] LIKE p [ESCAPE e]` (BLOB·CLOB 확장) | | E061-04/05 · F281 |
| SIMILAR TO | `a [NOT] SIMILAR TO p [ESCAPE e]` (SQL 정규식) | 99 | T141 |
| LIKE_REGEX | `a [NOT] LIKE_REGEX p [FLAG f]` (XQuery 정규식) | 2008 | F841 |
| NULL | `a IS [NOT] NULL` (행 값: 모든 필드 기준) | | E061-06 · F481 |
| 한정 비교 | `a op {ANY | SOME | ALL} (SELECT …)` | | E061-07 · E061-12 |
| EXISTS | `EXISTS (SELECT …)` | | E061-08 · T501 |
| UNIQUE | `UNIQUE (SELECT …)` · 2023 NULL 처리 | | F291 · F292 |
| MATCH | `r MATCH [UNIQUE] [SIMPLE | PARTIAL | FULL] (SELECT …)` | 92 | F741(추정 — 술어 자체 ID는 별도일 수 있음) |
| OVERLAPS | `(s1, e1) OVERLAPS (s2, e2)` | 92 | F053 |
| DISTINCT | `a IS [NOT] DISTINCT FROM b` | 99 | T151 · T152 |
| MEMBER | `x [NOT] MEMBER [OF] m` | 2003 | S271 |
| SUBMULTISET | `m1 [NOT] SUBMULTISET [OF] m2` | 2003 | S271 |
| SET | `m IS [NOT] A SET` | 2003 | S271 |
| 형 | `x IS [NOT] OF (t1, ONLY t2)` | 99 | S151 |
| 정규형 | `s IS [NOT] [NFC|NFD|NFKC|NFKD] NORMALIZED` | 2003 | F394(추정 — 정규화 함수 기능일 수 있음) |
| JSON | `x IS [NOT] JSON [VALUE | ARRAY | OBJECT | SCALAR] [WITH | WITHOUT UNIQUE [KEYS]]` | 2016 | T821 · T822 |
| JSON_EXISTS | `JSON_EXISTS(j, 'path' [PASSING …] [ON ERROR])` (술어처럼 씀) | 2016 | T821 |
| 진리값 | `x IS [NOT] {TRUE | FALSE | UNKNOWN}` | 99 | F571 |
| XML | `x IS [NOT] DOCUMENT` · `IS [NOT] CONTENT` · `XMLEXISTS(…)` · `IS [NOT] VALID …` | 2003+ | X090 · X091 · X096 · X141~ |
| 기간 술어 | `p1 CONTAINS p2|t` · `p1 OVERLAPS p2` · `p1 EQUALS p2` · `p1 PRECEDES p2` · `p1 SUCCEEDS p2` · `p1 IMMEDIATELY PRECEDES p2` · `p1 IMMEDIATELY SUCCEEDS p2` (`p` = `PERIOD name` 또는 `PERIOD (s, e)`) | 2011 | T502 |

출처: [PG-F] [PG-U] [BNF03] https://modern-sql.com/feature/is-distinct-from https://modern-sql.com/blog/2019-02/periods (추정 — 주소 미확인)

## §5. 질의식

**질의 명세 절 순서**(쓰는 순서): `WITH` → `SELECT [ALL|DISTINCT] 목록` → `FROM` → `WHERE` → `GROUP BY` → `HAVING` → `WINDOW` → (집합 연산) → `ORDER BY` → `OFFSET` → `FETCH`. 논리 평가 순서 = FROM → WHERE → GROUP BY → HAVING → WINDOW/윈도 함수 → SELECT → DISTINCT → 집합 연산 → ORDER BY → OFFSET/FETCH.

| 구성 | 표준 꼴 | 판 | 기능 ID |
|---|---|---|---|
| 별칭 | `t [AS] a (c1, c2)` (열 이름 바꿈) | | E051-08/09 · T285 |
| 내부 조인 | `a [INNER] JOIN b ON …` | 92 | F041-01/02 |
| 외부 조인 | `LEFT|RIGHT [OUTER] JOIN` · `FULL [OUTER] JOIN` | 92 | F041-03/04 · F406 |
| 교차 | `CROSS JOIN` · 콤마 | | F407 |
| 자연·USING | `NATURAL [INNER|LEFT|…] JOIN` · `JOIN … USING (c) [AS alias]` | 92 · alias 2016 | F401 · F405 · F404 |
| 분할 외부 조인 | `a PARTITION BY (c) RIGHT JOIN b` | 2003 | F403 |
| 파생 테이블 | `(SELECT …) [AS] a` | | F591 |
| LATERAL | `LATERAL (SELECT …)` | 99 | T491 |
| 표본 | `t TABLESAMPLE {BERNOULLI | SYSTEM} (pct) [REPEATABLE (seed)]` | 2003 | T613 |
| UNNEST | `UNNEST(arr [, arr2]) [WITH ORDINALITY] AS u(x, n)` | 99/2003 | S091 · S301 |
| 테이블 함수 | `TABLE(f(…))` · `f(…)` 파생 · 다형 테이블 함수 `f(TABLE t PARTITION BY … ORDER BY …, DESCRIPTOR(c), COPARTITION …)` | PTF 2016 | T326 · B200~B209 |
| ONLY | `ONLY (t)` (하위 테이블 제외) | 99 | S111 |
| JSON_TABLE / XMLTABLE | `JSON_TABLE(j, 'path' COLUMNS (…) [PLAN …])` · `XMLTABLE(…)` | 2016 / 2006 | T821 · T824 · T827 · T838 · X300 |
| MATCH_RECOGNIZE | `t MATCH_RECOGNIZE (PARTITION BY … ORDER BY … MEASURES … ONE ROW|ALL ROWS PER MATCH AFTER MATCH SKIP … PATTERN (…) DEFINE …)` | 2016 | R010 · R020(WINDOW 절 안) · R030 |
| 시스템 시간 | `t FOR SYSTEM_TIME {AS OF x | BETWEEN [SYMMETRIC] x AND y | FROM x TO y}` | 2011 | T180 |
| GROUP BY | `GROUP BY [ALL|DISTINCT] c, ROLLUP(…), CUBE(…), GROUPING SETS ((…),(…)), ()` · `GROUPING(c1, c2)` | 99 · DISTINCT 2016 | E051-02 · T431 · T432 · T433 · T434 |
| HAVING | `HAVING cond` | | E051-06 |
| WINDOW | `WINDOW w AS (PARTITION BY … ORDER BY … frame)` | 2003 | T611 · T612 |
| ORDER BY | `ORDER BY expr [ASC|DESC] [NULLS FIRST|LAST]` · 부질의·뷰 안 ORDER BY 2008 · 그룹 테이블 2023 | | E121-02/03 · T611 · F850~F855 · F868 |
| OFFSET / FETCH | `OFFSET n {ROW|ROWS} FETCH {FIRST|NEXT} [n [PERCENT]] {ROW|ROWS} {ONLY | WITH TIES}` | 2008 · PERCENT·WITH TIES 2011 | F856~F867 |
| 집합 연산 | `q1 {UNION | EXCEPT | INTERSECT} [ALL | DISTINCT] [CORRESPONDING [BY (c)]] q2` · 우선순위 INTERSECT > UNION=EXCEPT | | E071 · F302~F305 · F301 |
| WITH | `WITH [RECURSIVE] cte (c) AS (q), …` | 99 | T121 · T122 · T131 · T132 |
| SEARCH / CYCLE | `SEARCH {DEPTH|BREADTH} FIRST BY c SET ord` · `CYCLE c SET mark [TO v DEFAULT d] USING path` (2023: TO/DEFAULT 생략·임의 값) | 99 · 2023 | T131 · T133 |
| VALUES | `VALUES (1,'a'), (2,'b')` (테이블 값 생성자) | 92 | F641 |
| TABLE | `TABLE t` (= `SELECT * FROM t`) | 92 | F661 |
| 자료 변경 결과 | `FROM FINAL|NEW|OLD TABLE (INSERT …)` | 2003 | T495 |
| 커서 갱신 | `DECLARE c CURSOR FOR q FOR {READ ONLY | UPDATE [OF c1]}` | | E121 · F831 |

출처: [PG-F] [PG-U] [BNF03] [W16] https://modern-sql.com/feature/match_recognize https://modern-sql.com/feature/with https://modern-sql.com/feature/fetch-first https://modern-sql.com/feature/lateral

## §6. 윈도·집계 함수 전수

| 분류 | 함수 | 판 | 기능 ID |
|---|---|---|---|
| 기본 집계 | `COUNT(*)` · `COUNT([ALL|DISTINCT] x)` · `SUM` · `AVG` · `MAX` · `MIN` | 86 | E091 |
| 불리언 집계 | `EVERY(b)` · `ANY(b)` · `SOME(b)` | 99 | T031(추정) |
| 통계 | `STDDEV_POP` · `STDDEV_SAMP` · `VAR_POP` · `VAR_SAMP` | 2003 | T621 |
| 회귀·상관 | `COVAR_POP` · `COVAR_SAMP` · `CORR` · `REGR_SLOPE` · `REGR_INTERCEPT` · `REGR_COUNT` · `REGR_R2` · `REGR_AVGX` · `REGR_AVGY` · `REGR_SXX` · `REGR_SYY` · `REGR_SXY` | 2003 | T621 |
| 멀티셋 집계 | `COLLECT(x)` · `FUSION(m)` · `INTERSECTION(m)` | 2003 | S271 · S275 |
| 배열 집계 | `ARRAY_AGG(x [ORDER BY …])` | 2008 | S098 |
| 역분포 | `PERCENTILE_CONT(p) WITHIN GROUP (ORDER BY x)` · `PERCENTILE_DISC(p) WITHIN GROUP (…)` | 2003 | T612 |
| 가설 집합 | `RANK(v) WITHIN GROUP (ORDER BY x)` · `DENSE_RANK(…)` · `PERCENT_RANK(…)` · `CUME_DIST(…)` | 2003 | T612 |
| 문자열 집계 | `LISTAGG([DISTINCT] x [, sep] [ON OVERFLOW {ERROR | TRUNCATE ['…'] {WITH|WITHOUT} COUNT}]) WITHIN GROUP (ORDER BY …)` | 2016 | T625 |
| JSON 집계 | `JSON_ARRAYAGG(x [ORDER BY …] [NULL|ABSENT ON NULL] [RETURNING t])` · `JSON_OBJECTAGG(k VALUE v [… ON NULL] [WITH|WITHOUT UNIQUE KEYS])` | 2016 | T811 · T812 · T813 · T814 |
| XML 집계 | `XMLAGG(x [ORDER BY …])` | 2003 | X034 · X035 |
| 임의 값 | `ANY_VALUE(x)` | 2023 | T626 |
| GROUPING | `GROUPING(c, …)` | 99 | T431 · T433 |
| FILTER 절 | `agg(…) FILTER (WHERE cond)` | 2003 | T612 |
| 순위 윈도 | `ROW_NUMBER()` · `RANK()` · `DENSE_RANK()` · `PERCENT_RANK()` · `CUME_DIST()` | 2003 | T611 · T612 |
| 분할 | `NTILE(n)` | 2008 | T614 |
| 오프셋 | `LEAD(x [, off [, def]]) [RESPECT|IGNORE NULLS]` · `LAG(…)` | 2011 | T615 · T616 |
| 값 | `FIRST_VALUE(x) [RESPECT|IGNORE NULLS]` · `LAST_VALUE(x) …` · `NTH_VALUE(x, n) [FROM FIRST|FROM LAST] [RESPECT|IGNORE NULLS]` | 2011 | T617 · T618 |
| 중첩 윈도 | `VALUE_OF(x AT ROW_MARKER …)` 등 (윈도 함수 안 윈도 값 참조) | 2016(추정) | T619 |
| 프레임 COUNT DISTINCT | `COUNT(DISTINCT x) OVER (… frame)` | 2023(추정) | T627 |
| 윈도 명세 | `OVER (w | [w_base] PARTITION BY … ORDER BY … frame)` | 2003 | T611 |
| 프레임 | `{ROWS | RANGE | GROUPS} {start | BETWEEN start AND end} [EXCLUDE {CURRENT ROW | GROUP | TIES | NO OTHERS}]` · 경계 `UNBOUNDED PRECEDING` · `n PRECEDING` · `CURRENT ROW` · `n FOLLOWING` · `UNBOUNDED FOLLOWING` | ROWS/RANGE 2003 · GROUPS 2011 | T611 · T612 · T620 |

출처: [PG-F] [PG-U] https://modern-sql.com/feature/filter https://modern-sql.com/feature/listagg https://modern-sql.com/blog/2019-02/postgresql-11#over (추정 — 주소 미확인) https://www.postgresql.org/docs/current/functions-aggregate.html https://www.postgresql.org/docs/current/functions-window.html

## §7. DML

| 문 | 표준 꼴 | 판 | 기능 ID |
|---|---|---|---|
| INSERT | `INSERT INTO t [(c1, …)] [OVERRIDING {SYSTEM | USER} VALUE] {VALUES (…), … | query | DEFAULT VALUES}` | OVERRIDING 2003 | E101-01 · F222 · T174 |
| UPDATE(검색) | `UPDATE t [[AS] a] SET c = v, (c1, c2) = (v1, v2) | ROW(…) [WHERE cond]` | 다중 열 대입 2003 | E101-03 · T641 |
| UPDATE(위치) | `UPDATE t SET … WHERE CURRENT OF cursor` | | E121-06 |
| DELETE(검색) | `DELETE FROM t [[AS] a] [WHERE cond]` | | E101-04 |
| DELETE(위치) | `DELETE FROM t WHERE CURRENT OF cursor` | | E121-07 |
| MERGE | `MERGE INTO t [AS] a USING src ON cond WHEN MATCHED [AND c] THEN {UPDATE SET … | DELETE} WHEN NOT MATCHED [AND c] THEN INSERT [(…)] [OVERRIDING …] VALUES (…)` | 2003 · AND·DELETE 2008 | F312 · F313 · F314 |
| TRUNCATE | `TRUNCATE TABLE t [{CONTINUE | RESTART} IDENTITY]` | 2008 | F200 · F202 |
| 기간 갱신 | `UPDATE t FOR PORTION OF p FROM x TO y SET …` · `DELETE FROM t FOR PORTION OF p FROM x TO y` | 2011 | T181 |
| 단일 행 SELECT | `SELECT … INTO :v1, :v2 FROM …` (임베디드·루틴) | | E111 |
| CALL | `CALL proc(a, name => b)` | 99 · 이름 인자 2011 | T321-04 · T521 · T522 |
| 자료 변경 + 조회 | `SELECT … FROM FINAL TABLE (INSERT …)` | 2003 | T495 |

출처: [PG-F] [PG-U] https://modern-sql.com/caniuse/merge (추정 — 주소 미확인) https://www.postgresql.org/docs/current/sql-merge.html (Compatibility 절)

## §8. DDL

| 대상 | 표준 꼴(요점) | 판 | 기능 ID |
|---|---|---|---|
| 스키마 | `CREATE SCHEMA s [AUTHORIZATION u] [DEFAULT CHARACTER SET cs] [PATH …] [스키마 요소 …]` · `DROP SCHEMA s {CASCADE|RESTRICT}` | 92 | F311-01 · F171 · F381 |
| 테이블 | `CREATE [{GLOBAL|LOCAL} TEMPORARY] TABLE t (요소, …) [ON COMMIT {PRESERVE|DELETE} ROWS] [WITH SYSTEM VERSIONING]` | | F031-01 · F531 |
| 열 정의 | `c t [DEFAULT v] [GENERATED {ALWAYS | BY DEFAULT} AS IDENTITY [(START WITH n INCREMENT BY n …)]] [GENERATED ALWAYS AS (expr)] [열 제약] [COLLATE c]` | IDENTITY·생성 열 2003 | E141-07 · T174 · T175 |
| 열 제약 | `NOT NULL` · `UNIQUE` · `PRIMARY KEY` · `CHECK (…)` · `REFERENCES t (c)` | | E141 |
| 테이블 제약 | `[CONSTRAINT n] {PRIMARY KEY (…) | UNIQUE [NULLS [NOT] DISTINCT] (…) | UNIQUE (VALUE) | FOREIGN KEY (…) REFERENCES t (…) [MATCH {SIMPLE|PARTIAL|FULL}] [ON DELETE a] [ON UPDATE a] | CHECK (…)}` · 동작 a = `NO ACTION | RESTRICT | CASCADE | SET NULL | SET DEFAULT` | NULLS DISTINCT 2023 | E141-03/04/06 · F191 · F701 · F741 · T191 · F292 · S291 |
| 지연 | `[NOT] DEFERRABLE [INITIALLY {DEFERRED|IMMEDIATE}]` · `[NOT] ENFORCED` | ENFORCED 2011 | F721 · F492 |
| LIKE | `CREATE TABLE t (LIKE s [INCLUDING|EXCLUDING {IDENTITY|DEFAULTS|GENERATED}])` | 2003 | T171 · T173 |
| AS 질의 | `CREATE TABLE t [(c, …)] AS (q) WITH [NO] DATA` | 2003 | T172 |
| 형 테이블 | `CREATE TABLE t OF udt [UNDER super]` | 99 | S051 · S081 |
| 시스템 기간 | `sys_start TIMESTAMP GENERATED ALWAYS AS ROW START, sys_end … AS ROW END, PERIOD FOR SYSTEM_TIME (sys_start, sys_end)` + `WITH SYSTEM VERSIONING` | 2011 | T180 |
| 응용 기간 | `PERIOD FOR p (s, e)` · `PRIMARY KEY (id, p WITHOUT OVERLAPS)` · `FOREIGN KEY (id, PERIOD p) REFERENCES t (id, PERIOD q)` | 2011 | T181 |
| ALTER TABLE | `ADD [COLUMN] def` · `ALTER [COLUMN] c {SET DEFAULT v | DROP DEFAULT | SET DATA TYPE t | SET NOT NULL | DROP NOT NULL | ADD|SET|DROP IDENTITY … | DROP EXPRESSION | SET SCOPE …}` · `DROP [COLUMN] c {CASCADE|RESTRICT}` · `ADD CONSTRAINT …` · `ALTER CONSTRAINT n [NOT] ENFORCED` · `DROP CONSTRAINT n {CASCADE|RESTRICT}` · `ADD|DROP SYSTEM VERSIONING` · `ADD|DROP PERIOD` | | F031-04 · F033 · F381~F388 |
| DROP | `DROP {TABLE|VIEW|DOMAIN|SEQUENCE|TYPE|SCHEMA|ROLE|ASSERTION|…} n {CASCADE | RESTRICT}` (표준에서 CASCADE/RESTRICT는 필수어 · 생략 허용은 T551 계열 아님 — 추정) | | F031-13/16 · F032 |
| 뷰 | `CREATE [RECURSIVE] VIEW v [(c, …)] AS q [WITH [CASCADED|LOCAL] CHECK OPTION]` | RECURSIVE 99 | F031-02 · F311-03/04 · F751 · T131 |
| 도메인 | `CREATE DOMAIN d [AS] t [DEFAULT v] [CONSTRAINT n CHECK (VALUE …)]` · `ALTER DOMAIN` | 92 | F251 · F711 |
| 시퀀스 | `CREATE SEQUENCE s [AS t] [START WITH n] [INCREMENT BY n] [MAXVALUE n|NO MAXVALUE] [MINVALUE …] [[NO] CYCLE]` · `ALTER SEQUENCE s RESTART [WITH n]` | 2003 | T176 · T177 |
| 형 | `CREATE TYPE t AS 원본형 FINAL`(구별) · `CREATE TYPE t [UNDER s] AS (속성 …) [INSTANTIABLE] [NOT FINAL] [REF IS SYSTEM GENERATED] [메서드 명세]` · `CREATE METHOD` · `ALTER TYPE` · `DROP TYPE` | 99 | S011 · S023~S028 |
| 루틴 | `CREATE {PROCEDURE p | FUNCTION f} ([IN|OUT|INOUT] a t [DEFAULT v], …) [RETURNS t | RETURNS TABLE (c t, …)] [LANGUAGE SQL|C|…] [[NOT] DETERMINISTIC] [{NO SQL | CONTAINS SQL | READS SQL DATA | MODIFIES SQL DATA}] [{RETURNS NULL | CALLED} ON NULL INPUT] [SQL SECURITY {DEFINER|INVOKER}] [DYNAMIC RESULT SETS n] 본문` · 본문 = `RETURN expr` 또는 `BEGIN ATOMIC … END`(PSM 복합문) 또는 `EXTERNAL NAME '…'` | 99 · 기본값 2011 | T321 · T323 · T324 · T326 · T341 · T471 · T522~T525 · B128 |
| 트리거 | `CREATE TRIGGER n {BEFORE | AFTER | INSTEAD OF} {INSERT | DELETE | UPDATE [OF c, …]} ON t [REFERENCING {OLD|NEW} [ROW] [AS] o | {OLD|NEW} TABLE [AS] ot] [FOR EACH {ROW | STATEMENT}] [WHEN (cond)] 문` | 99 · INSTEAD OF 2008 | T200 · T211~T218 |
| 단언 | `CREATE ASSERTION n CHECK (cond) [지연 특성]` | 92 | F521 |
| 문자 집합 | `CREATE CHARACTER SET cs [AS] GET src [COLLATE c]` | 92 | F451 |
| 대조 | `CREATE COLLATION c FOR cs FROM src [{NO PAD | PAD SPACE}]` | 92 | F690 · F692 |
| 번역 | `CREATE TRANSLATION tr FOR cs1 TO cs2 FROM …` | 92 | F695 |
| 변환 | `CREATE TRANSFORM FOR udt grp (TO SQL WITH f, FROM SQL WITH g)` · `ALTER TRANSFORM` | 99 | S241 · S242 |
| 사용자 정의 순서·캐스트 | `CREATE ORDERING FOR udt {EQUALS ONLY | ORDER FULL} BY …` · `CREATE CAST (s AS t) WITH f [AS ASSIGNMENT]` | 99 | S251 · S211 |
| 역할 | `CREATE ROLE r [WITH ADMIN u]` · `DROP ROLE r` | 99 | T331 · T332 |
| (표준 아님) | `CREATE INDEX`는 **표준에 없다**(구현 정의) | — | — |

출처: [PG-F] [PG-U] [BNF03] https://www.postgresql.org/docs/current/sql-createtable.html (Compatibility 절) https://www.postgresql.org/docs/current/sql-createtrigger.html (Compatibility 절) https://modern-sql.com/blog/2019-02/system-versioned-tables (추정 — 주소 미확인)

## §9. 접근 제어

| 문 | 표준 꼴 | 기능 ID |
|---|---|---|
| 권한 부여 | `GRANT {ALL PRIVILEGES | 동작, …} ON [TABLE] t | DOMAIN d | COLLATION c | CHARACTER SET cs | TRANSLATION tr | TYPE t | SEQUENCE s | {SPECIFIC} ROUTINE|FUNCTION|PROCEDURE|METHOD r TO {u | PUBLIC}, … [WITH HIERARCHY OPTION] [WITH GRANT OPTION] [GRANTED BY {CURRENT_USER | CURRENT_ROLE}]` | F031-03 · E081 |
| 동작(권한) | `SELECT [(c, …)]` · `SELECT (method)` · `INSERT [(c)]` · `UPDATE [(c)]` · `DELETE` · `REFERENCES [(c)]` · `USAGE` · `TRIGGER` · `UNDER` · `EXECUTE` | E081-01~10 · T217 · T281 · F731 |
| 역할 부여 | `GRANT r1, … TO u, … [WITH ADMIN OPTION] [GRANTED BY …]` | T331 · T332 |
| 회수 | `REVOKE [GRANT OPTION FOR | HIERARCHY OPTION FOR] 권한 ON 객체 FROM u [GRANTED BY …] {CASCADE | RESTRICT}` | F031-19 · F034~F038 |
| 역할 회수 | `REVOKE [ADMIN OPTION FOR] r FROM u {CASCADE | RESTRICT}` | T331 |
| 세션 역할 | `SET ROLE {r | NONE}` (§11) | T331 |
| 실행 권한 의미 | `SQL SECURITY DEFINER | INVOKER` (루틴) | T323 · T324 |

출처: [PG-F] [PG-U] https://www.postgresql.org/docs/current/sql-grant.html (Compatibility 절) https://www.postgresql.org/docs/current/sql-revoke.html

## §10. 트랜잭션

| 문 | 표준 꼴 | 기능 ID |
|---|---|---|
| 시작 | `START TRANSACTION [모드, …]` (표준은 첫 문장이 암묵 시작도 허용) | T241 |
| 다음 트랜잭션 특성 | `SET [LOCAL] TRANSACTION 모드, …` | E152 · T251 |
| 모드 | `ISOLATION LEVEL {READ UNCOMMITTED | READ COMMITTED | REPEATABLE READ | SERIALIZABLE}` · `READ ONLY | READ WRITE` · `DIAGNOSTICS SIZE n` | E152-01/02 · F111~F114 · F124 |
| 기본 격리 | 표준 기본 = `SERIALIZABLE` | E152 |
| 커밋 | `COMMIT [WORK] [AND [NO] CHAIN]` | E151-01 · T261 |
| 롤백 | `ROLLBACK [WORK] [AND [NO] CHAIN]` · `ROLLBACK [WORK] TO SAVEPOINT s` | E151-02 · T261 · T271 |
| 세이브포인트 | `SAVEPOINT s` · `RELEASE SAVEPOINT s` | T271 · T272 |
| 제약 시점 | `SET CONSTRAINTS {ALL | n, …} {DEFERRED | IMMEDIATE}` | F721 |
| 다중 서버 | 여러 연결에 걸친 트랜잭션 | T262 |

출처: [PG-F] [PG-U] https://www.postgresql.org/docs/current/sql-start-transaction.html https://www.postgresql.org/docs/current/sql-set-transaction.html (Compatibility 절)

## §11. 세션·연결

| 문 | 표준 꼴 | 기능 ID |
|---|---|---|
| 연결 | `CONNECT TO {서버 [AS 연결명] [USER u] | DEFAULT}` | F771 |
| 연결 전환 | `SET CONNECTION {연결명 | DEFAULT}` | F771 |
| 연결 해제 | `DISCONNECT {연결명 | DEFAULT | CURRENT | ALL}` | F771 |
| 스키마 | `SET SCHEMA 'name'` (값 지정 = 문자열 식) | F761 · F763 |
| 카탈로그 | `SET CATALOG 'name'` | F761 · F762 |
| 문자 집합 | `SET NAMES 'cs'` | F761 |
| 경로 | `SET PATH 'sch1, sch2'` | S071 |
| 시간대 | `SET TIME ZONE {LOCAL | INTERVAL '+09:00' HOUR TO MINUTE}` | F411 |
| 사용자 | `SET SESSION AUTHORIZATION 'u'` | F321 |
| 역할 | `SET ROLE {r | NONE}` | T331 |
| 세션 특성 | `SET SESSION CHARACTERISTICS AS TRANSACTION 모드` | F761(추정) |
| 대조 | `SET COLLATION c [FOR cs]` · `SET NO COLLATION` | F693 |
| 변환 그룹 | `SET [DEFAULT] TRANSFORM GROUP …` | S241 |
| 세션 값 조회 | `CURRENT_USER` · `SESSION_USER` · `SYSTEM_USER` · `CURRENT_ROLE` · `CURRENT_SCHEMA` · `CURRENT_CATALOG` · `CURRENT_PATH` | F321 · F762 · F763 · S071 |

출처: [PG-F] [PG-U] https://www.postgresql.org/docs/current/ecpg-connect.html https://www.postgresql.org/docs/current/sql-set-session-authorization.html (Compatibility 절)

## §12. 동적 SQL·커서·진단

| 분류 | 문 | 기능 ID |
|---|---|---|
| 준비 | `PREPARE stmt FROM :sql` · `DEALLOCATE PREPARE stmt` | B031 |
| 실행 | `EXECUTE stmt [INTO 출력] [USING 입력]` · `EXECUTE IMMEDIATE :sql` | B031 |
| 기술 | `DESCRIBE [INPUT | OUTPUT] stmt USING [SQL] DESCRIPTOR 'd'` · `ALLOCATE|DEALLOCATE|GET|SET DESCRIPTOR` | B031 · B036 |
| 확장 동적 | `ALLOCATE CURSOR` · `PREPARE` 이름을 변수로 · `DESCRIBE CURSOR` | B032 · T472 |
| 커서 선언 | `DECLARE c [SENSITIVE | INSENSITIVE | ASENSITIVE] [[NO] SCROLL] CURSOR [WITH | WITHOUT HOLD] [WITH | WITHOUT RETURN] FOR q [FOR READ ONLY | FOR UPDATE [OF …]]` | E121-01 · E121-17 · F431 · F791 · T231 · T471 |
| 열기·닫기 | `OPEN c [USING …]` · `CLOSE c` | E121-04 · E121-08 |
| 가져오기 | `FETCH [NEXT | PRIOR | FIRST | LAST | ABSOLUTE n | RELATIVE n] [FROM] c INTO …` | E121-10 · F432~F437 |
| 결과 집합 | 프로시저 `DYNAMIC RESULT SETS n` + `WITH RETURN` 커서 | T471 |
| 진단 | `GET DIAGNOSTICS :n = ROW_COUNT, … ` · `GET DIAGNOSTICS CONDITION i :s = RETURNED_SQLSTATE, MESSAGE_TEXT, …` | F120~F123 · T511 |
| SQLSTATE | 5글자 = 클래스 2 + 하위 3. 대표: `00` 성공 · `01` 경고 · `02` 자료 없음 · `07` 동적 SQL 오류 · `08` 연결 예외 · `0A` 기능 미지원 · `21` 카디널리티 위반 · `22` 자료 예외 · `23` 무결성 위반 · `24` 잘못된 커서 상태 · `25` 잘못된 트랜잭션 상태 · `28` 권한 지정 오류 · `2B`/`2D` 트랜잭션 종료 관련 · `3D` 잘못된 카탈로그 · `3F` 잘못된 스키마 · `40` 트랜잭션 롤백(`40001` 직렬화 실패) · `42` 구문 오류·접근 위반 · `44` WITH CHECK OPTION 위반 · `HZ` 원격 DB 접근 | E171 |
| 예외 선언 | `WHENEVER {SQLERROR | NOT FOUND | SQLWARNING} {CONTINUE | GOTO l}` (임베디드) | B041 |
| 신호 | `SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = '…'` · `RESIGNAL` · `DECLARE … HANDLER FOR` · `DECLARE CONDITION` | Part 4 SQL/PSM (Foundation 아님 · P0xx 기능 ID — PG 부록에 없음 · 추정) |
| PSM 제어 | `BEGIN [ATOMIC] … END` · `DECLARE v t` · `SET v = …` · `IF/CASE/LOOP/WHILE/REPEAT/FOR/LEAVE/ITERATE` · `RETURN` | Part 4 (BEGIN ATOMIC 단문 본문은 Foundation T321) |

출처: [PG-F] [PG-U] https://www.postgresql.org/docs/current/errcodes-appendix.html https://www.postgresql.org/docs/current/ecpg-dynamic.html https://www.postgresql.org/docs/current/sql-declare.html (Compatibility 절) https://en.wikipedia.org/wiki/SQL/PSM

## §13. 내장 함수 전수

| 분류 | 함수 | 판 | 기능 ID |
|---|---|---|---|
| 수 | `ABS(x)` · `MOD(a,b)` | 99 | T441 |
| 수 | `LN(x)` · `EXP(x)` · `POWER(a,b)` · `SQRT(x)` · `FLOOR(x)` · `CEIL(x)`/`CEILING(x)` · `WIDTH_BUCKET(x, lo, hi, n)` | 2003 | T621 |
| 삼각 | `SIN` · `COS` · `TAN` · `ASIN` · `ACOS` · `ATAN` · `SINH` · `COSH` · `TANH` | 2016 | T622 |
| 로그 | `LOG(b, x)` · `LOG10(x)` | 2016 | T623 · T624 |
| 길이 | `CHAR_LENGTH(s [USING CHARACTERS|OCTETS])`/`CHARACTER_LENGTH` · `OCTET_LENGTH(s)` | 92 | E021-04/05 |
| 위치 | `POSITION(a IN s [USING …])` | 92 | E021-11 · T047 |
| 부분 | `SUBSTRING(s FROM n [FOR m] [USING …])` | 92 | E021-06 |
| 정규 부분 | `SUBSTRING(s SIMILAR p ESCAPE e)` | 99/2008 | T581 |
| 대소 | `UPPER(s)` · `LOWER(s)` | 92 | E021-08 |
| 다듬기 | `TRIM([LEADING|TRAILING|BOTH] [c] FROM s)` | 92 | E021-09 |
| 다듬기(2023) | `LTRIM(s [, chars])` · `RTRIM(s [, chars])` · `BTRIM(s [, chars])` (여러 글자 집합) | 2023 | T056 |
| 채우기(2023) | `LPAD(s, n [, pad])` · `RPAD(s, n [, pad])` | 2023 | T055 |
| 덮어쓰기 | `OVERLAY(s PLACING r FROM n [FOR m])` | 2003 | T312 |
| 정규화 | `NORMALIZE(s [, NFC|NFD|NFKC|NFKD [, len]])` | 2003 | F394 |
| 문자 집합 변환 | `CONVERT(s USING conv)` · `TRANSLATE(s USING tr)` | 92 | F695 |
| 정규식(XQuery) | `OCCURRENCES_REGEX(p IN s …)` · `POSITION_REGEX(…)` · `SUBSTRING_REGEX(…)` · `TRANSLATE_REGEX(p IN s WITH r …)` | 2008 | F842 · F843 · F844 · F845 |
| 날짜·시간 | `CURRENT_DATE` · `CURRENT_TIME[(p)]` · `CURRENT_TIMESTAMP[(p)]` · `LOCALTIME[(p)]` · `LOCALTIMESTAMP[(p)]` | 92 | F051-06~08 · F411 |
| 추출 | `EXTRACT({YEAR|MONTH|DAY|HOUR|MINUTE|SECOND|TIMEZONE_HOUR|TIMEZONE_MINUTE} FROM dt)` | 92 | F051(추정) |
| 서식 변환 | `CAST(dt AS VARCHAR(30) FORMAT 'YYYY-MM-DD')` | 2016 | T839 |
| 비교 | `GREATEST(a, …)` · `LEAST(a, …)` | 2023 | T054 |
| JSON 질의 | `JSON_VALUE(j, 'path' [RETURNING t] [ON EMPTY] [ON ERROR])` · `JSON_QUERY(j, 'path' [WITH|WITHOUT [CONDITIONAL|UNCONDITIONAL] ARRAY WRAPPER] [KEEP|OMIT QUOTES] …)` · `JSON_EXISTS(…)` · `JSON_TABLE(…)` | 2016 | T821 · T825~T829 |
| JSON 생성 | `JSON_OBJECT(k VALUE v | k : v, … [NULL|ABSENT ON NULL] [WITH UNIQUE KEYS] [RETURNING t])` · `JSON_ARRAY(v, … | query …)` | 2016 | T811 · T814 · T830 |
| JSON 2023 | `JSON(s)`(파싱 생성자) · `JSON_SCALAR(v)` · `JSON_SERIALIZE(j [RETURNING t])` · 단순 접근자 `j.a[0]` · 경로 항목 메서드 `.bigint()` `.boolean()` `.date()` `.decimal(p,s)` `.integer()` `.number()` `.string()` `.time()` `.time_tz()` `.timestamp()` `.timestamp_tz()` | 2023 | T801~T803 · T860~T878 |
| JSON 경로 언어 | `lax`/`strict` · `$.a.b` · `[*]` · `.*` · `?( @ > 1 )` 필터 · `starts with` · `like_regex` · `.type()` `.size()` `.double()` `.ceiling()` `.floor()` `.abs()` `.datetime()` `.keyvalue()` | 2016 | T831~T837 |
| XML | `XMLELEMENT` · `XMLFOREST` · `XMLCONCAT` · `XMLCOMMENT` · `XMLPI` · `XMLTEXT` · `XMLPARSE` · `XMLSERIALIZE` · `XMLDOCUMENT` · `XMLCAST` · `XMLQUERY` · `XMLVALIDATE` · `XMLAGG` · `XMLTABLE` · `XMLEXISTS` · `XMLNAMESPACES` | 2003/2006 | X020~X305 |
| 모음 | `CARDINALITY(a|m)` · `ARRAY_MAX_CARDINALITY(a)` · `TRIM_ARRAY(a, n)` · `ELEMENT(m)` · `SET(m)` | 99~2016 | S091 · S403 · S404 · S271 |
| 사용자·세션 | `CURRENT_USER` · `SESSION_USER` · `SYSTEM_USER` · `USER` · `CURRENT_ROLE` · `CURRENT_PATH` · `CURRENT_SCHEMA` · `CURRENT_CATALOG` · `CURRENT_DEFAULT_TRANSFORM_GROUP` · `CURRENT_TRANSFORM_GROUP_FOR_TYPE t` | | F321 · F762 · F763 · S071 · S241 |
| 시퀀스 | `NEXT VALUE FOR seq` | 2003 | T176 |
| 집계 2023 | `ANY_VALUE(x)` | 2023 | T626 |
| 그래프(2023) | Part 16 SQL/PGQ — `CREATE PROPERTY GRAPH g VERTEX TABLES (…) EDGE TABLES (… SOURCE KEY … REFERENCES … DESTINATION KEY …)` · `SELECT … FROM GRAPH_TABLE (g MATCH (a IS person)-[e IS knows]->(b) WHERE … COLUMNS (a.name, b.name))` | 2023 | Part 16 (G 계열 기능 ID · 추정) |

**SQL:2023 추가분 검증 결과**(PG 키워드 표에서 SQL:2016 열에는 없고 SQL:2023 열에 새로 생긴 낱말과 대조):

| 낱말 | SQL:2023 신규 예약어? | 기능 |
|---|---|---|
| `GREATEST` · `LEAST` | ✓ | T054 |
| `LPAD` · `RPAD` | ✓ | T055 |
| `LTRIM` · `RTRIM` · `BTRIM` | ✓ (LTRIM/RTRIM도 2023 신규 — 확인) | T056 |
| `ANY_VALUE` | ✓ | T626 |
| `JSON` · `JSON_SCALAR` · `JSON_SERIALIZE` | ✓ | T801 · T803 |
| `COPARTITION` | 비예약어로 신규 | B202(PTF) |

출처: [PG-F] [PG-U] [PG-K] [W23] https://www.postgresql.org/docs/current/functions-math.html https://www.postgresql.org/docs/current/functions-string.html https://www.postgresql.org/docs/current/functions-json.html https://peter.eisentraut.org/blog/2023/04/04/sql-2023-is-finished-here-is-whats-new (추정 — 주소 미확인) https://www.iso.org/standard/79473.html (ISO/IEC 9075-16:2023 카탈로그 · 추정 — 번호 미확인)

## §14. 예약어 전체

원천 = PostgreSQL 부록 C 키워드 표의 `SQL:2023` · `SQL:2016` 열(844줄 · 2026-10-10 내려받아 기계 집계).

| 판 | 예약어 | 비예약어 | 합 |
|---|---|---|---|
| SQL:2016 | **398** | **297** | 695 |
| SQL:2023 | **409** | **298** | 707 |
| SQL-92(참고) | 227 | 50 | 277 |

**SQL:2023 차이**(2016 대비 · 11개 모두 2016 열에 없던 새 낱말): 예약어 +11 = `ANY_VALUE` · `BTRIM` · `GREATEST` · `JSON` · `JSON_SCALAR` · `JSON_SERIALIZE` · `LEAST` · `LPAD` · `LTRIM` · `RPAD` · `RTRIM` / 비예약어 +1 = `COPARTITION` / 삭제·강등 0.
주의: PG 표는 "PG가 아는 키워드"를 기준으로 줄을 만들므로, 표준에만 있고 PG 문법에 없는 낱말도 표준 열 값으로 실려 있다(예: `ABS` 행). Part 16(SQL/PGQ) 낱말(`GRAPH_TABLE` · `PROPERTY` · `VERTEX` · `EDGE`)은 표에 **없음**(확인) — 표는 Part 2 중심이며, PGQ 키워드의 예약 여부는 미확인(추정).

### 14-1. SQL:2016 예약어 (398개)

| 머리 | 낱말 |
|---|---|
| A | ABS · ABSENT · ACOS · ALL · ALLOCATE · ALTER · AND · ANY · ARE · ARRAY · ARRAY_AGG · ARRAY_MAX_CARDINALITY · AS · ASENSITIVE · ASIN · ASYMMETRIC · AT · ATAN · ATOMIC · AUTHORIZATION · AVG |
| B | BEGIN · BEGIN_FRAME · BEGIN_PARTITION · BETWEEN · BIGINT · BINARY · BLOB · BOOLEAN · BOTH · BY |
| C | CALL · CALLED · CARDINALITY · CASCADED · CASE · CAST · CEIL · CEILING · CHAR · CHARACTER · CHARACTER_LENGTH · CHAR_LENGTH · CHECK · CLASSIFIER · CLOB · CLOSE · COALESCE · COLLATE · COLLECT · COLUMN · COMMIT · CONDITION · CONNECT · CONSTRAINT · CONTAINS · CONVERT · COPY · CORR · CORRESPONDING · COS · COSH · COUNT · COVAR_POP · COVAR_SAMP · CREATE · CROSS · CUBE · CUME_DIST · CURRENT · CURRENT_CATALOG · CURRENT_DATE · CURRENT_DEFAULT_TRANSFORM_GROUP · CURRENT_PATH · CURRENT_ROLE · CURRENT_ROW · CURRENT_SCHEMA · CURRENT_TIME · CURRENT_TIMESTAMP · CURRENT_TRANSFORM_GROUP_FOR_TYPE · CURRENT_USER · CURSOR · CYCLE |
| D | DATALINK · DATE · DAY · DEALLOCATE · DEC · DECFLOAT · DECIMAL · DECLARE · DEFAULT · DEFINE · DELETE · DENSE_RANK · DEREF · DESCRIBE · DETERMINISTIC · DISCONNECT · DISTINCT · DLNEWCOPY · DLPREVIOUSCOPY · DLURLCOMPLETE · DLURLCOMPLETEONLY · DLURLCOMPLETEWRITE · DLURLPATH · DLURLPATHONLY · DLURLPATHWRITE · DLURLSCHEME · DLURLSERVER · DLVALUE · DOUBLE · DROP · DYNAMIC |
| E | EACH · ELEMENT · ELSE · EMPTY · END · END-EXEC · END_FRAME · END_PARTITION · EQUALS · ESCAPE · EVERY · EXCEPT · EXEC · EXECUTE · EXISTS · EXP · EXTERNAL · EXTRACT |
| F | FALSE · FETCH · FILTER · FIRST_VALUE · FLOAT · FLOOR · FOR · FOREIGN · FRAME_ROW · FREE · FROM · FULL · FUNCTION · FUSION |
| G | GET · GLOBAL · GRANT · GROUP · GROUPING · GROUPS |
| H | HAVING · HOLD · HOUR |
| I | IDENTITY · IMPORT · IN · INDICATOR · INITIAL · INNER · INOUT · INSENSITIVE · INSERT · INT · INTEGER · INTERSECT · INTERSECTION · INTERVAL · INTO · IS |
| J | JOIN · JSON_ARRAY · JSON_ARRAYAGG · JSON_EXISTS · JSON_OBJECT · JSON_OBJECTAGG · JSON_QUERY · JSON_TABLE · JSON_TABLE_PRIMITIVE · JSON_VALUE |
| L | LAG · LANGUAGE · LARGE · LAST_VALUE · LATERAL · LEAD · LEADING · LEFT · LIKE · LIKE_REGEX · LISTAGG · LN · LOCAL · LOCALTIME · LOCALTIMESTAMP · LOG · LOG10 · LOWER |
| M | MATCH · MATCHES · MATCH_NUMBER · MATCH_RECOGNIZE · MAX · MEMBER · MERGE · METHOD · MIN · MINUTE · MOD · MODIFIES · MODULE · MONTH · MULTISET |
| N | NATIONAL · NATURAL · NCHAR · NCLOB · NEW · NO · NONE · NORMALIZE · NOT · NTH_VALUE · NTILE · NULL · NULLIF · NUMERIC |
| O | OCCURRENCES_REGEX · OCTET_LENGTH · OF · OFFSET · OLD · OMIT · ON · ONE · ONLY · OPEN · OR · ORDER · OUT · OUTER · OVER · OVERLAPS · OVERLAY |
| P | PARAMETER · PARTITION · PATTERN · PER · PERCENT · PERCENTILE_CONT · PERCENTILE_DISC · PERCENT_RANK · PERIOD · PORTION · POSITION · POSITION_REGEX · POWER · PRECEDES · PRECISION · PREPARE · PRIMARY · PROCEDURE · PTF |
| R | RANGE · RANK · READS · REAL · RECURSIVE · REF · REFERENCES · REFERENCING · REGR_AVGX · REGR_AVGY · REGR_COUNT · REGR_INTERCEPT · REGR_R2 · REGR_SLOPE · REGR_SXX · REGR_SXY · REGR_SYY · RELEASE · RESULT · RETURN · RETURNS · REVOKE · RIGHT · ROLLBACK · ROLLUP · ROW · ROWS · ROW_NUMBER · RUNNING |
| S | SAVEPOINT · SCOPE · SCROLL · SEARCH · SECOND · SEEK · SELECT · SENSITIVE · SESSION_USER · SET · SHOW · SIMILAR · SIN · SINH · SKIP · SMALLINT · SOME · SPECIFIC · SPECIFICTYPE · SQL · SQLEXCEPTION · SQLSTATE · SQLWARNING · SQRT · START · STATIC · STDDEV_POP · STDDEV_SAMP · SUBMULTISET · SUBSET · SUBSTRING · SUBSTRING_REGEX · SUCCEEDS · SUM · SYMMETRIC · SYSTEM · SYSTEM_TIME · SYSTEM_USER |
| T | TABLE · TABLESAMPLE · TAN · TANH · THEN · TIME · TIMESTAMP · TIMEZONE_HOUR · TIMEZONE_MINUTE · TO · TRAILING · TRANSLATE · TRANSLATE_REGEX · TRANSLATION · TREAT · TRIGGER · TRIM · TRIM_ARRAY · TRUE · TRUNCATE |
| U | UESCAPE · UNION · UNIQUE · UNKNOWN · UNNEST · UPDATE · UPPER · USER · USING |
| V | VALUE · VALUES · VALUE_OF · VARBINARY · VARCHAR · VARYING · VAR_POP · VAR_SAMP · VERSIONING |
| W | WHEN · WHENEVER · WHERE · WIDTH_BUCKET · WINDOW · WITH · WITHIN · WITHOUT |
| X | XML · XMLAGG · XMLATTRIBUTES · XMLBINARY · XMLCAST · XMLCOMMENT · XMLCONCAT · XMLDOCUMENT · XMLELEMENT · XMLEXISTS · XMLFOREST · XMLITERATE · XMLNAMESPACES · XMLPARSE · XMLPI · XMLQUERY · XMLSERIALIZE · XMLTABLE · XMLTEXT · XMLVALIDATE |
| Y | YEAR |

### 14-2. SQL:2016 비예약어 (297개)

| 머리 | 낱말 |
|---|---|
| A | A · ABSOLUTE · ACCORDING · ACTION · ADA · ADD · ADMIN · AFTER · ALWAYS · ASC · ASSERTION · ASSIGNMENT · ATTRIBUTE · ATTRIBUTES |
| B | BASE64 · BEFORE · BERNOULLI · BLOCKED · BOM · BREADTH |
| C | C · CASCADE · CATALOG · CATALOG_NAME · CHAIN · CHAINING · CHARACTERISTICS · CHARACTERS · CHARACTER_SET_CATALOG · CHARACTER_SET_NAME · CHARACTER_SET_SCHEMA · CLASS_ORIGIN · COBOL · COLLATION · COLLATION_CATALOG · COLLATION_NAME · COLLATION_SCHEMA · COLUMNS · COLUMN_NAME · COMMAND_FUNCTION · COMMAND_FUNCTION_CODE · COMMITTED · CONDITIONAL · CONDITION_NUMBER · CONNECTION · CONNECTION_NAME · CONSTRAINTS · CONSTRAINT_CATALOG · CONSTRAINT_NAME · CONSTRAINT_SCHEMA · CONSTRUCTOR · CONTENT · CONTINUE · CONTROL · CURSOR_NAME |
| D | DATA · DATETIME_INTERVAL_CODE · DATETIME_INTERVAL_PRECISION · DB · DEFAULTS · DEFERRABLE · DEFERRED · DEFINED · DEFINER · DEGREE · DEPTH · DERIVED · DESC · DESCRIPTOR · DIAGNOSTICS · DISPATCH · DOCUMENT · DOMAIN · DYNAMIC_FUNCTION · DYNAMIC_FUNCTION_CODE |
| E | ENCODING · ENFORCED · ERROR · EXCLUDE · EXCLUDING · EXPRESSION |
| F | FILE · FINAL · FINISH · FIRST · FLAG · FOLLOWING · FORMAT · FORTRAN · FOUND · FS · FULFILL |
| G | G · GENERAL · GENERATED · GO · GOTO · GRANTED |
| H | HEX · HIERARCHY |
| I | ID · IGNORE · IMMEDIATE · IMMEDIATELY · IMPLEMENTATION · INCLUDING · INCREMENT · INDENT · INITIALLY · INPUT · INSTANCE · INSTANTIABLE · INSTEAD · INTEGRITY · INVOKER · ISOLATION |
| K | K · KEEP · KEY · KEYS · KEY_MEMBER · KEY_TYPE |
| L | LAST · LENGTH · LEVEL · LIBRARY · LIMIT · LINK · LOCATION · LOCATOR |
| M | M · MAP · MAPPING · MATCHED · MAXVALUE · MEASURES · MESSAGE_LENGTH · MESSAGE_OCTET_LENGTH · MESSAGE_TEXT · MINVALUE · MORE · MUMPS |
| N | NAME · NAMES · NAMESPACE · NESTED · NESTING · NEXT · NFC · NFD · NFKC · NFKD · NIL · NORMALIZED · NULLABLE · NULLS · NULL_ORDERING · NUMBER |
| O | OBJECT · OCCURRENCE · OCTETS · OFF · OPTION · OPTIONS · ORDERING · ORDINALITY · OTHERS · OUTPUT · OVERFLOW · OVERRIDING |
| P | P · PAD · PARAMETER_MODE · PARAMETER_NAME · PARAMETER_ORDINAL_POSITION · PARAMETER_SPECIFIC_CATALOG · PARAMETER_SPECIFIC_NAME · PARAMETER_SPECIFIC_SCHEMA · PARTIAL · PASCAL · PASS · PASSING · PASSTHROUGH · PAST · PATH · PERMISSION · PERMUTE · PIPE · PLACING · PLAN · PLI · PRECEDING · PRESERVE · PREV · PRIOR · PRIVATE · PRIVILEGES · PRUNE · PUBLIC |
| Q | QUOTES |
| R | READ · RECOVERY · RELATIVE · REPEATABLE · REQUIRING · RESPECT · RESTART · RESTORE · RESTRICT · RETURNED_CARDINALITY · RETURNED_LENGTH · RETURNED_OCTET_LENGTH · RETURNED_SQLSTATE · RETURNING · ROLE · ROUTINE · ROUTINE_CATALOG · ROUTINE_NAME · ROUTINE_SCHEMA · ROW_COUNT |
| S | SCALAR · SCALE · SCHEMA · SCHEMA_NAME · SCOPE_CATALOG · SCOPE_NAME · SCOPE_SCHEMA · SECTION · SECURITY · SELECTIVE · SELF · SEMANTICS · SEQUENCE · SERIALIZABLE · SERVER · SERVER_NAME · SESSION · SETS · SIMPLE · SIZE · SORT_DIRECTION · SOURCE · SPACE · SPECIFIC_NAME · STANDALONE · STATE · STATEMENT · STRING · STRIP · STRUCTURE · STYLE · SUBCLASS_ORIGIN |
| T | T · TABLE_NAME · TEMPORARY · THROUGH · TIES · TOKEN · TOP_LEVEL_COUNT · TRANSACTION · TRANSACTIONS_COMMITTED · TRANSACTIONS_ROLLED_BACK · TRANSACTION_ACTIVE · TRANSFORM · TRANSFORMS · TRIGGER_CATALOG · TRIGGER_NAME · TRIGGER_SCHEMA · TYPE |
| U | UNBOUNDED · UNCOMMITTED · UNCONDITIONAL · UNDER · UNLINK · UNMATCHED · UNNAMED · UNTYPED · URI · USAGE · USER_DEFINED_TYPE_CATALOG · USER_DEFINED_TYPE_CODE · USER_DEFINED_TYPE_NAME · USER_DEFINED_TYPE_SCHEMA · UTF16 · UTF32 · UTF8 |
| V | VALID · VERSION · VIEW |
| W | WHITESPACE · WORK · WRAPPER · WRITE |
| X | XMLDECLARATION · XMLSCHEMA |
| Y | YES |
| Z | ZONE |

출처: [PG-K] https://www.postgresql.org/docs/current/sql-keywords-appendix.html [W23] https://en.wikipedia.org/wiki/SQL_reserved_words

## §15. nexa-sql 대조 열

> 2026-10-10 · 읽기 전용 정적 분석(협업 세션 코드 감사 · 기준 HEAD 3e739d7 + 10-10 미커밋) · 방언별 상세 = [108 §11](108-dbms-syntax-and-objects-survey.md).
> 구조: 표준 문법 참조 = `crates/nsql-script/grammar/ansi.sqlg`(문장 5 · 절 40) 위에 방언 파일(`oracle` · `mssql` · `postgres` · `sqlite` · `mysql`)이 **덧입힌다**(`grammar.rs:101-146`). 덧입히기만 있고 **빼기 연산이 없다** — ansi에 들어간 것은 모든 방언에 상속된다.

### §15-1 문장 단위(§7~§12 대조)

`ansi.sqlg [start] next`(`ansi.sqlg:8-9`) = SELECT · WITH · INSERT INTO · UPDATE · DELETE FROM · MERGE INTO · CREATE · ALTER · DROP · TRUNCATE TABLE · GRANT · REVOKE · COMMIT · ROLLBACK · SAVEPOINT · EXPLAIN · DESCRIBE · BEGIN · DECLARE.

| 표준 문장(109 절) | ansi.sqlg | 비고 |
|---|---|---|
| 질의 · `WITH [RECURSIVE]`(§5) | ✅ | `ansi.sqlg:20-23` · SEARCH/CYCLE 절 ❌ |
| `INSERT` · `UPDATE` · `DELETE`(§7) | ✅ | `OVERRIDING` · `WHERE CURRENT OF` · `FOR PORTION OF` ❌ |
| `MERGE`(§7) | 부분 | `WHEN MATCHED [AND] THEN UPDATE/DELETE` · `WHEN NOT MATCHED THEN INSERT`만 |
| `TRUNCATE TABLE`(§7) | ✅ | |
| `CREATE SCHEMA/TABLE/VIEW/SEQUENCE/TRIGGER/PROCEDURE/FUNCTION/TYPE`(§8) | ✅ | `CREATE INDEX`(비표준)도 들어 있음 · `ansi.sqlg:113-114` |
| `CREATE DOMAIN/ASSERTION/ROLE/CHARACTER SET/COLLATION/TRANSLATION/CAST/ORDERING/TRANSFORM/METHOD`(§8) | ❌ | |
| `CREATE GLOBAL/LOCAL TEMPORARY TABLE` · `RECURSIVE VIEW`(§8) | ❌ | |
| `ALTER TABLE` ADD / DROP COLUMN / DROP CONSTRAINT / RENAME(§8) | ✅ | `ansi.sqlg:118-120` |
| `ALTER TABLE … ALTER COLUMN` · `SET/DROP DEFAULT`(§8) | ❌ | MS·PG 방언 파일에만 |
| `ALTER DOMAIN/TYPE/ROUTINE/SEQUENCE`(§8) | 부분 | SEQUENCE만 |
| `DROP …` `CASCADE/RESTRICT`(§8) | 부분 | `IF EXISTS`는 있음(비표준) · CASCADE/RESTRICT 없음 |
| `GRANT` / `REVOKE`(§9) | 부분 | 시작 낱말만 · 다음 낱말 없음 |
| `START TRANSACTION` · `SET TRANSACTION` · `SET CONSTRAINTS` · `RELEASE SAVEPOINT`(§10) | ❌ | |
| `COMMIT` · `ROLLBACK` · `SAVEPOINT`(§10) | 부분 | 시작 낱말만 · `AND CHAIN`·`TO SAVEPOINT` 없음 |
| `CALL` · `RETURN`(§8 루틴) | ❌ | |
| `DECLARE CURSOR` · `OPEN` · `FETCH` · `CLOSE`(§12) | 부분 | `DECLARE`만 |
| `SET SCHEMA/CATALOG/ROLE/SESSION AUTHORIZATION/TIME ZONE/PATH`(§11) | ❌ | |
| `CONNECT` · `DISCONNECT` · `SET CONNECTION`(§11) | 클라이언트 명령 | `command.rs` |
| `PREPARE` · `EXECUTE` · `DESCRIBE` · `GET DIAGNOSTICS`(§12) | 부분 | `DESCRIBE`만 |
| `VALUES` 문 · `TABLE t` 문(§5) | ❌ | |

### §15-2 절·식 단위(§3~§6)

| 표준 요소 | nexa-sql | 비고 |
|---|---|---|
| `WINDOW` 절 · 프레임 `ROWS/RANGE/GROUPS … EXCLUDE` | ❌ | 완성 후보 없음 · 강조 낱말 없음(`PRECEDING` `FOLLOWING` `UNBOUNDED`) |
| `LATERAL` · `TABLESAMPLE` · `UNNEST` | ❌ | |
| `GROUPING SETS` · `ROLLUP` · `CUBE` | ❌ | |
| 집계 `FILTER (WHERE …)` · `WITHIN GROUP` | ❌ | |
| `FETCH FIRST/NEXT … ROWS ONLY/WITH TIES` | 부분 | `FETCH FIRST`만 · 상속으로 SQLite·MySQL에도 뜸 |
| `LEFT/RIGHT/FULL OUTER JOIN` | 부분 | OUTER 별칭은 있으나 FROM 다음 낱말에 `RIGHT OUTER`·`FULL OUTER` 없음 · `ansi.sqlg:28-50` |
| 술어 `IS [NOT] DISTINCT FROM` · `SIMILAR TO` · `LIKE_REGEX` · `IS JSON` | ❌ | §4 |
| `CAST` · `CASE` · `NULLIF` · `COALESCE` | ✅ | `builtins` 공통 함수 39 |
| SQL:2023 함수 `GREATEST` `LEAST` `LPAD` `RPAD` `LTRIM` `RTRIM` `BTRIM` `ANY_VALUE` | 부분 | 방언 함수 표에는 있으나 ANSI 공통 표에는 없음(추정 · 감사 표 기준) |

### §15-3 ansi.sqlg에 섞인 비표준

| 낱말 | 표준 여부 | 결과 |
|---|---|---|
| `LIMIT` | 비표준(SQLite·PG·MySQL) | ORA·MS 완성에도 뜸 |
| `RETURNING` | 비표준(ORA·PG·SQLite) | MS·MySQL에도 뜸 |
| `EXPLAIN` · `DESCRIBE` | 비표준(동적 SQL DESCRIBE와 뜻이 다름) | 전 방언 |
| `CREATE INDEX` | 비표준(표준에 인덱스 없음) | 실용상 유지 |
| `BEGIN` | 표준은 `BEGIN ATOMIC`(루틴 본문)과 `START TRANSACTION` | 트랜잭션 시작으로 쓰는 방언(PG·SQLite·MS) 있음 |

### §15-4 강조기·인용 판정

- 강조기 키워드 = 방언 무관 196낱말(`nexa-ui/crates/nexa-ctl/src/highlight.rs:439-448`). §14 예약어 398(2016) 대비 **약 절반** — 빠진 표본: `LATERAL` `ROLLUP` `CUBE` `GROUPING` `SETS` `FILTER` `WITHIN` `NULLS` `PRECEDING` `FOLLOWING` `UNBOUNDED` `RANGE` `INTERVAL` `COLLATE` `ESCAPE` `ANY` `SOME` `SIMILAR` `CALL`.
- 강조기 어휘: 줄 주석 `--`만 · 블록 주석 비중첩(표준은 중첩) · `"…"`를 문자열 색으로 칠함(표준에서 `"…"` = **식별자**).
- 인용 판정 예약어 = 포맷터 절 키워드 117(`identq.rs:52` → `nsql-format/src/lib.rs:437-565`) · 표준 예약어 398과 대조하면 대부분 빠짐 → 열 이름이 예약어여도 인용 없이 들어간다(108 §9 T-325).

## §16. 점검 절차 · 결과

이 문서를 **기능 점검표**로 쓰는 방법. 절마다 예문을 만들어 네 층에서 같은 문장을 본다. 키·마우스 주입 없이 돌리는 길만 쓴다(61 §2-4).

| 층 | 무엇을 보나 | 도구(이미 있는 길) | 판정 |
|---|---|---|---|
| ① 분리·어휘 | 문장 수 · 문자열/주석 경계 · 바인드 | `nsql plan -d <방언> <예문.sql>` | 기대 문장 수 = 실제 |
| ② 완성 | 그 자리의 다음 낱말 후보 · 객체 자리 | GUI 기동 명령 `open:<예문>` → `editor.caret:<줄>/<열>` → `intel.probe` → `intel.dump:<파일>` | 표준 절 순서에 맞는 후보가 있고, 그 방언에 없는 문법(§15-3)이 없다 |
| ③ 강조 | 예약어·자료형·주석·문자열 색 | 창 캡처(`screencapture -l`) | 예약어 = 키워드 색 · `"x"` = 식별자 색 |
| ④ 실행 | 실서버 왕복(읽기 · 임시 객체 `NSQLT_*`만) | `nsql run -c <프로필>` | 서버 오류 없음 |

**예문 원천**: §1~§13 각 표의 문법 꼴 → `examples/ansi/<절>.sql`(만들 예정 · 예: `05-query.sql` = LATERAL · GROUPING SETS · WINDOW · FETCH WITH TIES) · 방언별 = 108 같은 T 번호 표의 예문.

**결과(2026-10-10)**: **아직 수행하지 않음**. 이번 차수는 조사·문서화와 정적 대조(§15)까지만 했다. 첫 수행 = T-326(문법 빼기 연산·누락 절) 착수 때 ①②를 먼저 자동화하고, 결과 표를 이 절에 적는다(통과 · 실패 · 해당 없음 · 실행 커밋).

## §17. 자동 완성 정확도 개선 후보(우선순위)

판단 기준 = **틀린 후보를 내는 것**(사용자가 고르면 구문 오류)을 **빠진 후보**보다 먼저. 근거 = §15 · 108 §9.

| 순위 | 후보 | 효과 | 근거 | TODO |
|---|---|---|---|---|
| 1 | `.sqlg` **빼기 연산**(`-next` 또는 방언별 `exclude`) + ansi의 비표준 낱말(`LIMIT` · `RETURNING`)을 방언 파일로 옮김 | ORA·MS의 `LIMIT`, MS·MY의 `RETURNING`, SQLite·MySQL의 `MERGE`·`FETCH FIRST` 같은 **틀린 후보 제거** | §15-3 · 108 §9 #9 | T-326 |
| 2 | 인용 판정·강조를 **DBMS 예약어 사실 표**로(예약/비예약 구분 · 방언별) | 예약어 열 이름 자동 인용 · 강조 누락 해소 | §14 · §15-4 · 108 §8-2 | T-325 |
| 3 | 어휘 층 방언화(백틱 · `#` · `\'` · 중첩 주석 · PG `[]`) | 문자열·주석 경계가 틀려 **완성 문맥 전체가 어긋나는 일** 제거 | 108 §9 #1~#6 | T-324 |
| 4 | 표준 절 보강: `WINDOW`/프레임 · `GROUPING SETS/ROLLUP/CUBE` · `LATERAL` · `FILTER`/`WITHIN GROUP` · `FETCH … WITH TIES` · `RIGHT/FULL OUTER JOIN` | 빠진 후보 채움 | §15-2 | T-326 |
| 5 | 표준 문장 보강: `CALL` · `START/SET TRANSACTION` · `RELEASE SAVEPOINT` · `SET SCHEMA` · `VALUES` · `DROP … CASCADE/RESTRICT` · `ALTER COLUMN` | 시작 낱말·다음 낱말 채움 | §15-1 | T-326 |
| 6 | SQL:2023 함수(`GREATEST` `LEAST` `LPAD` `RPAD` `LTRIM` `RTRIM` `BTRIM` `ANY_VALUE`)를 ANSI 공통 함수 표에 · 방언에 없는 것은 방언 표에서 막기 | 함수 후보 정확도 | §13 · §14(2023 신규 11) | T-325 |
| 7 | 술어 후보(`IS DISTINCT FROM` · `SIMILAR TO` · `IS JSON`)는 **지원하는 방언에서만** | 틀린 후보 방지 | §4 · 108 각 장 T-5 | T-326 |
