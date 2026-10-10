# 108 · DBMS별 문법·객체 생성 전수 조사 — 지원 DBMS 6 · 새 DBMS 추가 시 재사용 템플릿 (사용자 10-10)

> **요구**(사용자 10-10): "DBMS별 문법, 객체 생성에 대한 부분을 현재 지원하는 DBMS에 대해서 Study해주고 나중에 지원 DBMS를 추가하면 동일한 조사를 선행할 수 있도록 문서화 잘해줘 · 이 문서를 기준으로 기능 점검도 진행하고 자동 완성 정확도 개선에 쓸 것". 배경 = 같은 날 지적 "예약어·자료형·오버로드처럼 DBMS 분석이 충분했으면 놓치지 않았을 오류".
> **원천 규칙**: 각 장의 사실은 **공식 문서**를 원천으로 하고 출처 URL을 표 아래에 단다. 공식 문서에서 확인하지 못한 것은 **"추정"** 으로 표시한다. 수량(예약어 수 · 함수 수)은 조사 시점의 판 기준이며 판을 함께 적는다.
> **짝 문서**: ANSI/ISO 표준 문법 전수 = [109](109-ansi-sql-syntax-survey.md) · 문법 참조 기반 완성 = [82](82-grammar-driven-completion.md) · 인텔리센스 = [76](76-intellisense-and-outline.md) · 객체 탐색기 트리 = [83](83-object-explorer-dbms-trees-and-generate-sql.md) · 소스 생성 원칙 = [100 §5](100-object-source-run-and-output-tab.md) · 변수·방언 재작성 = [63](63-variable-management.md).

## §0. 목적과 쓰임

이 문서는 nexa-sql이 DBMS를 다루는 모든 층의 **단일 원천(사실 표)** 이다. 한 DBMS의 어휘·예약어·자료형·함수·객체 규칙이 코드 여러 곳에 흩어져 서로 다르게 적히는 것을 막는다.

| 쓰는 곳 | 이 문서에서 가져가는 것 | 코드 자리(이 저장소에서는) |
|---|---|---|
| 자동 완성 · 문법 참조 | 절 순서 · 다음에 올 수 있는 낱말 · 객체 자리(T-7~T-10) | `crates/nsql-script/grammar/<방언>.sqlg` · `crates/nsql-script/src/intel.rs` · `grammar.rs` |
| 내장 목록 | 예약어(T-3) · 자료형(T-4) · 내장 함수 시그니처(T-6) · 시스템 패키지/프로시저 | `crates/nsql-script/src/builtins.rs` |
| 어휘 분석 | 식별자·인용·리터럴·주석·구분자·바인드(T-2) | `crates/nsql-script/src/lexer.rs` · `split.rs` · `bind.rs` · `dialect.rs` |
| 하이라이터 · 포맷터 | 키워드 집합(T-3) · 블록 구분(T-10) | nexa-ui `Highlighter` · `crates/nsql-format` |
| 카탈로그 · 탐색기 · 소스 생성 | 객체 종류 · 이름 계층 · 상태 · 오버로드 식별 키 · 카탈로그 뷰 · 소스 보기(T-9) | `crates/nsql-catalog`(`tree.rs` · `gen.rs` · `detail.rs`) |
| 실행기 · 드라이버 | 배치 구분 · 호출 문법 · 트랜잭션 기본 · 오류 모델 · 바인드 타입 · 대량 적재 · 취소(T-10~T-14) | `crates/nsql-run` · `crates/nsql-driver-*` |
| **기능 점검** | 항목마다 "지원 현황(T-15)" 열 = 점검표 | `scripts/*-func-check.*` · E2E 스크립트 |

**쓰는 순서**: ① 기능을 고치거나 완성 오류를 볼 때 → 그 DBMS 장의 해당 T 번호 표를 먼저 연다 ② 표와 코드가 다르면 = 결함 후보 → §9에 적고 TODO 번호를 단다 ③ 새 DBMS를 추가할 때 → §1 템플릿을 그대로 복사해 새 장을 채운 뒤 §10 절차로 코드에 반영한다.

### §0-1 조사 기록·신뢰도(2026-10-10 · 협업 세션)

장마다 공식 문서를 직접 열어 대조한 정도가 다르다. **코드를 고치는 근거로 쓰기 전에 "추정"과 아래 "대조 안 함" 범위는 원문을 한 번 더 연다.**

| 장 | 기준 판 | 출처 URL(고유) | 직접 대조 | "추정"·대조 안 함 |
|---|---|---|---|---|
| §2 Oracle | 19c | 57 | 식별자·리터럴·SQL 예약어 110 · PL/SQL 예약어 85 · 자료형 · 우선순위 · 집계 56/분석 46 함수 · ALL_PROCEDURES/ALL_ARGUMENTS(OVERLOAD·SUBPROGRAM_ID) | 63곳(PL/SQL 키워드 전체 · 제공 패키지 서브프로그램 목록 · DELETE/TRUNCATE/SAVEPOINT/REVOKE/CALL 페이지 · OCI 속성 이름 · JSON 형 21c · IF [NOT] EXISTS 19.28 백포트) |
| §3 SQL Server | 2019+(2022·2025 표기) | 96 | 16 URL만(호환성 수준 · 식별자 · 예약어 185 · 우선순위 · 자료형 · 날짜·JSON 함수 · 함수 범주) | 나머지 80 URL은 **위치만** 적고 내용은 대조 안 함(문자열 함수 이름 · 시스템 프로시저 · `sys.objects.type` 코드 · TDS 세부) |
| §4 PostgreSQL | 15/16 | 87 | 어휘 · 우선순위 16단 · NULLS 기본 · 부록 C 키워드(78/23/55) · 문자열·수학·날짜·집계·JSON 함수 · SELECT · 권한 표 | 14곳(일반 비예약 약 300개 목록 미수록 · 이진/비트/기하 함수 · 형 OID 표) · 17 기능 표기(MERGE RETURNING·JSON_TABLE) |
| §5 SQLite | 3.46(최신 3.54 표기) | 62 | 키워드 147 · 핵심 함수 60 서명 · 우선순위 · 기능 도입 판 | 6곳 + **판 숫자 표시 없이 들어간 것**(UPDATE FROM 3.33 · 생성 열 3.31 · NULLS FIRST/LAST 3.30 · RENAME COLUMN 3.25 · 행 값 3.15) = changes.html 대조 필요 |
| §6·§7 MySQL·ODBC | 8.0/8.4 · ODBC 3.8 | 144 | MySQL 예약어 260 · ODBC 예약 키워드 235 · 함수 참조 전체 · ODBC 문자열/수치/날짜 스칼라 함수 | 9곳 · ODBC 시스템·변환 함수 목록은 대조 안 함 · MySQL 함수 장별 분류 = 조사자 분류 |
| §11 코드 감사 | HEAD 3e739d7 + 10-10 미커밋 | — | 정적 분석(grep/read) | **실행 확인 없음** — §9 결함은 재현부터 |

## §1. 조사 템플릿 — 새 DBMS가 오면 이 번호 그대로 채운다

각 장(§2~§7)은 아래 **T-1~T-15**를 같은 번호로 가진다. 표 위주로 쓰고, 표 아래에 출처 URL을 단다. 해당 없는 항목은 지우지 말고 "해당 없음(이유)"으로 남긴다(빠뜨림과 구분하기 위해).

| 번호 | 항목 | 채울 내용 |
|---|---|---|
| **T-1** | 버전·판·공식 문서 원천 | 조사 기준 버전 · 지원 하한 버전 · 판(에디션) 차이 · 공식 문서 첫 페이지 URL(SQL 참조 · 카탈로그 참조 · 오류 참조) |
| **T-2** | 어휘 | 식별자 최대 길이 · 허용 문자 · 인용 문자(`"` · `[]` · `` ` ``) · 대소문자 접힘(대문자/소문자/보존) · 유니코드 · **리터럴** = 문자열 인용·이스케이프 · `N''` · `q''` · `E''` · `$$` · 숫자 · 날짜시간 · 간격 · 이진 · 불리언 · NULL · **주석** 3종(`--` · `/* */` · `#`) · 중첩 주석 여부 · **문장 구분자/배치**(`;` · `/` · `GO` · `$$` · `DELIMITER`) · **바인드 표기**(`:n` · `@n` · `$1` · `?`) · 대체 변수(`&v`) |
| **T-3** | 예약어·키워드 전체 | 공식 목록 원천 · 수 · **예약/비예약 구분**(비예약은 식별자로 쓸 수 있음) · 인용 없이 쓰면 오류가 나는 낱말 · 완성·하이라이터에 넣을 집합 |
| **T-4** | 자료형 전체 | 이름 · 별칭 · 인자 모양(`(n)` · `(p,s)` · `(n CHAR)` · `MAX`) · 범위·최대 크기 · 기본 인자 · 사용자 정의 타입(도메인 · 객체 타입 · 별칭 타입) |
| **T-5** | 연산자·우선순위·비교 규칙 | 산술 · 문자열 연결(`\|\|` · `+` · `CONCAT`) · 비교 · 논리 · 우선순위 표 · NULL 3값 논리 · 빈 문자열 = NULL 여부 · NULLS FIRST/LAST 기본 · 정렬 규칙(collation) · 대소문자 구분 비교 기본 |
| **T-6** | 내장 함수 전수 | 범주별(문자열 · 숫자 · 날짜시간 · 변환 · 조건 · 집계 · 윈도 · JSON/XML · 시스템) + **시그니처**(인자 이름·타입·선택 인자) · 시스템 패키지/프로시저(예 `DBMS_OUTPUT` · `sp_*`) |
| **T-7** | 질의 문법 | 절 순서 · 행 제한(`TOP` · `LIMIT` · `FETCH FIRST` · `ROWNUM`) · 힌트 · 계층 질의(`CONNECT BY`) · `PIVOT`/`UNPIVOT` · `MODEL` · 집합 연산 · 조인 종류 · `LATERAL`/`APPLY` · 샘플링 · `FOR UPDATE` · CTE · 재귀 |
| **T-8** | DML | `INSERT` 변형(다중 행 · `INSERT ALL/FIRST` · `SELECT`에서) · `RETURNING`/`OUTPUT` · `ON CONFLICT`/`ON DUPLICATE KEY` · `UPDATE … FROM`/조인 갱신 · `DELETE` 조인 · `MERGE` · `TRUNCATE` · `UPSERT`/`REPLACE` |
| **T-9** | **객체 종류 전수 표** | 테이블 종류(힙 · 임시 · 분할 · IOT · 외부) · 뷰 · MV · 시퀀스/IDENTITY · 인덱스 종류 · 제약 · 트리거 · 루틴(프로시저 · 함수 · 패키지 · 타입 메서드 · 집계 · 연산자) · 타입/도메인 · 시노님 · DB 링크 · 스키마/DB/카탈로그 계층 · 사용자/롤 · 잡/큐 · 확장. **객체마다** = CREATE/ALTER/DROP 핵심 절 · `OR REPLACE`/`IF [NOT] EXISTS` · 이름 규칙(2부/3부/4부) · 상태(VALID/ENABLED) · 의존 · **오버로드 가능 여부와 식별 키**(예 Oracle 패키지 멤버 = `overload` + `subprogram_id` · PG = oid/서명 · SQL Server = 없음) · 읽는 카탈로그 뷰 · 소스 보기 방법 |
| **T-10** | 루틴 세부 | 인자 모드(IN/OUT/IN OUT/INOUT/VARIADIC) · 기본값 · 이름 지정 호출(`=>` · `@x =`) · 반환(스칼라 · 테이블 · REF CURSOR · SETOF · 다중 결과 집합) · 언어 · **블록 구분**(`/` · `GO` · `$$` · `DELIMITER`) · 호출 문법(`EXEC` · `CALL` · `SELECT f()` · 익명 블록) |
| **T-11** | 트랜잭션·세션 | 자동 커밋 기본 · `BEGIN`/`COMMIT`/`ROLLBACK`/`SAVEPOINT` · DDL 자동 커밋 여부 · 격리 수준 · 세션 설정 문 · 스키마/DB 전환(`USE` · `ALTER SESSION` · `search_path`) |
| **T-12** | DCL | `GRANT`/`REVOKE` 대상·권한 이름 · 롤 · 객체/시스템 권한 · `WITH GRANT OPTION` |
| **T-13** | 오류 모델 | 오류 코드 형식(`ORA-nnnnn` · 번호/상태 · SQLSTATE) · 줄/열 위치 · 컴파일 오류 뷰(`USER_ERRORS` 등) · 경고 |
| **T-14** | 클라이언트 인터페이스 | 드라이버(이 저장소의 crate) · 바인드 타입 매핑 · LOB · 다중 결과 · 대량 적재(COPY · 배열 DML · TDS bulk) · 실행 취소 방식 |
| **T-15** | **nexa-sql 지원 현황** | 위 항목마다 ✅ / 부분 / ❌ · 코드 자리(파일) · 결함·TODO 번호 — **기능 점검표**로 그대로 쓴다 |

> 표 머리 공통: `| 항목 | DBMS 사실 | 출처 | nexa-sql(T-15) |` — 사실 칸과 지원 칸을 한 줄에 두어 차이가 바로 보이게 한다.

## §2. Oracle Database 19c

> 범례 — 표의 내용은 공식 문서(docs.oracle.com 19c)에서 확인한 것이 기본이다. 공식 원천에서 직접 확인하지 못한 항목은 **추정**으로 표시했다(일반 지식·2차 자료·다른 버전 문서 기반).
> 공통 약어: `sqlrf` = SQL Language Reference · `lnpls` = PL/SQL Language Reference · `refrn` = Reference(사전 뷰) · `arpls` = PL/SQL Packages and Types Reference · `lnoci` = OCI Programmer's Guide.

### T-1 버전·판·공식 문서 원천

| 항목 | 내용 |
|---|---|
| 대상 버전 | Oracle Database **19c**(12.2 계열 장기 지원판 · 내부 번호 19.x RU) |
| 하한 버전 메모 | 식별자 128바이트 = `COMPATIBLE` ≥ 12.2 · 그 아래는 30바이트(T-2) · `FETCH FIRST`/`OFFSET`·`IDENTITY`·`DBMS_SQL.RETURN_RESULT`(암시 결과) = 12c(12.1)부터 · 개인 임시 테이블 = 18c부터 · `JSON` 자료형 = 21c부터(19c 없음) · `IF [NOT] EXISTS` = 23ai(19.28 백포트 = **추정**, 2차 자료) |
| 판(on-premises) | Standard Edition 2(SE2) · Enterprise Edition(EE) · EE on Engineered Systems(EE-ES) · Personal Edition(PE · Linux/Windows) |
| 판 차이(대표) | Partitioning = EE/EE-ES 유료 옵션 · PE 포함 · **SE2 불가** · 그 밖 EE 전용 옵션(In-Memory · Advanced Security · Diagnostics/Tuning Pack 등) = SE2 불가(**추정**, 2차 자료) · SE2 RAC 불가(19c부터 · **추정**) |
| SQL 문법 영향 | 문법 자체는 판과 무관(파서 동일). 판 차이는 기능 사용(라이선스·옵션 설치) 문제 — 파티션 DDL은 SE2에서 실행 오류(ORA-00439) **추정** |

| 공식 문서 | URL |
|---|---|
| SQL Language Reference | https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/ |
| PL/SQL Language Reference | https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/ |
| Reference(초기화 매개변수·사전 뷰·V$) | https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ |
| Error Messages | https://docs.oracle.com/en/database/oracle/oracle-database/19/errmg/ · (신규 통합) https://docs.oracle.com/en/error-help/db/ |
| PL/SQL Packages and Types | https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/ |
| OCI Programmer's Guide | https://docs.oracle.com/en/database/oracle/oracle-database/19/lnoci/ |
| Licensing Information | https://docs.oracle.com/en/database/oracle/oracle-database/19/dblic/Licensing-Information.html |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/dblic/Licensing-Information.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Database-Object-Names-and-Qualifiers.html
출처(2차·추정): https://oracle-base.com/articles/23/if-not-exists-ddl-clause-23 · https://oraclelicensingexperts.com/blog-oracle-database-se2-licensing.html

---

### T-2 어휘

#### T-2-1 식별자

| 규칙 | 내용 |
|---|---|
| 최대 길이 | `COMPATIBLE` ≥ 12.2: **1~128바이트**(기본) · DB 이름 8바이트 · 디스크 그룹·PDB·롤백 세그먼트·테이블스페이스(셋) 30바이트 / < 12.2: 30바이트 · DB 링크 128바이트 |
| 다부분 이름 | 부분마다 따로 한도 · 마침표·따옴표 각 1바이트 → `"s"."t"."c"` 최대 392바이트 |
| 비인용 식별자 | 영문자(DB 문자셋의 알파벳)로 시작 · 영숫자 + `_` `$` `#`(`$`·`#` 비권장) · DB 링크는 `.` `@` 추가 허용 · 예약어 불가 |
| 인용 식별자 `"…"` | 아무 글자로 시작·공백·구두점 가능 · 예약어도 가능(비권장) · 대소문자 구분 · 참조 때마다 따옴표 필요 |
| 공통 금지 | 큰따옴표 `"` · 널 문자 `\0` |
| 대소문자 접힘 | 비인용 = **대문자로 해석**(DB 문자셋 규칙 = `UPPER` 상당, `NLS_UPPER` 아님 · 예 `ß`는 `SS`로 안 바뀜) · 인용 = 그대로 |
| 늘 대문자·인용 무시 | DB 이름 · 전역 DB 이름 · DB 링크 이름 · 디스크 그룹 · PDB 이름 |
| `ROWID` 예외 | 대문자 `ROWID`는 인용해도 컬럼 이름 불가 · `"Rowid"`/`"rowid"`는 가능 |
| 피할 접두 | `SYS_` · `ORA_`(내부 사용) |
| 네임스페이스(스키마 안, 공유) | 테이블 · 뷰 · 시퀀스 · 사설 시노님 · 독립 프로시저 · 독립 함수 · 패키지 · 사용자 정의 연산자 · 사용자 정의 타입 → **서로 같은 이름 불가** |
| 네임스페이스(스키마 안, 각자) | 클러스터 · 제약 · DB 트리거 · 차원 · 인덱스 · MV(내부 동명 테이블 생성 → 테이블과 동명 불가) · 사설 DB 링크 |
| 네임스페이스(DB 전역, 각자) | 에디션 · PFILE/SPFILE · 프로파일 · 공용 DB 링크 · 공용 시노님 · 테이블스페이스 · 롤 |

```sql
CREATE TABLE "employees" (...);  -- 셋 다 별개 객체
CREATE TABLE "Employees" (...);
CREATE TABLE "EMPLOYEES" (...);
SELECT * FROM employees;          -- = "EMPLOYEES"
```

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Database-Object-Names-and-Qualifiers.html

#### T-2-2 리터럴

| 종류 | 문법 | 비고 |
|---|---|---|
| 문자열 | `'abc'` · 안의 작은따옴표 = `''`(`'Jackie''s'`) | 최대 4000바이트(`MAX_STRING_SIZE=STANDARD`) / 32767(EXTENDED) · 비교는 CHAR 의미(공백 채움) |
| 국가 문자셋 | `N'…'` / `n'…'` | NCHAR/NVARCHAR2 |
| 대체 인용 | `q'X…X'` · `nq'X…X'` · 구분자 = 공백·탭·개행 외 아무 글자 · 짝 괄호 `[]` `{}` `<>` `()`는 짝으로 닫음 | `q'[It's]'` · `q'{SELECT 'a' FROM dual;}'` |
| 정수 | `7` `+255` | 최대 38자리 |
| NUMBER | `25` `+6.34` `0.5` `25e-03` `-1` | 지수 −130~125 · 소수점은 늘 `.`(NLS 무관) |
| BINARY_FLOAT / DOUBLE | 접미 `f`/`F` · `d`/`D`(`25f` `0.5d`) | 접미는 숫자 리터럴에서만(`'9f'` 변환은 오류) |
| 특수 부동소수 | `BINARY_FLOAT_NAN` `BINARY_FLOAT_INFINITY` `BINARY_DOUBLE_NAN` `BINARY_DOUBLE_INFINITY` | |
| DATE | `DATE '1998-12-25'`(ANSI · 시각 없음) · `TO_DATE('…','fmt')` | DATE는 늘 시각 포함 → 비교는 `TRUNC`/범위 |
| TIMESTAMP | `TIMESTAMP '1997-01-31 09:26:50.124'`(소수 9자리까지) | |
| TIMESTAMP WITH TIME ZONE | `TIMESTAMP '1997-01-31 09:26:56.66 +02:00'` · `… US/Pacific` · `… US/Pacific PDT` | WITH LOCAL TIME ZONE은 리터럴 없음 |
| INTERVAL YEAR TO MONTH | `INTERVAL '123-2' YEAR(3) TO MONTH` · `INTERVAL '50' MONTH` | 선행 정밀도 기본 2(`INTERVAL '123' YEAR` = 오류) |
| INTERVAL DAY TO SECOND | `INTERVAL '4 5:12:10.222' DAY TO SECOND(3)` · `INTERVAL '11:20' HOUR TO MINUTE` · `INTERVAL '30.12345' SECOND(2,4)` | 소수 초 기본 6 |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Literals.html

#### T-2-3 주석·종결자·바인드·치환·NULL

| 항목 | 내용 |
|---|---|
| 블록 주석 | `/* … */` 여러 줄 · 키워드·매개변수·구두점 사이 어디든 · SQL*Plus는 기본적으로 여러 줄 주석 안 빈 줄 불허 |
| 줄 주석 | `-- …` 줄 끝까지 |
| 중첩 주석 | 문서 명시 없음 — 첫 `*/`에서 끝남(중첩 불가) = **추정** |
| 힌트 | `/*+ … */` · `--+ …`(한 줄) · `+`는 구분자 바로 뒤(공백 없음) · 블록 첫 `SELECT`/`INSERT`/`UPDATE`/`DELETE`/`MERGE` 바로 뒤 하나만 · 틀린 힌트는 조용히 무시 |
| 문장 종결자(서버) | 서버로 보내는 SQL 텍스트에는 종결자 없음(`;`를 붙이면 ORA-00911) = **추정**(OCI 관행) · PL/SQL 블록은 `END;`까지가 문장 |
| 종결자(SQL*Plus) | SQL = `;` 또는 빈 줄 뒤 `/` · PL/SQL 블록(`DECLARE`/`BEGIN`/`CREATE PROCEDURE…`) = `END;` 다음 **줄 맨 앞 `/` 단독** · `/` 단독 = 버퍼 재실행 |
| 바인드 변수 | `:name` · `:1`(위치) — 서버가 해석 · SQL*Plus `VARIABLE x NUMBER` → `:x` |
| 치환 변수(SQL*Plus, 클라이언트) | `&v` = 값 묻고 버림(두 번 쓰면 두 번 물음) · `&&v` = 한 번 묻고 보관 · `DEFINE v = …`/`UNDEFINE v` · 연결 문자 `.`(`&v.txt` · `&v..log`) · `SET DEFINE OFF` · `SET ESCAPE \` · `SET VERIFY OFF` · `&1` `&2` = 스크립트 인자 · 값은 CHAR · 상한 2048개·240바이트 · 치환은 파싱 전 텍스트 치환(첫 토큰 불가 SP2-0734) |
| 빈 문자열 | **길이 0 문자값 = NULL**(`'' IS NULL` 참) · 향후 바뀔 수 있다고 문서가 경고 |
| `||`와 NULL | 연결은 NULL 피연산자를 무시(`'a'||NULL = 'a'`) — 유일하게 NULL 전파 안 하는 연산자 |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Comments.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Nulls.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/About-SQL-Operators.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/21/sqpug/using-substitution-variables-sqlplus.html (19c 판 URL 404 → 21c 판으로 대조 · 19c와 동일 = **추정**)

---

### T-3 예약어

#### T-3-1 SQL 예약어(공식 표 · **110개** · `*` = ANSI 예약어이기도 함 60개)

| 예약어 |
|---|
| ACCESS · ADD · ALL\* · ALTER\* · AND\* · ANY\* · AS\* · ASC · AUDIT · BETWEEN\* · BY\* · CHAR\* · CHECK\* · CLUSTER · COLUMN\* · COLUMN_VALUE(주1) · COMMENT · COMPRESS · CONNECT\* · CREATE\* · CURRENT\* · DATE\* · DECIMAL\* · DEFAULT\* · DELETE\* · DESC · DISTINCT\* · DROP\* · ELSE\* · EXCLUSIVE · EXISTS\* · FILE · FLOAT\* · FOR\* · FROM\* · GRANT\* · GROUP\* · HAVING\* · IDENTIFIED · IMMEDIATE · IN\* · INCREMENT · INDEX · INITIAL · INSERT\* · INTEGER\* · INTERSECT\* · INTO\* · IS\* · LEVEL · LIKE\* · LOCK · LONG · MAXEXTENTS · MINUS · MLSLABEL · MODE · MODIFY · NESTED_TABLE_ID(주1) · NOAUDIT · NOCOMPRESS · NOT\* · NOWAIT · NULL\* · NUMBER · OF\* · OFFLINE · ON\* · ONLINE · OPTION · OR\* · ORDER\* · PCTFREE · PRIOR · PUBLIC · RAW · RENAME · RESOURCE · REVOKE\* · ROW\* · ROWID(주2) · ROWNUM · ROWS\* · SELECT\* · SESSION · SET\* · SHARE · SIZE · SMALLINT\* · START\* · SUCCESSFUL · SYNONYM · SYSDATE · TABLE\* · THEN\* · TO\* · TRIGGER\* · UID · UNION\* · UNIQUE\* · UPDATE\* · USER\* · VALIDATE · VALUES\* · VARCHAR\* · VARCHAR2 · VIEW · WHENEVER\* · WHERE\* · WITH\* |

- 주1: `COLUMN_VALUE`·`NESTED_TABLE_ID` = 속성 이름으로만 예약.
- 주2: 대문자 `ROWID`는 인용해도 컬럼 이름 불가(T-2-1).
- `SYS_` 접두 이름 = 시스템 생성 객체용 예약.
- **예약어 vs 키워드**: 예약어 = 비인용 식별자로 못 씀. 키워드(`V$RESERVED_WORDS`에 `RESERVED='N'`로 나오는 수백 개 — 예 `DUAL`, 자료형·함수 이름)는 식별자로 쓸 수 있으나 비권장. 실제 서버의 전체 목록은 `SELECT keyword, reserved, res_type, res_attr, res_semi FROM V$RESERVED_WORDS` (열 이름 = **추정**, refrn 미대조).

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Oracle-SQL-Reserved-Words.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-RESERVED_WORDS.html (뷰 존재는 문서 체계상 확실 · 열 세부 = **추정**)

#### T-3-2 PL/SQL 예약어(공식 표 D-1 · **85개**)

| 글자 | 예약어 |
|---|---|
| A | ALL ALTER AND ANY AS ASC AT |
| B | BEGIN BETWEEN BY |
| C | CASE CHECK CLUSTERS CLUSTER COLAUTH COLUMNS COMPRESS CONNECT CRASH CREATE CURSOR |
| D | DECLARE DEFAULT DESC DISTINCT DROP |
| E | ELSE END EXCEPTION EXCLUSIVE |
| F | FETCH FOR FROM FUNCTION |
| G | GOTO GRANT GROUP |
| H | HAVING |
| I | IDENTIFIED IF IN INDEX INDEXES INSERT INTERSECT INTO IS |
| L | LIKE LOCK |
| M | MINUS MODE |
| N | NOCOMPRESS NOT NOWAIT NULL |
| O | OF ON OPTION OR ORDER OVERLAPS |
| P | PROCEDURE PUBLIC |
| R | RESOURCE REVOKE |
| S | SELECT SHARE SIZE SQL START SUBTYPE |
| T | TABAUTH TABLE THEN TO TYPE |
| U | UNION UNIQUE UPDATE |
| V | VALUES VIEW VIEWS |
| W | WHEN WHERE WITH |

- 위 표 합계 = 85(글자별 개수 합으로 확인).
- PL/SQL **키워드**(표 D-2 · 약 250개 · 예 `CONSTANT` `ELSIF` `FORALL` `INDICES` `PRAGMA` `RESULT_CACHE` `PARTITION`) = 식별자로 쓸 수 있으나 비권장. 개수는 수기 집계 = **추정**.

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/plsql-reserved-words-keywords.html

---

### T-4 자료형 전체

#### T-4-1 SQL 내장 자료형(`DUMP` 코드 포함)

| 코드 | 자료형 | 인자·범위·최대 |
|---|---|---|
| 1 | `VARCHAR2(size [BYTE\|CHAR])` | size 필수 · 최대 4000바이트(`MAX_STRING_SIZE=STANDARD` 기본) / 32767(EXTENDED) |
| 1 | `NVARCHAR2(size)` | 국가 문자셋 · 최대 4000/32767바이트(글자 수는 AL16UTF16 ×2, UTF8 ×3 기준) |
| 96 | `CHAR[(size [BYTE\|CHAR])]` | 고정 길이 · 기본 1 · 최대 2000바이트 |
| 96 | `NCHAR[(size)]` | 고정 길이 국가 문자셋 · 최대 2000바이트 |
| 2 | `NUMBER[(p[,s])]` | p 1~38 · s −84~127 · 크기 1.0E−130 ~ 1.0E126 미만 · 1~22바이트 · 정밀도 초과 = 오류, 스케일 초과 = 반올림 |
| 2 | `FLOAT[(p)]` | NUMBER 하위 · 이진 정밀도 1~126 |
| 100 | `BINARY_FLOAT` | 32비트 IEEE · 4바이트 |
| 101 | `BINARY_DOUBLE` | 64비트 IEEE · 8바이트 |
| 8 | `LONG` | 가변 문자 2GB(2^31−1) · 하위 호환용(신규 금지) |
| 12 | `DATE` | BC 4712-01-01 ~ 9999-12-31 · 7바이트 · 초까지(소수 초·시간대 없음) |
| 180 | `TIMESTAMP[(fsp)]` | fsp 0~9(기본 6) · 7 또는 11바이트 |
| 181 | `TIMESTAMP[(fsp)] WITH TIME ZONE` | 13바이트 |
| 231 | `TIMESTAMP[(fsp)] WITH LOCAL TIME ZONE` | DB 시간대로 정규화 저장 · 세션 시간대로 표시 |
| 182 | `INTERVAL YEAR[(yp)] TO MONTH` | yp 0~9(기본 2) · 5바이트 |
| 183 | `INTERVAL DAY[(dp)] TO SECOND[(fsp)]` | dp 0~9(기본 2) · fsp 0~9(기본 6) · 11바이트 |
| 23 | `RAW(size)` | size 필수 · 최대 2000(STANDARD) / 32767(EXTENDED) |
| 24 | `LONG RAW` | 이진 2GB · 하위 호환용 |
| 69 | `ROWID` | 물리 행 주소(base64 표기) · `DBMS_ROWID`로 분해 |
| 208 | `UROWID[(size)]` | 논리 행 주소(IOT·외부 테이블) · 기본·최대 4000바이트 |
| 112 | `CLOB` | DB 문자셋 · 최대 (4GB−1) × 블록 크기 |
| 112 | `NCLOB` | 국가 문자셋 · 같음 |
| 113 | `BLOB` | 이진 · 같음 |
| 114 | `BFILE` | DB 밖 파일 읽기 전용 로케이터 · 표 = 4GB / 본문 = 2^64−1(OS 한도) |
| — | `JSON` | **19c 없음**(21c 도입) · 19c는 `VARCHAR2`/`CLOB`/`BLOB` + `IS JSON` 검사 제약 = **추정**(21c 신기능 기준) |
| — | `XMLType` · `URIType`(`DBURIType` `XDBURIType` `HTTPURIType`) | Oracle 제공 타입(CLOB·바이너리 XML·객체 관계 저장) |
| — | `ANYTYPE` · `ANYDATA` · `ANYDATASET` | Oracle 제공 타입(이형 값) |
| — | `SDO_GEOMETRY` 등 공간 · 미디어 타입 | Oracle 제공(Spatial) — 세부 미대조 = **추정** |

- `MAX_STRING_SIZE=EXTENDED`: 4000바이트 초과 값은 내부 LOB로 줄 밖 저장(`DBMS_LOB` 조작 불가) · 전환 시 객체 무효화 가능.

#### T-4-2 ANSI·DB2 별칭 → Oracle 실제 형

| ANSI 이름 | Oracle 형 |
|---|---|
| `CHARACTER(n)` `CHAR(n)` | `CHAR(n)` |
| `CHARACTER VARYING(n)` `CHAR VARYING(n)` | `VARCHAR2(n)` |
| `NATIONAL CHARACTER(n)` `NATIONAL CHAR(n)` `NCHAR(n)` | `NCHAR(n)` |
| `NATIONAL CHARACTER VARYING(n)` `NATIONAL CHAR VARYING(n)` `NCHAR VARYING(n)` | `NVARCHAR2(n)` |
| `NUMERIC[(p,s)]` `DECIMAL[(p,s)]` | `NUMBER(p,s)`(s 기본 0) |
| `INTEGER` `INT` `SMALLINT` | `NUMBER(38)` |
| `FLOAT` · `DOUBLE PRECISION` | `FLOAT(126)` |
| `REAL` | `FLOAT(63)` |
| (SQL/DS·DB2) `VARCHAR(n)` · `LONG VARCHAR` | `VARCHAR(n)`(= VARCHAR2 동의) · `LONG` |
| (대응 없음) | `GRAPHIC` `VARGRAPHIC` `LONG VARGRAPHIC` `TIME` |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Data-Types.html

#### T-4-3 PL/SQL 전용·사용자 정의

| 형 | 정의(STANDARD 패키지) / 내용 |
|---|---|
| `BOOLEAN` | `TRUE`/`FALSE`/`NULL` · **19c SQL에서 쓸 수 없음**(컬럼·SQL 함수 인자·SELECT 대상 불가 · SQL에서 부른 PL/SQL 함수의 인자·익명 블록 바인드는 예외) |
| `BINARY_INTEGER` | `INTEGER range -2147483647..2147483647` |
| `PLS_INTEGER` | `= BINARY_INTEGER`(동일) · 넘침 = 예외 ORA-01426 **추정** |
| `SIMPLE_INTEGER` | `BINARY_INTEGER NOT NULL` · 넘침 시 감싸 돌기(예외 없음) = **추정**(네이티브 컴파일 최적화) |
| `NATURAL` / `NATURALN` | 0..2147483647 / + NOT NULL |
| `POSITIVE` / `POSITIVEN` | 1..2147483647 / + NOT NULL |
| `SIGNTYPE` | −1..1 |
| `VARCHAR2` (PL/SQL 변수) | 최대 32767바이트 = **추정**(lnpls 하위 절 미대조 · 널리 알려진 값) |
| `REF CURSOR` · `SYS_REFCURSOR` | 커서 변수(강·약 타입) |
| `SUBTYPE n IS base [RANGE a..b] [NOT NULL]` | 사용자 정의 하위형 |
| `%TYPE` · `%ROWTYPE` | 컬럼·변수 형 / 행 레코드 형 고정 |
| `RECORD` · 연관 배열(`INDEX BY`) | PL/SQL 전용 컬렉션·레코드 |
| SQL 사용자 정의 형 | `CREATE TYPE … AS OBJECT`(속성+메서드) · `REF` 형(`IS [NOT] DANGLING`) · `VARRAY(n) OF` · `TABLE OF`(중첩 테이블 = 별도 저장 테이블) |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/plsql-predefined-data-types.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/plsql-data-types.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/21/lnpls/expression.html (BOOLEAN SQL 제약 — 21c 판으로 대조)

---

### T-5 연산자·우선순위·NULL·정렬·콜레이션

#### T-5-1 연산자 우선순위(높음 → 낮음)

| 순위 | 연산자 | 뜻 |
|---|---|---|
| 1 | 단항 `+` `-` · `PRIOR` · `CONNECT_BY_ROOT` · `COLLATE` | 부호·계층 위치·콜레이션 |
| 2 | `*` `/` | 곱·나눗셈 |
| 3 | 이항 `+` `-` · `\|\|` | 덧셈·뺄셈·연결(같은 순위) |
| 4 | `=` `!=`(`<>` `^=`) `<` `>` `<=` `>=` | 비교 |
| 5 | `IS [NOT] NULL` · `LIKE` · `[NOT] BETWEEN` · `[NOT] IN` · `EXISTS` · `IS OF type` | 비교 조건 |
| 6 | `NOT` | 논리 부정 |
| 7 | `AND` | |
| 8 | `OR` | |
| 집합 | `UNION` `UNION ALL` `INTERSECT` `MINUS` | 모두 같은 순위 · 왼쪽부터(괄호로 변경) |

- 같은 순위는 왼쪽→오른쪽이나 `AND`/`OR` 여러 조건의 평가 순서는 보장 안 됨.
- `<>`·`^=`·`~=` 동의어 = **추정**(Comparison Conditions 절 미대조).
- `LIKE` 변형: `LIKE` `LIKEC` `LIKE2` `LIKE4` · 정규식 조건 `REGEXP_LIKE` = **추정**(Pattern-matching 절 미대조).
- 멀티셋 연산자: `MULTISET EXCEPT|INTERSECT|UNION [ALL|DISTINCT]` = **추정**.

#### T-5-2 NULL 규칙

| 규칙 | 내용 |
|---|---|
| 산술 | NULL이 끼면 결과 NULL(연결 `\|\|` 제외) |
| 비교 | `= NULL`·`!= NULL` → UNKNOWN · 판정은 `IS [NOT] NULL`만 |
| UNKNOWN | WHERE에서 FALSE처럼 동작 · `NOT UNKNOWN` = UNKNOWN |
| `DECODE` | NULL끼리 같다고 봄 |
| 복합 키 | NULL 아닌 부분이 같으면 같은 키로 봄 |
| 빈 문자열 | `''` = NULL |
| NULL 함수 | `NVL` `NVL2` `COALESCE` `NULLIF` `LNNVL` `NANVL` |

#### T-5-3 정렬·콜레이션

| 항목 | 내용 |
|---|---|
| NULL 정렬 기본 | `ASC` → **NULLS LAST** · `DESC` → **NULLS FIRST** · `NULLS FIRST|LAST`로 지정 |
| ORDER BY 한도 | 식 최대 255개 · `ORDER SIBLINGS BY` = 계층 질의에서만 |
| 언어 정렬 | `NLS_SORT`(BINARY · 언어명 · `_CI` 대소문자 무시 · `_AI` 악센트 무시) · `NLS_COMP`(BINARY · LINGUISTIC · ANSI) = **추정**(globalization 가이드 미대조) |
| 12.2 콜레이션 절 | 컬럼·테이블·스키마 `DEFAULT COLLATION` · 식 `expr COLLATE name` · `ALTER SESSION SET DEFAULT_COLLATION = …`(COMPATIBLE ≥ 12.2 + `MAX_STRING_SIZE=EXTENDED` 필요) · 함수 `COLLATION` `NLS_COLLATION_ID` `NLS_COLLATION_NAME` |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/About-SQL-Operators.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/About-SQL-Conditions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Nulls.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SELECT.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/ALTER-SESSION.html

---

### T-6 내장 함수 전수

#### T-6-1 단일 행 함수(sqlrf 분류 그대로)

| 분류 | 함수 |
|---|---|
| 숫자 | ABS ACOS ASIN ATAN ATAN2 BITAND CEIL COS COSH EXP FLOOR LN LOG MOD NANVL POWER REMAINDER ROUND(number) SIGN SIN SINH SQRT TAN TANH TRUNC(number) WIDTH_BUCKET |
| 문자 → 문자 | CHR CONCAT INITCAP LOWER LPAD LTRIM NCHR NLS_INITCAP NLS_LOWER NLS_UPPER NLSSORT REGEXP_REPLACE REGEXP_SUBSTR REPLACE RPAD RTRIM SOUNDEX SUBSTR TRANSLATE TRANSLATE…USING TRIM UPPER |
| 문자 → 숫자 | ASCII INSTR LENGTH REGEXP_COUNT REGEXP_INSTR |
| 문자셋 | NLS_CHARSET_DECL_LEN NLS_CHARSET_ID NLS_CHARSET_NAME |
| 콜레이션 | COLLATION NLS_COLLATION_ID NLS_COLLATION_NAME |
| 날짜·시각 | ADD_MONTHS CURRENT_DATE CURRENT_TIMESTAMP DBTIMEZONE EXTRACT(datetime) FROM_TZ LAST_DAY LOCALTIMESTAMP MONTHS_BETWEEN NEW_TIME NEXT_DAY NUMTODSINTERVAL NUMTOYMINTERVAL ORA_DST_AFFECTED ORA_DST_CONVERT ORA_DST_ERROR ROUND(date) SESSIONTIMEZONE SYS_EXTRACT_UTC SYSDATE SYSTIMESTAMP TO_CHAR(datetime) TO_DSINTERVAL TO_TIMESTAMP TO_TIMESTAMP_TZ TO_YMINTERVAL TRUNC(date) TZ_OFFSET |
| 일반 비교 | GREATEST LEAST |
| 변환 | ASCIISTR BIN_TO_NUM CAST CHARTOROWID COMPOSE CONVERT DECOMPOSE HEXTORAW NUMTODSINTERVAL NUMTOYMINTERVAL RAWTOHEX RAWTONHEX ROWIDTOCHAR ROWIDTONCHAR SCN_TO_TIMESTAMP TIMESTAMP_TO_SCN TO_BINARY_DOUBLE TO_BINARY_FLOAT TO_BLOB(bfile/raw) TO_CHAR(bfile/blob · character · datetime · number) TO_CLOB(bfile/blob · character) TO_DATE TO_DSINTERVAL TO_LOB TO_MULTI_BYTE TO_NCHAR(character · datetime · number) TO_NCLOB TO_NUMBER TO_SINGLE_BYTE TO_TIMESTAMP TO_TIMESTAMP_TZ TO_YMINTERVAL TREAT UNISTR VALIDATE_CONVERSION |
| LOB | BFILENAME EMPTY_BLOB EMPTY_CLOB |
| 컬렉션 | CARDINALITY COLLECT POWERMULTISET POWERMULTISET_BY_CARDINALITY SET |
| 계층 | SYS_CONNECT_BY_PATH |
| 데이터 마이닝 | CLUSTER_DETAILS CLUSTER_DISTANCE CLUSTER_ID CLUSTER_PROBABILITY CLUSTER_SET FEATURE_COMPARE FEATURE_DETAILS FEATURE_ID FEATURE_SET FEATURE_VALUE ORA_DM_PARTITION_NAME PREDICTION PREDICTION_BOUNDS PREDICTION_COST PREDICTION_DETAILS PREDICTION_PROBABILITY PREDICTION_SET |
| XML | DEPTH EXISTSNODE EXTRACT(XML) EXTRACTVALUE PATH SYS_DBURIGEN SYS_XMLAGG SYS_XMLGEN XMLAGG XMLCAST XMLCDATA XMLCOLATTVAL XMLCOMMENT XMLCONCAT XMLDIFF XMLELEMENT XMLEXISTS XMLFOREST XMLISVALID XMLPARSE XMLPATCH XMLPI XMLQUERY XMLROOT XMLSEQUENCE XMLSERIALIZE XMLTABLE XMLTRANSFORM |
| JSON | JSON_QUERY JSON_TABLE JSON_VALUE JSON_ARRAY JSON_ARRAYAGG JSON_OBJECT JSON_OBJECTAGG JSON_DATAGUIDE (조건: `IS [NOT] JSON` · `JSON_EXISTS` · `JSON_TEXTCONTAINS` = **추정**) |
| 인코딩·디코딩 | DECODE DUMP ORA_HASH STANDARD_HASH VSIZE |
| NULL 관련 | COALESCE LNNVL NANVL NULLIF NVL NVL2 |
| 환경·식별자 | CON_DBID_TO_ID CON_GUID_TO_ID CON_NAME_TO_ID CON_UID_TO_ID ORA_INVOKING_USER ORA_INVOKING_USERID SYS_CONTEXT SYS_GUID SYS_TYPEID UID USER USERENV |

#### T-6-2 집계·분석·기타

| 분류 | 함수 |
|---|---|
| 집계(56) | APPROX_COUNT APPROX_COUNT_DISTINCT APPROX_COUNT_DISTINCT_AGG APPROX_COUNT_DISTINCT_DETAIL APPROX_MEDIAN APPROX_PERCENTILE APPROX_PERCENTILE_AGG APPROX_PERCENTILE_DETAIL APPROX_RANK APPROX_SUM AVG COLLECT CORR CORR_\*(CORR_S · CORR_K) COUNT COVAR_POP COVAR_SAMP CUME_DIST DENSE_RANK FIRST GROUP_ID GROUPING GROUPING_ID JSON_ARRAYAGG JSON_OBJECTAGG LAST LISTAGG MAX MEDIAN MIN PERCENT_RANK PERCENTILE_CONT PERCENTILE_DISC RANK REGR_\*(REGR_SLOPE · INTERCEPT · COUNT · R2 · AVGX · AVGY · SXX · SYY · SXY) STATS_BINOMIAL_TEST STATS_CROSSTAB STATS_F_TEST STATS_KS_TEST STATS_MODE STATS_MW_TEST STATS_ONE_WAY_ANOVA STATS_T_TEST_\*(ONE · PAIRED · INDEP · INDEPU) STATS_WSR_TEST STDDEV STDDEV_POP STDDEV_SAMP SUM SYS_OP_ZONE_ID SYS_XMLAGG TO_APPROX_COUNT_DISTINCT TO_APPROX_PERCENTILE VAR_POP VAR_SAMP VARIANCE XMLAGG |
| 분석(46 · `*` = 창 절 가능) | AVG\* CLUSTER_DETAILS CLUSTER_DISTANCE CLUSTER_ID CLUSTER_PROBABILITY CLUSTER_SET CORR\* COUNT\* COVAR_POP\* COVAR_SAMP\* CUME_DIST DENSE_RANK FEATURE_DETAILS FEATURE_ID FEATURE_SET FEATURE_VALUE FIRST FIRST_VALUE\* LAG LAST LAST_VALUE\* LEAD LISTAGG MAX\* MIN\* NTH_VALUE\* NTILE PERCENT_RANK PERCENTILE_CONT PERCENTILE_DISC PREDICTION PREDICTION_COST PREDICTION_DETAILS PREDICTION_PROBABILITY PREDICTION_SET RANK RATIO_TO_REPORT REGR_\*\* ROW_NUMBER STDDEV\* STDDEV_POP\* STDDEV_SAMP\* SUM\* VAR_POP\* VAR_SAMP\* VARIANCE\* |
| 객체 참조 | DEREF MAKE_REF REF REFTOHEX VALUE = **추정**(해당 분류 페이지 미대조) |
| 모델 | CV ITERATION_NUMBER PRESENTNNV PRESENTV PREVIOUS = **추정** |
| OLAP | CUBE_TABLE = **추정** |
| 데이터 카트리지 | DATAOBJ_TO_MAT_PARTITION DATAOBJ_TO_PARTITION = **추정** |

분석 절: `OVER ([PARTITION BY …] [ORDER BY … [ASC|DESC] [NULLS FIRST|LAST]] [{ROWS|RANGE} BETWEEN {UNBOUNDED PRECEDING|CURRENT ROW|expr PRECEDING|expr FOLLOWING} AND {…|UNBOUNDED FOLLOWING}])` · 기본 창 = `RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW` · 분석 함수는 SELECT 목록·ORDER BY에만 · 중첩 불가. (`GROUPS` 단위·`EXCLUDE`는 21c = **추정**.)

#### T-6-3 자주 쓰는 함수 시그니처

| 함수 | 시그니처 |
|---|---|
| NVL / NVL2 | `NVL(e1, e2)` · `NVL2(e1, not_null_val, null_val)` |
| DECODE | `DECODE(expr, search1, result1 [, search2, result2 …] [, default])` |
| CASE | `CASE expr WHEN v THEN r … [ELSE d] END` · `CASE WHEN cond THEN r … END` |
| SUBSTR / INSTR | `SUBSTR(s, pos [, len])`(pos 음수 = 끝에서) · `INSTR(s, sub [, pos [, nth]])` |
| TRIM | `TRIM([[LEADING|TRAILING|BOTH] [ch] FROM] s)` · `LTRIM(s [, set])` · `RTRIM` |
| LPAD / RPAD | `LPAD(s, n [, pad])` |
| REPLACE / TRANSLATE | `REPLACE(s, from [, to])` · `TRANSLATE(s, from, to)` |
| REGEXP_* | `REGEXP_SUBSTR(s, pat [, pos [, occ [, match [, subexpr]]]])` · `REGEXP_REPLACE(s, pat [, rep [, pos [, occ [, match]]]])` · `REGEXP_LIKE(s, pat [, match])`(조건) |
| TO_CHAR / TO_DATE / TO_NUMBER | `TO_CHAR(x [, fmt [, 'nlsparam']])` · `TO_DATE(s [DEFAULT v ON CONVERSION ERROR] [, fmt [, nls]])` · `TO_NUMBER(s [DEFAULT … ON CONVERSION ERROR] [, fmt [, nls]])`(DEFAULT 절 = 12.2) |
| CAST | `CAST(expr AS type [DEFAULT v ON CONVERSION ERROR] [, fmt [, nls]])` · `CAST(MULTISET(subq) AS coll_type)` |
| ADD_MONTHS 등 | `ADD_MONTHS(d, n)` · `MONTHS_BETWEEN(d1, d2)` · `TRUNC(d [, fmt])` · `ROUND(d [, fmt])` · `LAST_DAY(d)` · `NEXT_DAY(d, 'day')` · `EXTRACT(YEAR FROM d)` |
| LISTAGG | `LISTAGG([ALL|DISTINCT] expr [, sep] [ON OVERFLOW {ERROR | TRUNCATE ['…'] [WITH|WITHOUT COUNT]}]) WITHIN GROUP (ORDER BY …) [OVER (PARTITION BY …)]`(DISTINCT = 19c) |
| SYS_CONTEXT | `SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA' | 'SESSION_USER' | 'DB_NAME' | 'SID' …)` |
| ROW_NUMBER 등 | `ROW_NUMBER() OVER (…)` · `RANK()` · `DENSE_RANK()` · `LAG(e [, off [, default]]) [IGNORE NULLS] OVER (…)` |
| JSON_VALUE | `JSON_VALUE(doc, '$.path' [RETURNING type] [ERROR|NULL|DEFAULT v ON ERROR])` |

(시그니처는 각 함수 절 형식에 따른 정리 — 개별 페이지 전수 대조는 안 함 = 세부 옵션은 **추정**.)

#### T-6-4 주요 Oracle 제공 PL/SQL 패키지

| 패키지 | 주요 서브프로그램 |
|---|---|
| DBMS_OUTPUT | ENABLE(buffer_size) DISABLE PUT PUT_LINE NEW_LINE GET_LINE GET_LINES |
| DBMS_SQL | OPEN_CURSOR PARSE BIND_VARIABLE BIND_ARRAY DEFINE_COLUMN DEFINE_ARRAY EXECUTE EXECUTE_AND_FETCH FETCH_ROWS COLUMN_VALUE VARIABLE_VALUE DESCRIBE_COLUMNS(2·3) CLOSE_CURSOR IS_OPEN LAST_ERROR_POSITION LAST_ROW_COUNT TO_REFCURSOR TO_CURSOR_NUMBER **RETURN_RESULT** **GET_NEXT_RESULT**(공식 대조 31종) |
| DBMS_LOB | GETLENGTH READ WRITE WRITEAPPEND APPEND SUBSTR INSTR COPY ERASE TRIM CREATETEMPORARY FREETEMPORARY OPEN CLOSE LOADFROMFILE LOADCLOBFROMFILE LOADBLOBFROMFILE COMPARE |
| DBMS_METADATA | GET_DDL(object_type, name, schema …) GET_DEPENDENT_DDL GET_GRANTED_DDL SET_TRANSFORM_PARAM OPEN/FETCH_*/CLOSE |
| DBMS_STATS | GATHER_TABLE_STATS GATHER_SCHEMA_STATS GATHER_DATABASE_STATS GATHER_INDEX_STATS DELETE_*_STATS LOCK_TABLE_STATS SET_TABLE_PREFS EXPORT/IMPORT_*_STATS |
| DBMS_XPLAN | DISPLAY(plan_table) DISPLAY_CURSOR(sql_id, child, format) DISPLAY_AWR DISPLAY_SQL_PLAN_BASELINE |
| DBMS_SCHEDULER | CREATE_JOB DROP_JOB RUN_JOB STOP_JOB ENABLE DISABLE SET_ATTRIBUTE CREATE_PROGRAM CREATE_SCHEDULE |
| DBMS_LOCK | SLEEP(18c부터 `DBMS_SESSION.SLEEP` 권장) REQUEST RELEASE CONVERT ALLOCATE_UNIQUE |
| DBMS_SESSION | SET_CONTEXT SET_ROLE SET_IDENTIFIER RESET_PACKAGE MODIFY_PACKAGE_STATE SLEEP |
| DBMS_APPLICATION_INFO | SET_MODULE SET_ACTION SET_CLIENT_INFO |
| DBMS_UTILITY | FORMAT_ERROR_STACK FORMAT_ERROR_BACKTRACE FORMAT_CALL_STACK GET_TIME COMPILE_SCHEMA NAME_RESOLVE |
| DBMS_ERRLOG | CREATE_ERROR_LOG(dml_table_name …) |
| DBMS_ROWID | ROWID_OBJECT ROWID_BLOCK_NUMBER ROWID_ROW_NUMBER ROWID_RELATIVE_FNO |
| DBMS_RANDOM | VALUE STRING NORMAL SEED |
| DBMS_CRYPTO | HASH MAC ENCRYPT DECRYPT RANDOMBYTES |
| UTL_FILE | FOPEN FCLOSE GET_LINE PUT_LINE PUT PUTF FFLUSH FREMOVE FRENAME FCOPY IS_OPEN |
| UTL_RAW / UTL_I18N / UTL_HTTP / UTL_SMTP | CAST_TO_RAW·CONCAT / STRING_TO_RAW / REQUEST·BEGIN_REQUEST / OPEN_CONNECTION |

(DBMS_SQL 목록 = 공식 대조 · 나머지 패키지의 서브프로그램 목록 = **추정**(arpls 각 장 미대조).)

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Functions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Single-Row-Functions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Aggregate-Functions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Analytic-Functions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_SQL.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/ (패키지 목록 진입점)

---

### T-7 질의 문법

#### T-7-1 SELECT 절 순서

```
[WITH {plsql_declarations | subquery_factoring | subav_factoring} …]
SELECT [/*+ hint */] [DISTINCT | UNIQUE | ALL] select_list
FROM table_reference | join_clause | inline_analytic_view [, …]
[WHERE cond]
[hierarchical_query_clause]           -- [START WITH c] CONNECT BY [NOCYCLE] … (순서 바뀜 허용)
[GROUP BY expr | ROLLUP(…) | CUBE(…) | GROUPING SETS(…)] [HAVING cond]   -- HAVING은 GROUP BY 앞도 가능
[model_clause]
[{UNION [ALL] | INTERSECT | MINUS} query_block …]
[ORDER [SIBLINGS] BY expr [ASC|DESC] [NULLS FIRST|LAST] …]
[row_limiting_clause]
[FOR UPDATE [OF col …] [NOWAIT | WAIT n | SKIP LOCKED]]
```
- `WINDOW` 절 없음(창은 `OVER(…)` 안에서만) · `UNIQUE` = `DISTINCT` 동의 · 19c `FROM` 생략 불가(`FROM DUAL`) — `FROM` 생략은 23ai = **추정**.

#### T-7-2 절별 요약

| 기능 | 문법 · 제약 |
|---|---|
| 힌트 | `/*+ … */` 블록 첫 키워드 바로 뒤 하나 |
| ROWNUM | 의사 컬럼 · `WHERE ROWNUM <= n`(ORDER BY보다 먼저 매겨짐 → 정렬 뒤 자르려면 인라인 뷰) · `ROWNUM > 1` 단독은 0행 = **추정**(ROWNUM 절 미대조) |
| 행 제한(12c+) | `[OFFSET n {ROW|ROWS}] [FETCH {FIRST|NEXT} [n | p PERCENT] {ROW|ROWS} {ONLY|WITH TIES}]` · `WITH TIES`는 ORDER BY 필요 · `FOR UPDATE`와 함께 불가 · 선택 목록에 `CURRVAL`/`NEXTVAL` 불가 · 동명 컬럼 = ORA-00918 |
| 계층 질의 | `START WITH cond CONNECT BY [NOCYCLE] PRIOR a = b` · 의사 컬럼 `LEVEL` `CONNECT_BY_ISLEAF` `CONNECT_BY_ISCYCLE`(NOCYCLE 시) · 연산자 `CONNECT_BY_ROOT` · 함수 `SYS_CONNECT_BY_PATH(col, sep)` · 정렬은 `ORDER SIBLINGS BY` |
| 재귀 WITH(11gR2+) | `WITH q (c1, c2) AS (anchor UNION ALL recursive)` · 컬럼 별칭 필수 · `SEARCH {BREADTH|DEPTH} FIRST BY c SET ord` · `CYCLE c SET mark TO 'Y' DEFAULT 'N'`(없으면 순환 시 오류) |
| WITH 함수(12c+) | `WITH FUNCTION f … BEGIN … END; SELECT …` · 비최상위·DML에서는 `WITH_PLSQL` 힌트 필요 |
| PIVOT / UNPIVOT | `PIVOT [XML] (agg(col) [AS a] FOR col IN (v1 [AS a1], …))` · `UNPIVOT [INCLUDE|EXCLUDE NULLS] (val FOR name IN (c1 AS 'x', …))` |
| MODEL | `MODEL [PARTITION BY …] DIMENSION BY (…) MEASURES (…) [RULES …] (cell = expr …)` |
| MATCH_RECOGNIZE(12c+) | `MATCH_RECOGNIZE (PARTITION BY … ORDER BY … MEASURES … {ONE ROW | ALL ROWS} PER MATCH AFTER MATCH SKIP … PATTERN (…) DEFINE …)` |
| 집합 연산 | `UNION` `UNION ALL` `INTERSECT` `MINUS`(`EXCEPT` 없음 = 21c 추가 **추정**) · 열 수·형 일치 · 이름 = 첫 질의 · 왼쪽부터 |
| ANSI 조인 | `[INNER] JOIN … ON|USING` · `{LEFT|RIGHT|FULL} [OUTER] JOIN` · `CROSS JOIN` · `NATURAL [..] JOIN` · 분할 외부 조인 `PARTITION BY (expr)`(FULL 불가) · USING 외부 조인 열 = `COALESCE(a,b)` |
| APPLY / LATERAL(12c+) | `CROSS APPLY` · `OUTER APPLY`(오른쪽 = 테이블 참조·`TABLE(coll)` · 좌상관) · `LATERAL (subquery)`(PIVOT·UNPIVOT·행 패턴과 함께 불가) |
| 옛 외부 조인 `(+)` | WHERE에서만(또는 `TABLE` 좌상관) · **같은 질의 블록에 ANSI 조인 섞기 불가** · 여러 조건이면 모두 `(+)` · `OR` 결합 불가 · `IN`으로 비교 불가 · 자기 자신과 외부 조인 불가 · 상수 비교 열도 `(+)` 필요 · 12c부터 한 테이블이 여러 테이블의 null 생성 측 가능 · Oracle 권장 = ANSI |
| SAMPLE | `FROM t SAMPLE [BLOCK] (pct) [SEED (n)]` · 0.000001 < pct < 100 · SEED 0~4294967295 |
| 파티션 지정 | `t PARTITION (p)` · `SUBPARTITION (sp)` · `PARTITION FOR (key)` |
| 플래시백 | `AS OF {SCN|TIMESTAMP} expr` · `VERSIONS BETWEEN {SCN|TIMESTAMP} a AND b`(MINVALUE/MAXVALUE) · `AS OF PERIOD FOR` · AS OF + FOR UPDATE 불가 · VERSIONS = 뷰·임시·외부·클러스터 테이블 불가 |
| CONTAINERS | `CONTAINERS(t)`(CDB 루트·공통 사용자) |
| FOR UPDATE | 최상위 SELECT만 · `OF col`(실제 컬럼 이름 · 그 테이블 행만 잠금) · `NOWAIT` · `WAIT n`(초) · `SKIP LOCKED`(큐) · 지정 없으면 무한 대기 · DISTINCT·집합 연산·GROUP BY·집계·CURSOR 식·행 제한과 함께 불가 · 뷰에는 비권장(ORA-01733 예) |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SELECT.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Joins.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Comments.html

---

### T-8 DML

| 문장 | 문법 · 요점 |
|---|---|
| INSERT 단일 | `INSERT [hint] INTO t [alias] [(cols)] {VALUES (expr|DEFAULT, …) | subquery} [RETURNING … INTO …] [LOG ERRORS …]` · **VALUES는 한 행만**(다중 행 VALUES = 23ai = **추정**) · 여러 행은 `INSERT ALL` 또는 `INSERT … SELECT … UNION ALL` |
| INSERT 다중 테이블 | `INSERT {ALL|FIRST} [WHEN c THEN] INTO t1 [(cols)] [VALUES (…)] … [ELSE INTO …] subquery` · ALL = 참인 WHEN 모두 · FIRST = 첫 참만 · WHEN 최대 127 · 무조건 = `INSERT ALL INTO … INTO … SELECT …` · 대상 = 테이블만(뷰·MV·원격 불가) · **시퀀스 사용 불가**(NEXTVAL 한 번 평가) · RETURNING 불가 |
| 직접 경로 | `/*+ APPEND */`(INSERT…SELECT) · `/*+ APPEND_VALUES */` · 고수위 위 기록 · 커밋 전 같은 테이블 조회 = ORA-12838 · 트리거·FK 있으면 조용히 일반 INSERT |
| RETURNING | `RETURNING expr, … INTO :b1, …` · 여러 행 = 바인드 배열 / PL/SQL `BULK COLLECT INTO` · 다중 테이블 INSERT·병렬 DML·원격·LONG·INSTEAD OF 뷰 불가 · UPDATE/DELETE는 단일 집합 집계 허용 |
| UPDATE | `UPDATE [hint] {t | view | (subquery)} [alias] SET c = expr|DEFAULT, (c1, c2) = (subquery), VALUE(a) = … [WHERE …] [RETURNING …] [LOG ERRORS …]` · **FROM 절 없음** → 상관 서브쿼리 또는 **갱신 가능 조인 뷰**(키 보존 테이블의 열만 = **추정**, Admin Guide) · 서브쿼리 0행 = NULL 대입 · 가상 열 불가 · 파티션 키 이동 = `ENABLE ROW MOVEMENT` 필요 |
| DELETE | `DELETE [hint] [FROM] {t | view | (subquery)} [alias] [WHERE …] [RETURNING …] [LOG ERRORS …]`(`FROM` 생략 가능 = **추정**, DELETE 페이지 미대조) |
| MERGE | `MERGE INTO t USING src ON (cond) WHEN MATCHED THEN UPDATE SET … [WHERE …] [DELETE WHERE …] WHEN NOT MATCHED THEN INSERT (…) VALUES (…) [WHERE …] [LOG ERRORS …]` · 둘 중 하나 이상 · `DELETE WHERE` = 갱신 뒤 값으로 평가 · ON 절 열 갱신 불가 · 대상 행은 문장당 한 번만 갱신(소스 중복 = ORA-30926 = **추정**) · `ON (0=1)` = 무조건 INSERT |
| TRUNCATE | `TRUNCATE TABLE t [{PRESERVE|PURGE} MATERIALIZED VIEW LOG] [{DROP [ALL]|REUSE} STORAGE] [CASCADE]` · **DDL**(암시 커밋 · 롤백 불가) = **추정**(TRUNCATE 페이지 미대조) |
| 오류 기록 | `LOG ERRORS [INTO [schema.]err_t] [(tag)] [REJECT LIMIT {n | UNLIMITED}]` · 기본 표 이름 `ERR$_` + 대상 이름 앞 25자 · `DBMS_ERRLOG.CREATE_ERROR_LOG` · 기본 한도 0 · 기록 안 되는 오류 = 지연 제약 위반·직접 경로 유일성·UPDATE/MERGE 유일성 · LONG·LOB·객체 열 추적 불가 · `UNLIMITED` = ORA-30645 문구로 확인 |
| 갱신 가능 뷰 조건 | 집합 연산·DISTINCT·집계·분석·GROUP BY·ORDER BY·MODEL·CONNECT BY·선택 목록 서브쿼리·`WITH READ ONLY`·재귀 WITH 없음 · 한 번에 기반 테이블 하나 · 아니면 `INSTEAD OF` 트리거 |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/INSERT.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/UPDATE.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/MERGE.html
출처: https://docs.oracle.com/en/error-help/db/ora-30645 (REJECT LIMIT 0~100000 또는 UNLIMITED)
출처(미대조 참고): https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/DELETE.html · https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/TRUNCATE-TABLE.html

---

### T-9 객체 종류 전수 표

공통: `OR REPLACE` = 뷰·코드 객체·시노님·타입 등 일부 · **`IF [NOT] EXISTS` = 19c 없음**(23ai · 19.28 백포트 = 2차 자료 **추정**) · 이름 = `[schema.]name[@dblink]` · 상태 `ALL_OBJECTS.STATUS` = `VALID`/`INVALID`/`N/A` · 의존 = `ALL_DEPENDENCIES`(`DBA_`/`USER_`) · DDL 원문 = `DBMS_METADATA.GET_DDL('TYPE','NAME','SCHEMA')` · 코드 원문 = `ALL_SOURCE(OWNER,NAME,TYPE,LINE,TEXT)` · 컴파일 오류 = `ALL_ERRORS`.

| 객체 | CREATE / ALTER / DROP 핵심 | OR REPLACE | 네임스페이스·이름 | 상태 | 오버로드·식별 키 | 사전 뷰 | 원문 |
|---|---|---|---|---|---|---|---|
| TABLE(힙) | `CREATE TABLE t (c type [DEFAULT e] [constraint] …) [TABLESPACE …] [PARTITION BY …]` · `ALTER TABLE ADD/MODIFY/DROP/RENAME COLUMN, ADD/DROP/ENABLE/DISABLE CONSTRAINT, MOVE, SHRINK` · `DROP TABLE t [CASCADE CONSTRAINTS] [PURGE]`(휴지통 `FLASHBACK TABLE … TO BEFORE DROP`) | 없음 | 공유 NS | N/A(테이블) | — | ALL_TABLES · ALL_TAB_COLUMNS(ALL_TAB_COLS 숨은 열 포함) · ALL_TAB_COMMENTS · ALL_COL_COMMENTS · ALL_PART_TABLES | GET_DDL('TABLE') |
| 전역 임시 테이블 | `CREATE GLOBAL TEMPORARY TABLE … ON COMMIT {DELETE|PRESERVE} ROWS` | 없음 | 공유 NS | — | — | ALL_TABLES(`TEMPORARY='Y'`) | GET_DDL |
| 개인 임시 테이블(18c+) | `CREATE PRIVATE TEMPORARY TABLE ora$ptt_x … ON COMMIT {DROP DEFINITION|PRESERVE DEFINITION}` · 접두 = `PRIVATE_TEMP_TABLE_PREFIX`(기본 `ORA$PTT_`) · 메모리·세션 한정 | 없음 | 세션 | — | — | USER_PRIVATE_TEMP_TABLES = **추정** | 없음 **추정** |
| 파티션 테이블 | `PARTITION BY {RANGE|LIST|HASH} (…) [INTERVAL (…)] [SUBPARTITION BY …]` · 참조·시스템 파티션 · `ALTER TABLE ADD/DROP/SPLIT/MERGE/EXCHANGE/TRUNCATE PARTITION` | — | 파티션 = 하위 객체(`SUBOBJECT_NAME`) | — | — | ALL_PART_TABLES · ALL_TAB_PARTITIONS · ALL_PART_KEY_COLUMNS | GET_DDL |
| IOT | `CREATE TABLE … (… PRIMARY KEY …) ORGANIZATION INDEX [OVERFLOW]` | — | 공유 NS | — | — | ALL_TABLES(`IOT_TYPE`) | GET_DDL |
| 외부 테이블 | `ORGANIZATION EXTERNAL (TYPE ORACLE_LOADER|ORACLE_DATAPUMP DEFAULT DIRECTORY d ACCESS PARAMETERS (…) LOCATION ('f'))` | — | 공유 NS | — | — | ALL_EXTERNAL_TABLES · ALL_EXTERNAL_LOCATIONS | GET_DDL |
| VIEW | `CREATE [OR REPLACE] [[NO] FORCE] [EDITIONING] VIEW v [(cols)] AS subq [WITH CHECK OPTION | WITH READ ONLY]` · `ALTER VIEW v COMPILE` · `DROP VIEW v [CASCADE CONSTRAINTS]` | 있음 | 공유 NS | VALID/INVALID | — | ALL_VIEWS(`TEXT` LONG · `TEXT_VC`) | GET_DDL('VIEW') |
| MATERIALIZED VIEW | `CREATE MATERIALIZED VIEW mv [BUILD IMMEDIATE|DEFERRED] [REFRESH {FAST|COMPLETE|FORCE} ON {DEMAND|COMMIT}] [ENABLE QUERY REWRITE] AS subq` · `CREATE MATERIALIZED VIEW LOG ON t` · `DBMS_MVIEW.REFRESH` | 없음 | 자체 NS(동명 내부 테이블 → 테이블과 동명 불가) | VALID/INVALID + `STALENESS` | — | ALL_MVIEWS · ALL_MVIEW_LOGS | GET_DDL('MATERIALIZED_VIEW') |
| SEQUENCE | `CREATE SEQUENCE s [START WITH n] [INCREMENT BY n] [MINVALUE|NOMINVALUE] [MAXVALUE] [CYCLE|NOCYCLE] [CACHE n|NOCACHE] [ORDER] [SESSION|GLOBAL]` · `s.NEXTVAL`/`s.CURRVAL` · `ALTER SEQUENCE`(12c+ `RESTART` = **추정**) | 없음 | 공유 NS | N/A | — | ALL_SEQUENCES(`LAST_NUMBER`) | GET_DDL('SEQUENCE') |
| IDENTITY 열(12c+) | `c NUMBER GENERATED {ALWAYS | BY DEFAULT [ON NULL]} AS IDENTITY [(seq opts)]` · 내부 시퀀스 `ISEQ$$_n` | — | 열 속성 | — | — | ALL_TAB_IDENTITY_COLS · ALL_TAB_COLUMNS(`IDENTITY_COLUMN`) | 테이블 DDL 안 |
| INDEX | `CREATE [UNIQUE|BITMAP] INDEX i ON t (c [ASC|DESC] | expr …) [REVERSE] [LOCAL|GLOBAL PARTITION BY …] [INVISIBLE] [ONLINE]` · 함수 기반 = 식 · 도메인 = `INDEXTYPE IS …` · `ALTER INDEX REBUILD [ONLINE]/UNUSABLE/VISIBLE` · `DROP INDEX i [ONLINE]` | 없음 | 자체 NS(테이블과 동명 가능) | `STATUS` VALID/UNUSABLE/N/A | — | ALL_INDEXES · ALL_IND_COLUMNS · ALL_IND_EXPRESSIONS | GET_DDL('INDEX') |
| 제약 | `[CONSTRAINT n] {PRIMARY KEY | UNIQUE | REFERENCES t(c) [ON DELETE {CASCADE|SET NULL}] | CHECK (cond) | NOT NULL}` + `[NOT] DEFERRABLE [INITIALLY {IMMEDIATE|DEFERRED}]` + `{ENABLE|DISABLE} [VALIDATE|NOVALIDATE]` · `ALTER TABLE … {ADD|DROP|ENABLE|DISABLE|RENAME} CONSTRAINT` · `SET CONSTRAINTS ALL DEFERRED` · 이름 없으면 `SYS_Cnnnn` | — | 자체 NS(스키마 단위) | `STATUS` ENABLED/DISABLED · `VALIDATED` | `CONSTRAINT_TYPE` P/U/R/C(NOT NULL = C)/V/O | ALL_CONSTRAINTS · ALL_CONS_COLUMNS | GET_DDL('CONSTRAINT'/'REF_CONSTRAINT') |
| TRIGGER | `CREATE [OR REPLACE] [EDITIONABLE] TRIGGER tr {BEFORE|AFTER|INSTEAD OF} {INSERT|UPDATE [OF c]|DELETE} ON t [REFERENCING …] [FOR EACH ROW] [FOLLOWS|PRECEDES …] [ENABLE|DISABLE] [WHEN (cond)] body` · DDL(`ON SCHEMA|DATABASE` `CREATE`/`ALTER`/`DROP`…) · DB 이벤트(`LOGON` `LOGOFF` `STARTUP` `SHUTDOWN` `SERVERERROR`) · **복합 트리거** `COMPOUND TRIGGER … BEFORE STATEMENT IS … AFTER EACH ROW IS … END` · `ALTER TRIGGER tr {ENABLE|DISABLE|COMPILE}` | 있음 | 자체 NS | VALID/INVALID + ENABLED/DISABLED | — | ALL_TRIGGERS(`TRIGGER_BODY` LONG) · ALL_TRIGGER_COLS | ALL_SOURCE(TYPE='TRIGGER') · GET_DDL |
| PROCEDURE | `CREATE [OR REPLACE] [EDITIONABLE|NONEDITIONABLE] PROCEDURE p [(params)] [AUTHID …] {IS|AS} …` · `ALTER PROCEDURE p COMPILE [DEBUG]` · `DROP PROCEDURE p` | 있음 | 공유 NS | VALID/INVALID | **독립 루틴 오버로드 불가** | ALL_PROCEDURES · ALL_ARGUMENTS | ALL_SOURCE · GET_DDL('PROCEDURE') |
| FUNCTION | 위 + `RETURN type [DETERMINISTIC] [PIPELINED] [PARALLEL_ENABLE] [RESULT_CACHE]` · 외부(`AS LANGUAGE C|JAVA`) · 집계(`AGGREGATE USING`) | 있음 | 공유 NS | VALID/INVALID | 독립 = 불가 | ALL_PROCEDURES(`PIPELINED` `AGGREGATE` `DETERMINISTIC`…) · ALL_ARGUMENTS | ALL_SOURCE |
| PACKAGE / PACKAGE BODY | `CREATE [OR REPLACE] PACKAGE pk [AUTHID …] [ACCESSIBLE BY (…)] {IS|AS} 선언 END;` · `CREATE [OR REPLACE] PACKAGE BODY pk …` · `ALTER PACKAGE pk COMPILE [PACKAGE|SPECIFICATION|BODY]` · `DROP PACKAGE [BODY] pk` | 있음 | 공유 NS(본문은 같은 이름) | 명세·본문 따로 VALID/INVALID | **패키지 안 오버로드 가능** · 식별 = `(OBJECT_NAME=패키지, PROCEDURE_NAME, SUBPROGRAM_ID)` 또는 `ALL_ARGUMENTS(PACKAGE_NAME, OBJECT_NAME, OVERLOAD)`(오버로드 없으면 OVERLOAD NULL · 소스 출현 순 번호) | ALL_PROCEDURES(패키지 행 = PROCEDURE_NAME NULL) · ALL_ARGUMENTS | ALL_SOURCE(TYPE='PACKAGE'/'PACKAGE BODY') · GET_DDL('PACKAGE_SPEC'/'PACKAGE_BODY') |
| TYPE / TYPE BODY | `CREATE [OR REPLACE] TYPE t [FORCE] {AS OBJECT (…) [NOT] FINAL [NOT] INSTANTIABLE | UNDER super (…) | AS VARRAY(n) OF … | AS TABLE OF …}` · `CREATE TYPE BODY` · `ALTER TYPE … ADD ATTRIBUTE … CASCADE` · `DROP TYPE t [FORCE|VALIDATE]` | 있음(의존 있으면 제약) | 공유 NS | VALID/INVALID | 메서드 오버로드 가능 | ALL_TYPES · ALL_TYPE_ATTRS · ALL_TYPE_METHODS · ALL_COLL_TYPES | ALL_SOURCE · GET_DDL('TYPE') |
| SYNONYM | `CREATE [OR REPLACE] [EDITIONABLE] [PUBLIC] SYNONYM s FOR [schema.]obj[@dblink]` · `DROP [PUBLIC] SYNONYM s [FORCE]` | 있음 | 사설 = 공유 NS · 공용 = DB 전역 NS(소유 `PUBLIC`) | VALID/INVALID | — | ALL_SYNONYMS(`TABLE_OWNER` `TABLE_NAME` `DB_LINK`) | GET_DDL('SYNONYM') |
| DATABASE LINK | `CREATE [SHARED] [PUBLIC] DATABASE LINK l CONNECT TO u IDENTIFIED BY p USING 'tns'` · `ALTER DATABASE LINK` · `DROP [PUBLIC] DATABASE LINK l` · 참조 `t@l` | 없음 | 사설 = 자체 NS · 공용 = 전역 · 이름 늘 대문자 | — | — | ALL_DB_LINKS | GET_DDL('DB_LINK') |
| DIRECTORY | `CREATE [OR REPLACE] DIRECTORY d AS '/path'` · `DROP DIRECTORY d` · 권한 READ/WRITE/EXECUTE | 있음 | 비스키마(소유 SYS) | — | — | ALL_DIRECTORIES | GET_DDL('DIRECTORY') |
| USER / SCHEMA | `CREATE USER u IDENTIFIED BY p [DEFAULT TABLESPACE …] [QUOTA …] [PROFILE …]` · `ALTER USER` · `DROP USER u [CASCADE]` · `CREATE SCHEMA AUTHORIZATION u CREATE TABLE … GRANT …`(묶음 실행, 새 스키마 만들지 않음) · 스키마 = 사용자와 1:1 · 스키마 전용 계정 `NO AUTHENTICATION`(18c+ = **추정**) | 없음 | 비스키마 | `ACCOUNT_STATUS` | — | ALL_USERS · DBA_USERS | GET_DDL('USER') |
| ROLE | `CREATE ROLE r [NOT IDENTIFIED | IDENTIFIED BY p | IDENTIFIED USING pkg | IDENTIFIED EXTERNALLY|GLOBALLY]` · `SET ROLE` · `DROP ROLE` | 없음 | 비스키마 전역 | — | — | DBA_ROLES · ROLE_SYS_PRIVS · ROLE_TAB_PRIVS · USER_ROLE_PRIVS | GET_DDL('ROLE') |
| JOB(스케줄러) | `DBMS_SCHEDULER.CREATE_JOB(job_name, job_type, job_action, start_date, repeat_interval, enabled …)` · 구식 `DBMS_JOB` | (프로시저) | 스키마 객체(`JOB`) | ENABLED · `STATE` | — | ALL_SCHEDULER_JOBS · ALL_SCHEDULER_JOB_RUN_DETAILS | GET_DDL('PROCOBJ') **추정** |
| LIBRARY | `CREATE [OR REPLACE] LIBRARY lib AS '/path/lib.so' [AGENT …]` | 있음 | 스키마 | VALID/INVALID | — | ALL_LIBRARIES | GET_DDL('LIBRARY') |
| CONTEXT | `CREATE [OR REPLACE] CONTEXT ns USING [schema.]pkg [INITIALIZED {EXTERNALLY|GLOBALLY} | ACCESSED GLOBALLY]` · 값 = `SYS_CONTEXT('ns','attr')` | 있음 | 비스키마(소유 SYS) | — | — | DBA_CONTEXT · ALL_CONTEXT | GET_DDL('CONTEXT') |
| EDITION | `CREATE EDITION e [AS CHILD OF p]` · `ALTER SESSION SET EDITION = e` · `DROP EDITION e [CASCADE]` · 에디션 가능 객체 = 뷰·시노님·PL/SQL 코드·타입 등 | 없음 | 비스키마 전역 NS | `USABLE` | — | ALL_EDITIONS · ALL_OBJECTS(`EDITION_NAME`·`EDITIONABLE`) | — |
| CLUSTER | `CREATE CLUSTER c (k type) [SIZE n] [HASHKEYS n]` + `CREATE INDEX ON CLUSTER c` | 없음 | 자체 NS | — | — | ALL_CLUSTERS | GET_DDL('CLUSTER') |
| 기타 | DIMENSION · OPERATOR · INDEXTYPE · JAVA SOURCE/CLASS/RESOURCE · QUEUE(AQ) · ANALYTIC VIEW · ATTRIBUTE DIMENSION · HIERARCHY · MINING MODEL · TABLESPACE · PROFILE · AUDIT POLICY · FLASHBACK ARCHIVE · RESTORE POINT · LOCKDOWN PROFILE | 일부 | 각자 | 일부 VALID/INVALID | OPERATOR 바인딩 오버로드 | ALL_OBJECTS.OBJECT_TYPE로 열거 | GET_DDL 일부 |

- `ALL_OBJECTS` 열: OWNER OBJECT_NAME SUBOBJECT_NAME OBJECT_ID DATA_OBJECT_ID OBJECT_TYPE CREATED LAST_DDL_TIME TIMESTAMP STATUS TEMPORARY GENERATED SECONDARY NAMESPACE EDITION_NAME SHARING EDITIONABLE ORACLE_MAINTAINED APPLICATION DEFAULT_COLLATION DUPLICATED SHARDED CREATED_APPID CREATED_VSNID MODIFIED_APPID MODIFIED_VSNID. `OBJECT_TYPE` 전체 값 목록은 공식 표 없음 → `SELECT DISTINCT object_type FROM all_objects`.
- `ALL_PROCEDURES` 열: OBJECT_NAME PROCEDURE_NAME OBJECT_ID SUBPROGRAM_ID OVERLOAD OBJECT_TYPE AGGREGATE PIPELINED IMPLTYPEOWNER PARALLEL INTERFACE DETERMINISTIC AUTHID RESULT_CACHE ORIGIN_CON_ID POLYMORPHIC(+ OWNER).
- `ALL_ARGUMENTS` 요점: `POSITION` 0 = 함수 반환값(ARGUMENT_NAME NULL) · `SEQUENCE` 1부터(반환 먼저) · `IN_OUT` = `IN`/`OUT`/`IN/OUT` · `DEFAULTED` · `DATA_LEVEL` = 18c부터 늘 0(복합형은 `ALL_PLSQL_TYPES`/`ALL_PLSQL_TYPE_ATTRS`/`ALL_PLSQL_COLL_TYPES`) · 패키지 지역형 = `TYPE_NAME`(패키지) + `TYPE_SUBNAME`.
- 표의 CREATE/ALTER 세부 옵션 중 개별 페이지를 열지 않은 것(PTT 뷰 이름 · SEQUENCE RESTART · 스키마 전용 계정 · JOB GET_DDL 종류 등)은 **추정**.

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Database-Object-Names-and-Qualifiers.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_OBJECTS.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_PROCEDURES.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_ARGUMENTS.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_ERRORS.html
출처(미대조 진입점): https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SQL-Statements-CREATE-TABLE-to-DROP-CLUSTER.html · https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_METADATA.html

---

### T-10 루틴 세부

| 항목 | 내용 |
|---|---|
| 매개변수 모드 | `IN`(기본 · 상수처럼 · 늘 참조 전달) · `OUT`(지정 필수 · 시작값 = 형 기본값 보통 NULL · 값 전달) · `IN OUT`(값 전달 양방향) |
| `NOCOPY` | OUT/IN OUT에 대한 **힌트**(참조 전달 요청 · 무시될 수 있음 · 별칭·예외 시 결과 불확정) |
| 실인자 | IN = 상수·리터럴·식 가능 · OUT/IN OUT = 변수만 · 처리 안 된 예외로 끝나면 OUT 값 미정의 |
| 기본값 | IN 매개변수만 `DEFAULT e` 또는 `:= e` · 생략 시에만 평가 · 생략 ≠ NULL |
| 표기법 | 위치 `p(1, 2)`(끝쪽 선택 인자만 생략) · 이름 `p(b => 2, a => 1)`(순서 무관·아무 선택 인자 생략) · 혼합 `p(1, b => 2)`(위치 먼저) · **SQL 안 함수 호출에도 이름·혼합 표기 가능** |
| 오버로드 | 패키지·블록 안만 · 독립 루틴 불가 · 이름·모드·반환형만 다르거나 같은 기반형의 하위형만 다르면 불가 = **추정**(lnpls 오버로드 절 미대조) |
| 반환 | 스칼라 · 레코드/컬렉션 · **PIPELINED 테이블 함수**(`PIPE ROW` · `SELECT * FROM TABLE(f(…))` · 19c는 `TABLE()` 생략 가능 = **추정**) · `REF CURSOR` / `SYS_REFCURSOR`(OUT 매개변수 또는 반환) · 다형 테이블 함수(18c+ · `ALL_PROCEDURES.POLYMORPHIC`) |
| 속성 | `DETERMINISTIC` · `PARALLEL_ENABLE` · `RESULT_CACHE` · `ACCESSIBLE BY (…)`(12.2+) · `PRAGMA AUTONOMOUS_TRANSACTION` · `PRAGMA UDF` |
| `AUTHID` | `DEFINER`(기본 · 소유자 권한·이름 해석) · `CURRENT_USER`(호출자 권한) · `ALL_PROCEDURES.AUTHID` |
| 익명 블록 | `[DECLARE …] BEGIN … [EXCEPTION WHEN … THEN …] END;` |
| 블록 종결 | 클라이언트(SQL*Plus 등)에서 줄 맨 앞 `/` 단독 · 서버로는 `BEGIN … END;` 텍스트 그대로(끝 `;` 포함) |
| 호출 | PL/SQL `p(…);` · `BEGIN p(…); END;` · SQL `CALL p(…)`(괄호 필수 · 함수는 `CALL f(…) INTO :x`) · SQL*Plus `EXEC[UTE] p(…)`(= `BEGIN p(…); END;` 줄임) · 함수 = `SELECT f(…) FROM dual` |
| 결과 반환(12c+) | `DBMS_SQL.RETURN_RESULT(rc IN OUT SYS_REFCURSOR | INTEGER, to_client BOOLEAN DEFAULT TRUE)` = 암시 결과 · 받는 쪽 `DBMS_SQL.GET_NEXT_RESULT(c, rc)`(더 없으면 ORA-01403) |
| 컴파일 경고 | `ALTER SESSION SET PLSQL_WARNINGS='ENABLE:ALL'` · 조건부 컴파일 `$IF … $THEN … $END` = **추정** |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/subprogram-parameters.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/plsql-subprograms.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_PROCEDURES.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_SQL.html
출처(미대조): https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CALL.html

---

### T-11 트랜잭션·세션

| 항목 | 내용 |
|---|---|
| 자동 커밋 | 서버에는 자동 커밋 모드 없음 — **클라이언트가 결정**(OCI `OCI_COMMIT_ON_SUCCESS` 실행 모드 · SQL*Plus `SET AUTOCOMMIT` · JDBC 기본 on) = **추정**(OCI 모드 이름은 lnoci 미대조) |
| 트랜잭션 시작 | 첫 DML · `SELECT … FOR UPDATE` · `SET TRANSACTION` · `DBMS_TRANSACTION` 호출로 **암시 시작**(`BEGIN TRANSACTION` 문 없음) |
| 끝 | `COMMIT [WORK] [COMMENT '…'] [WRITE [WAIT|NOWAIT] [IMMEDIATE|BATCH]]` · `COMMIT FORCE 'id'` · `ROLLBACK [WORK] [TO [SAVEPOINT] sp] | FORCE 'id'` |
| DDL 암시 커밋 | **DDL 앞**(문법상 유효하면 오류가 나도) + **성공한 DDL 뒤** 암시 COMMIT → DDL은 롤백 불가 · 열린 DML 트랜잭션이 확정됨 |
| SAVEPOINT | `SAVEPOINT sp` · `ROLLBACK TO SAVEPOINT sp`(이후 세이브포인트 지움 · 트랜잭션 유지) |
| 격리 수준 | `READ COMMITTED`(기본 · 문장 단위 읽기 일관성 · 잠긴 행은 대기) · `SERIALIZABLE`(트랜잭션 시작 시점 스냅숏 · 충돌 시 ORA-08177 = **추정**) · `READ ONLY` 트랜잭션 · READ UNCOMMITTED·REPEATABLE READ 없음 · 읽기는 잠금 안 함(MVCC·undo) |
| SET TRANSACTION | `SET TRANSACTION {READ ONLY | READ WRITE | ISOLATION LEVEL {SERIALIZABLE|READ COMMITTED} | USE ROLLBACK SEGMENT rs} [NAME 'x'(≤255바이트)]` · 트랜잭션의 **첫 문장**이어야 함 |
| 세션 설정 | `ALTER SESSION SET CURRENT_SCHEMA = s`(비한정 이름 해석만 바뀜 · 사용자·권한 불변) · `NLS_DATE_FORMAT` `NLS_TIMESTAMP_FORMAT` `NLS_NUMERIC_CHARACTERS` `NLS_LANGUAGE` `NLS_TERRITORY` `NLS_SORT` `NLS_COMP` · `TIME_ZONE` · `ISOLATION_LEVEL` · `CONSTRAINTS = {IMMEDIATE|DEFERRED|DEFAULT}` · `EDITION` · `CONTAINER`(CDB) · `DEFAULT_COLLATION` · `ROW ARCHIVAL VISIBILITY` |
| 세션 기능 | `ALTER SESSION {ENABLE|DISABLE|FORCE} PARALLEL {DML|DDL|QUERY}` · `{ENABLE|DISABLE} COMMIT IN PROCEDURE` · `CLOSE DATABASE LINK l` · `{ENABLE|DISABLE} RESUMABLE` |
| 잠금 | `LOCK TABLE t IN {ROW SHARE|ROW EXCLUSIVE|SHARE|SHARE ROW EXCLUSIVE|EXCLUSIVE} MODE [NOWAIT|WAIT n]` = **추정**(LOCK TABLE 페이지 미대조) |
| 자율 트랜잭션 | `PRAGMA AUTONOMOUS_TRANSACTION`(독립 커밋) |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SET-TRANSACTION.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/ALTER-SESSION.html
출처(미대조): https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SAVEPOINT.html · https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/LOCK-TABLE.html

---

### T-12 DCL

| 항목 | 문법 · 내용 |
|---|---|
| 시스템 권한 | `GRANT {sys_priv | role} [, …] TO {user | role | PUBLIC} [IDENTIFIED BY p] [WITH ADMIN OPTION | WITH DELEGATE OPTION] [CONTAINER = {CURRENT|ALL}]` · 예 `CREATE SESSION` `CREATE TABLE` `CREATE ANY TABLE` `SELECT ANY TABLE` `UNLIMITED TABLESPACE` · 목록 = `SYSTEM_PRIVILEGE_MAP` = **추정** |
| 객체 권한 | `GRANT {obj_priv [(cols)] | ALL [PRIVILEGES]} [, …] ON {[schema.]obj | USER u | DIRECTORY d | EDITION e | …} TO … [WITH GRANT OPTION | WITH HIERARCHY OPTION]` |
| 옵션 | `WITH ADMIN OPTION` = 시스템 권한·롤을 다시 주고/회수/변경/삭제 · `WITH DELEGATE OPTION` = 롤을 자기 스키마 코드에 부여(CBAC) · `WITH GRANT OPTION` = 객체 권한 재부여(사용자·PUBLIC에만, 롤 불가) · `WITH HIERARCHY OPTION` = READ/SELECT를 하위 뷰까지 · 옵션만 회수 불가(통째 회수 후 재부여) |
| 코드에 롤 | `GRANT role TO [schema.]program_unit`(CBAC) |
| 회수 | `REVOKE … FROM …` · 객체 권한 `CASCADE CONSTRAINTS` · `FORCE`(타입) = **추정** |
| 롤 | `CREATE ROLE` · `SET ROLE {r [IDENTIFIED BY p] | ALL [EXCEPT …] | NONE}` · 기본 롤 `ALTER USER u DEFAULT ROLE …` · 순환 부여 불가 · `IDENTIFIED GLOBALLY` 롤은 GRANT 불가 |
| 미리 정의된 롤 | `DBA`(UNLIMITED TABLESPACE 미포함 · KEEP SEQUENCE 포함) · `CONNECT` · `RESOURCE` 등(내용은 Security Guide = **추정**) |
| 정의자 권한 함정 | 정의자 권한 PL/SQL·뷰 안에서는 **롤로 받은 권한 무효**(직접 부여 필요) = **추정**(Security Guide) |

객체별 객체 권한:

| 객체 | 권한 |
|---|---|
| 테이블 | ALTER DEBUG DELETE INDEX INSERT READ REFERENCES SELECT UPDATE |
| 뷰 | DEBUG DELETE INSERT MERGE VIEW READ REFERENCES SELECT UNDER UPDATE |
| MV | ON COMMIT REFRESH QUERY REWRITE READ SELECT |
| 시퀀스 | ALTER KEEP SEQUENCE SELECT |
| 프로시저·함수·패키지·Java | DEBUG EXECUTE |
| 디렉터리 | READ WRITE EXECUTE |
| 객체 타입 | DEBUG EXECUTE UNDER |
| 라이브러리·인덱스형·연산자 | EXECUTE |
| 에디션 | USE |
| 마이닝 모델 | ALTER SELECT |
| 분석 뷰·계층 | ALTER READ SELECT |
| 스케줄러 객체 | EXECUTE ALTER USE |
| 사용자(`ON USER`) | INHERIT PRIVILEGES · INHERIT REMOTE PRIVILEGES · TRANSLATE SQL |
| 시노님 | 대상 객체와 같음 |

사전 뷰: `ALL_TAB_PRIVS`·`ALL_COL_PRIVS`·`USER_SYS_PRIVS`·`USER_ROLE_PRIVS`·`SESSION_PRIVS`·`SESSION_ROLES`·`DBA_SYS_PRIVS`·`ROLE_TAB_PRIVS` = **추정**(refrn 개별 미대조).

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/GRANT.html
출처(미대조): https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/REVOKE.html · https://docs.oracle.com/en/database/oracle/oracle-database/19/dbseg/

---

### T-13 오류 모델

| 항목 | 내용 |
|---|---|
| 오류 코드 형식 | `ORA-nnnnn: 메시지`(5자리 0 채움) · 접두별 원천: `ORA-` 서버 · `PLS-` PL/SQL 컴파일 · `SP2-` SQL*Plus · `TNS-` Oracle Net · `OCI-` 클라이언트 · `ORA-06512: at "S.P", line n` = PL/SQL 호출 스택 줄 = **추정**(errmg 형식 절 미대조) |
| 사용자 정의 오류 | `RAISE_APPLICATION_ERROR(-20000..-20999, msg [, keep_errors])` · `PRAGMA EXCEPTION_INIT(e, -n)` |
| 미리 정의 예외 | `NO_DATA_FOUND`(ORA-01403 · SQLCODE +100) · `TOO_MANY_ROWS`(ORA-01422) · `DUP_VAL_ON_INDEX`(ORA-00001) · `ZERO_DIVIDE`(ORA-01476) · `INVALID_NUMBER`(ORA-01722) · `VALUE_ERROR`(ORA-06502) · `CURSOR_ALREADY_OPEN` · `INVALID_CURSOR` · `LOGIN_DENIED` · `TIMEOUT_ON_RESOURCE` = **추정**(lnpls 예외 표 미대조) |
| SQLCODE / SQLERRM | 예외 처리기 안 `SQLCODE`(음수 · NO_DATA_FOUND = +100 · 사용자 예외 = +1) · `SQLERRM`(메시지 · 최대 512바이트) · 전체 스택 `DBMS_UTILITY.FORMAT_ERROR_STACK` · 줄 위치 `FORMAT_ERROR_BACKTRACE` · 12c `UTL_CALL_STACK` = **추정** |
| 오류 위치 | SQL 파싱 오류 = 오프셋(OCI `OCI_ATTR_PARSE_ERROR_OFFSET` · `DBMS_SQL.LAST_ERROR_POSITION`) · 컴파일 오류 = 줄·열 |
| 컴파일 오류 조회 | `ALL_ERRORS`/`USER_ERRORS`/`DBA_ERRORS`: OWNER NAME TYPE SEQUENCE **LINE POSITION** TEXT **ATTRIBUTE(ERROR|WARNING)** MESSAGE_NUMBER(접두 없는 번호) · TYPE 값 = ANALYTIC VIEW · ASSEMBLY · ATTRIBUTE DIMENSION · DIMENSION · FUNCTION · HIERARCHY · JAVA CLASS · JAVA SOURCE · LIBRARY · PACKAGE · PACKAGE BODY · PROCEDURE · QUEUE · TRIGGER · TYPE · TYPE BODY · VIEW |
| 컴파일 실패 시 | `CREATE` 자체는 성공 + 객체 INVALID + 경고 **ORA-24344: success with compilation error**(SQL*Plus "Warning: … created with compilation errors.") = **추정** · SQL*Plus `SHOW ERRORS [type name]`(= USER_ERRORS 조회) |
| 오류 문서 | 19c errmg · 통합 오류 도움말 `docs.oracle.com/en/error-help/db/ora-nnnnn` |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_ERRORS.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/errmg/
출처: https://docs.oracle.com/en/error-help/db/ora-30645 (오류 도움말 페이지 형식 예)
출처(미대조): https://docs.oracle.com/en/database/oracle/oracle-database/19/lnpls/plsql-error-handling.html

---

### T-14 클라이언트 인터페이스

| 항목 | 내용 |
|---|---|
| 기본 API | **OCI**(C) · 그 위에 ODPI-C · JDBC Thin(순수 Java, OCI 불필요) · ODP.NET Managed · Instant Client(Basic/Basic Light) = **추정**(제품 구성) |
| 실행 흐름 | `OCIStmtPrepare2` → 바인드(`OCIBindByName`/`OCIBindByPos`) → `OCIStmtExecute` → 정의(`OCIDefineByPos`) → `OCIStmtFetch2` → `OCIStmtRelease`(문장 캐시) |
| `OCIStmtExecute` | `(svchp, stmtp, errhp, ub4 iters, ub4 rowoff, snap_in, snap_out, ub4 mode)` · 비SELECT 실행 횟수 = `iters − rowoff` · SELECT `iters` = 즉시 가져올 행(모르면 0) · 비SELECT `iters=0` 오류 · DDL은 `iters=1`만 |
| 배열 DML | 바인드 배열 + `iters` = 행 수(성능상 ≤ 32767 권장) · `rowoff` = 시작 인덱스 · `OCI_BATCH_ERRORS` = 행 단위 오류 수집(전체 중단 안 함) · `OCI_RETURN_ROW_COUNT_ARRAY` = 행별 영향 수 |
| 바인드 형 | `:name`/`:n` · 외부형 `SQLT_CHR` `SQLT_STR` `SQLT_INT` `SQLT_FLT` `SQLT_VNU` `SQLT_ODT` `SQLT_TIMESTAMP(_TZ/_LTZ)` `SQLT_INTERVAL_YM/DS` `SQLT_BIN` `SQLT_CLOB` `SQLT_BLOB` `SQLT_RSET`(REF CURSOR) `SQLT_BOL`(12.1+ PL/SQL BOOLEAN) = **추정**(데이터형 장 미대조) · 같은 이름 바인드 반복 = SQL은 위치별 · PL/SQL은 이름별 = **추정** |
| LOB | 로케이터 기반 `OCILobRead2`/`OCILobWrite2`/`OCILobGetLength2`/`OCILobCreateTemporary` · 작은 LOB = 데이터 인터페이스(`SQLT_CHR`/`SQLT_BIN`로 직접 바인드·정의) · 프리페치 `OCI_ATTR_DEFAULT_LOBPREFETCH_SIZE` = **추정** |
| 취소 | `OCIBreak(void *hndlp, OCIError *errhp)` = 서버와 연결된 실행 중 OCI 호출의 **즉시(비동기) 종료** · 논블로킹 중 Break 뒤에는 `OCIReset(hndlp, errhp)` 필수 · 취소된 문장 = ORA-01013 = **추정** |
| 생존 확인 | `OCIPing` · 버전 `OCIServerVersion`/`OCIServerRelease2`/`OCIClientVersion` · 오류 `OCIErrorGet` |
| 암시 결과(12c+) | 서버 `DBMS_SQL.RETURN_RESULT(rc)` → 클라이언트 `OCIStmtGetNextResult(stmthp, errhp, void **result, ub4 *rtype, ub4 mode)` 반복(`OCI_NO_DATA` = 끝 · `rtype` = `OCI_RESULT_TYPE_SELECT`) |
| REF CURSOR | OUT 바인드 `SQLT_RSET` + 문장 핸들 → 그 핸들로 정의·페치 |
| 서버 출력 | `DBMS_OUTPUT.ENABLE` → 실행 → `DBMS_OUTPUT.GET_LINES(:arr, :n)`로 클라이언트가 회수(서버 푸시 없음) = **추정** |
| 프리페치 | `OCI_ATTR_PREFETCH_ROWS` / `OCI_ATTR_PREFETCH_MEMORY` = **추정** |

출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnoci/statement-functions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/lnoci/miscellaneous-functions.html
출처: https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_SQL.html
출처(미대조 진입점): https://docs.oracle.com/en/database/oracle/oracle-database/19/lnoci/

### T-15 nexa-sql 지원 현황

방언 열 **ORA**(Oracle) — 항목별 ✅/부분/❌와 코드 자리는 §11 표의 `ORA` 열 · 한 줄 판정은 §11 끝 "요약" · 결함은 §9.

## §3. Microsoft SQL Server 2019+

> 조사 범위 = SQL Server 2019(15.x) · 2022(16.x) · 2025(17.x) 엔진의 T-SQL. Azure SQL Database/MI·Synapse·Fabric 전용 문법은 제외(필요한 곳만 각주).
> 출처 표기 규칙: 각 표 아래 "출처:" 줄. URL 앞 **★** = 이번 조사(2026-10-10)에서 직접 열람해 내용을 대조한 문서 · 표시 없음 = 해당 주제의 공식 문서 위치(내용은 기존 지식 기반이며, 문서와 대조하지 않은 세부 사항은 본문에 "추정"으로 표시).

---

### T-1. 버전 · 호환성 수준 · 에디션 · 문서 위치

| 제품 | 엔진 버전 | 기본 호환성 수준 | 지원 호환성 수준 | 비고 |
|---|---|---|---|---|
| SQL Server 2019 (15.x) | 15 | 150 | 150, 140, 130, 120, 110, 100 | UTF-8 콜레이션(`_UTF8`) · 지능형 쿼리 처리 확장 |
| SQL Server 2022 (16.x) | 16 | 160 | 160 … 100 | `GREATEST`/`LEAST` · `DATETRUNC` · `DATE_BUCKET` · `GENERATE_SERIES` · `WINDOW` 절 · `IS [NOT] DISTINCT FROM` · 비트 조작 함수 · Ledger 테이블 · `JSON_OBJECT`/`JSON_ARRAY`/`JSON_PATH_EXISTS` |
| SQL Server 2025 (17.x) | 17 | 170 | 170 … 100 | 네이티브 `json`·`vector` 타입 · `REGEXP_*` · `||` 연결 · `UNISTR` · `BASE64_ENCODE/DECODE` · `PRODUCT` · `CURRENT_DATE` · `SUBSTRING` 길이 생략 · `JSON_OBJECTAGG`/`JSON_ARRAYAGG` · 퍼지 문자열 함수(미리 보기) |
| Azure SQL Database | 17(내부 번호) | 170 | 170 … 100 | 엔진 번호는 SQL Server와 비교 불가 |

- 호환성 수준은 **데이터베이스 단위**(`ALTER DATABASE … SET COMPATIBILITY_LEVEL = 160`) — 일부 예약어·문법·최적화기 동작이 이 값에 따라 달라진다. 클라이언트가 문법 기능을 판단할 때는 `SERVERPROPERTY('ProductMajorVersion')`(엔진)과 `sys.databases.compatibility_level`(DB) **둘 다** 봐야 한다.
- 에디션(2022 기준): Enterprise · Standard · Web · Express · Developer(무료·개발용) · Evaluation. 2025: **Web 단종** · Developer가 **Standard Developer / Enterprise Developer**로 나뉨 · Express DB 상한 50 GB(종전 10 GB) · Express with Advanced Services 단종. 판별 = `SERVERPROPERTY('Edition')`, `SERVERPROPERTY('EngineEdition')`.
- 2025의 미리 보기 기능(`EDIT_DISTANCE`·`JARO_WINKLER_*`·`VECTOR_SEARCH`·`CREATE VECTOR INDEX` 등)은 `ALTER DATABASE SCOPED CONFIGURATION SET PREVIEW_FEATURES = ON`이 있어야 쓸 수 있다.

| 문서 | URL |
|---|---|
| T-SQL 참조(루트) | https://learn.microsoft.com/en-us/sql/t-sql/language-reference |
| 시스템 카탈로그 뷰 | https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/catalog-views-transact-sql |
| 데이터베이스 엔진 오류 목록 | https://learn.microsoft.com/en-us/sql/relational-databases/errors-events/database-engine-events-and-errors |
| 시스템 저장 프로시저 | https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/system-stored-procedures-transact-sql |

출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/statements/alter-database-transact-sql-compatibility-level
출처: ★ https://learn.microsoft.com/en-us/sql/sql-server/what-s-new-in-sql-server-2025
출처: https://learn.microsoft.com/en-us/sql/sql-server/what-s-new-in-sql-server-2022
출처: https://learn.microsoft.com/en-us/sql/sql-server/editions-and-components-of-sql-server-2022

---

### T-2. 어휘 규칙(식별자 · 리터럴 · 주석 · 구분자)

#### T-2-1. 식별자

| 항목 | 규칙 |
|---|---|
| 길이 | 1~128자(`sysname` = `nvarchar(128)`) · **로컬 임시 테이블은 최대 116자**(나머지는 세션 접미사) |
| 일반 식별자 첫 글자 | 유니코드 3.2 글자 · `_` · `@` · `#` |
| 일반 식별자 이후 글자 | 글자 · 10진 숫자(각국 문자 포함) · `@` `$` `#` `_` |
| 일반 식별자 금지 | 예약어(대소문자 무관) · 공백·특수문자 · 보조 문자(supplementary) |
| 구분 식별자 `[ ]` | 늘 동작(`QUOTED_IDENTIFIER`와 무관) · `]` 이스케이프 = `]]` · `QUOTENAME()`이 만들어 줌 |
| 구분 식별자 `" "` | `SET QUOTED_IDENTIFIER ON`(대부분 연결의 기본)일 때만 식별자 · OFF면 문자열 리터럴 · `"` 이스케이프 = `""` |
| `@name` | 지역 변수·매개변수(다른 개체 이름으로 못 씀) |
| `@@name` | 시스템 함수(옛 "전역 변수") — 사용자 이름으로 쓰지 말 것 |
| `#name` / `##name` | 로컬 임시 테이블·프로시저(세션) / 전역 임시 개체(모든 세션) — `tempdb`에 생성 |
| 대소문자 | **콜레이션에 따름** — 서버 수준 개체(로그인·DB 이름) = 인스턴스 기본 콜레이션 · DB 안 개체(테이블·컬럼) = DB 기본 콜레이션 · 변수 이름 = 인스턴스 콜레이션(추정) · 키워드는 항상 대소문자 무관 |
| 다부분 이름 | `server.database.schema.object`(4부) · 중간 생략 = 점 유지(`db..obj`, `srv...obj`) · server = 연결된 서버 |

#### T-2-2. 리터럴

| 종류 | 형태 | 비고 |
|---|---|---|
| 문자열 | `'abc'` · 작은따옴표 이스케이프 `''` | `QUOTED_IDENTIFIER OFF`면 `"abc"`도 문자열 |
| 유니코드 문자열 | `N'한글'` | 접두 없으면 DB 기본 코드 페이지로 변환(손실 가능) |
| 유니코드 이스케이프 | `UNISTR(N'\00E9')` (2025) | 문자열 리터럴 자체에는 이스케이프 문법 없음 |
| 정수 | `123` | 범위에 따라 int / numeric 추정형 |
| 소수 | `1.5` → decimal · `1.5E3` → float | |
| 이진 | `0x0A1B` | `0x` = 빈 이진값 |
| 통화 | `$12.34` · `¥100` 등 통화 기호 접두 | money 상수 |
| 날짜·시간 | `'2026-10-10'` · `'20261010'` · `'2026-10-10T12:00:00'` | 날짜 전용 리터럴 문법 없음(문자열 → 암시 변환) · `SET DATEFORMAT`/`LANGUAGE` 영향 → ISO 8601 `yyyymmdd`·`yyyy-mm-ddThh:mi:ss` 권장 · ODBC 이스케이프 `{d '2026-10-10'}` `{ts '…'}` 지원 |
| 불리언 | **없음** — `bit` 0/1, `'TRUE'`/`'FALSE'` 문자열은 bit로 변환 가능 · 술어 결과를 값으로 못 씀(`SELECT a=b` 불가 → `IIF`/`CASE`) |
| NULL | `NULL` | |
| GUID | `'6F9619FF-8B86-D011-B42D-00C04FC964FF'` | 문자열 → uniqueidentifier 변환 |

#### T-2-3. 주석 · 구분자 · 매개변수

| 항목 | 규칙 |
|---|---|
| 줄 주석 | `-- …` 줄 끝까지 |
| 블록 주석 | `/* … */` · **중첩 허용**(`/* /* */ */`) — 분할기·하이라이터가 깊이를 세야 함 · 블록 주석은 `GO`를 넘을 수 없음 |
| 배치 구분자 `GO` | **서버 문법이 아님**(SSMS·sqlcmd·클라이언트가 처리) · 단독 줄 · `GO n` = n회 반복 · 구분자 단어는 도구 설정으로 변경 가능 |
| 문장 종결자 `;` | 대부분 생략 가능하나 "향후 필수" 예고 · **필수인 곳** = `WITH`(CTE) 앞 문장 · `MERGE` 문 끝 · `THROW` 앞 문장 · Service Broker `SEND`/`RECEIVE` 앞 등 |
| 매개변수 | `@name`(sp_executesql·RPC) · ODBC/OLE DB 위치 표식 `?`는 드라이버가 변환 · TDS RPC 기본 명명 `@P1, @P2…`(드라이버 관례) |
| sqlcmd 변수 | `$(var)` — sqlcmd·SSMS SQLCMD 모드의 **클라이언트 치환** · `:setvar`, `:r`, `:connect` 등 명령 |
| 레이블 | `label:` + `GOTO label` |

출처: ★ https://learn.microsoft.com/en-us/sql/relational-databases/databases/database-identifiers
출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/language-elements/transact-sql-syntax-conventions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/data-types/constants-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/slash-star-comment-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/sql-server-utilities-statements-go
출처: https://learn.microsoft.com/en-us/sql/tools/sqlcmd/sqlcmd-use-scripting-variables
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/set-quoted-identifier-transact-sql

---

### T-3. 예약어

#### T-3-1. T-SQL 예약어 — 185개(문서 표 그대로 · `WITHIN GROUP`은 한 항목)

ADD, ALL, ALTER, AND, ANY, AS, ASC, AUTHORIZATION, BACKUP, BEGIN, BETWEEN, BREAK, BROWSE, BULK, BY, CASCADE, CASE, CHECK, CHECKPOINT, CLOSE, CLUSTERED, COALESCE, COLLATE, COLUMN, COMMIT, COMPUTE, CONSTRAINT, CONTAINS, CONTAINSTABLE, CONTINUE, CONVERT, CREATE, CROSS, CURRENT, CURRENT_DATE, CURRENT_TIME, CURRENT_TIMESTAMP, CURRENT_USER, CURSOR, DATABASE, DBCC, DEALLOCATE, DECLARE, DEFAULT, DELETE, DENY, DESC, DISK, DISTINCT, DISTRIBUTED, DOUBLE, DROP, DUMP, ELSE, END, ERRLVL, ESCAPE, EXCEPT, EXEC, EXECUTE, EXISTS, EXIT, EXTERNAL, FETCH, FILE, FILLFACTOR, FOR, FOREIGN, FREETEXT, FREETEXTTABLE, FROM, FULL, FUNCTION, GOTO, GRANT, GROUP, HAVING, HOLDLOCK, IDENTITY, IDENTITY_INSERT, IDENTITYCOL, IF, IN, INDEX, INNER, INSERT, INTERSECT, INTO, IS, JOIN, KEY, KILL, LEFT, LIKE, LINENO, LOAD, MERGE, NATIONAL, NOCHECK, NONCLUSTERED, NOT, NULL, NULLIF, OF, OFF, OFFSETS, ON, OPEN, OPENDATASOURCE, OPENQUERY, OPENROWSET, OPENXML, OPTION, OR, ORDER, OUTER, OVER, PERCENT, PIVOT, PLAN, PRECISION, PRIMARY, PRINT, PROC, PROCEDURE, PUBLIC, RAISERROR, READ, READTEXT, RECONFIGURE, REFERENCES, REPLICATION, RESTORE, RESTRICT, RETURN, REVERT, REVOKE, RIGHT, ROLLBACK, ROWCOUNT, ROWGUIDCOL, RULE, SAVE, SCHEMA, SECURITYAUDIT, SELECT, SEMANTICKEYPHRASETABLE, SEMANTICSIMILARITYDETAILSTABLE, SEMANTICSIMILARITYTABLE, SESSION_USER, SET, SETUSER, SHUTDOWN, SOME, STATISTICS, SYSTEM_USER, TABLE, TABLESAMPLE, TEXTSIZE, THEN, TO, TOP, TRAN, TRANSACTION, TRIGGER, TRUNCATE, TRY_CONVERT, TSEQUAL, UNION, UNIQUE, UNPIVOT, UPDATE, UPDATETEXT, USE, USER, VALUES, VARYING, VIEW, WAITFOR, WHEN, WHERE, WHILE, WITH, WITHIN GROUP, WRITETEXT

| 관찰 | 의미(클라이언트 구현) |
|---|---|
| `CURRENT_DATE`가 목록에 있음 | 2025에서 함수가 추가되며 예약 — 호환성 수준·버전별로 다를 수 있음(추정 · 문서는 "예약어는 호환성 수준에 따라 다르다"고만 명시) |
| `INT`, `DATE`, `NAME`, `TYPE`, `STATUS`, `KEY`… | `INT`·`DATE`·`TYPE`은 **예약어 아님**(ODBC/향후 목록에만) · `KEY`는 예약어 → 인용 필요 |
| 변수·매개변수 이름 | 예약어도 제한 없음(`@select` 가능) |
| Synapse 전용 | `LABEL`(SQL Server엔 해당 없음) |

#### T-3-2. 별도 목록(전문은 출처 참조 · 여기서는 성격만)

| 목록 | 성격 | 클라이언트 처리 권장 |
|---|---|---|
| ODBC 예약어(= ISO 예약어 목록) | `ABSOLUTE`, `CAST`, `DATE`, `INT`, `INTEGER`, `TIME`, `TIMESTAMP`, `VALUE`, `ZONE`, `GO` 등 · SQL Server 문법을 제약하지 않음 | 식별자 인용 판정에는 쓰지 않음(경고 수준) |
| 향후 예약어(Future Keywords) | `BOOLEAN`, `LIMIT`, `ROLE`, `ROW`, `ROWS`, `SEQUENCE`, `WINDOW`, `PARTITION`, `RANGE`, `RESULT`, `XML*`, `REGR_*` 등 | 신규 개체 이름으로 피하라고 안내(경고 수준) |

출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/language-elements/reserved-keywords-transact-sql

---

### T-4. 데이터 타입

| 분류 | 타입 | 크기·범위·비고 |
|---|---|---|
| 정확 수치(정수) | `bigint` · `int` · `smallint` · `tinyint` | 8 · 4 · 2 · 1바이트 · **tinyint = 0~255(부호 없음)** |
| 정확 수치 | `bit` | 0/1/NULL · 불리언 대용 · 8개까지 1바이트로 묶어 저장 |
| 정확 수치 | `decimal(p,s)` = `numeric(p,s)` | p 1~38(기본 18) · s 0~p(기본 0) · 5~17바이트 · 두 타입은 동일 |
| 정확 수치 | `money` · `smallmoney` | 8바이트(±922조, 소수 4자리) · 4바이트(±214,748.3648) |
| 근사 수치 | `float(n)` · `real` | n 1~24 → real(4바이트) · 25~53 → float(8바이트, 기본 53) |
| 날짜·시간 | `date` | 0001-01-01~9999-12-31 · 3바이트 |
| | `time(n)` | 00:00:00~23:59:59.9999999 · 100ns · 3~5바이트 · n 0~7 |
| | `datetime` | 1753-01-01~9999-12-31 · 0.00333초(.000/.003/.007 반올림) · 8바이트 |
| | `datetime2(n)` | 0001~9999 · 100ns · 6~8바이트 · 기본 n=7 |
| | `datetimeoffset(n)` | datetime2 + 오프셋 ±14:00 · 8~10바이트 |
| | `smalldatetime` | 1900-01-01~2079-06-06 · 1분 · 4바이트 |
| 문자 | `char(n)` · `varchar(n\|max)` | n 1~8000 바이트 · `max` = 2^31-1 바이트 · 코드 페이지 = 콜레이션 · 2019+ `_UTF8` 콜레이션이면 UTF-8 |
| 유니코드 문자 | `nchar(n)` · `nvarchar(n\|max)` | n 1~4000 **바이트쌍**(UTF-16 코드 단위) · `max` = 2^30-1 문자 |
| 레거시 LOB | `text` · `ntext` · `image` | **사용 중단 예정** → `varchar(max)`/`nvarchar(max)`/`varbinary(max)` |
| 이진 | `binary(n)` · `varbinary(n\|max)` | n 1~8000 · max = 2^31-1 |
| 기타 | `cursor` | 변수·OUTPUT 매개변수 전용(컬럼 불가) |
| | `rowversion` (동의어 `timestamp` — 사용 중단) | 8바이트 · DB 단위 증가 · **날짜 아님** · 테이블당 1개 |
| | `hierarchyid` | CLR 시스템 타입 · 메서드 `GetAncestor()` 등 |
| | `uniqueidentifier` | 16바이트 GUID · `NEWID()`/`NEWSEQUENTIALID()` |
| | `sql_variant` | 최대 8016바이트 · max/xml/json 등 담을 수 없음 |
| | `xml` | 최대 2 GB · 형식화(XML 스키마 컬렉션) 가능 · 메서드 `query/value/exist/modify/nodes` |
| | `geometry` · `geography` | 공간 타입(CLR) |
| | `table` | 변수·TVF 반환 전용 |
| | `json` (**2025**) | 이진 저장(UTF-8) · 최대 2 GB · 객체/배열만 · `.modify()` 메서드 · alias 타입 불가 · **TDS ≥ 7.4 클라이언트에는 `varchar(max)` (Latin1_General_100_BIN2_UTF8)로 보임 · < 7.4 는 `nvarchar(max)`** |
| | `vector(n[, float16])` (**2025**) | 벡터 · JSON 배열로 노출 · 요소 float32(기본) 또는 float16(미리 보기 · 추정) |

| 동의어(ISO) | 실제 타입 |
|---|---|
| `binary varying` | varbinary |
| `char varying` / `character varying(n)` | varchar(n) |
| `character` / `character(n)` | char(1) / char(n) |
| `dec` | decimal |
| `double precision` | float |
| `float(1~7)` / `float(8~15)` | real / float (문서 원문 표기) |
| `integer` | int |
| `national char(n)` / `national character(n)` | nchar(n) |
| `national char varying(n)` / `national character varying(n)` | nvarchar(n) |
| `national text` | ntext |
| `rowversion` | timestamp |

- 동의어는 DDL에서만 의미 → 메타데이터(카탈로그·`sp_help`·결과 메타)에는 **기본 타입만** 남는다.
- 사용자 정의 타입: **alias 타입**(`CREATE TYPE ssn FROM varchar(11) NOT NULL`) · **테이블 타입**(`CREATE TYPE t AS TABLE (…)` → TVP) · **CLR UDT**(`CREATE TYPE … EXTERNAL NAME asm.[ns.class]`). 카탈로그 = `sys.types`(`is_user_defined`, `is_table_type`), `sys.table_types`, `sys.assembly_types`.
- 대형 값 타입(`varchar(max)`·`nvarchar(max)`) / LOB(`text`·`ntext`·`image`·`varbinary(max)`·`xml`) — `sp_help`는 길이 `-1`로 보고.

출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/data-types/data-types-transact-sql
출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/data-types/data-type-synonyms-transact-sql
출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/data-types/json-data-type
출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/functions/date-and-time-data-types-and-functions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/data-types/vector-data-type
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-type-transact-sql

---

### T-5. 연산자 · 우선순위 · NULL · 콜레이션

| 수준(높음→낮음) | 연산자 (2022+) |
|---|---|
| 1 | `~` (비트 NOT) |
| 2 | `*` `/` `%` |
| 3 | 단항 `+` `-` · `+`(덧셈·**문자열 연결**) · `-` · `&` `^` `|` · `<<` `>>`(2022+) |
| 4 | `=` `>` `<` `>=` `<=` `<>` `!=` `!>` `!<` |
| 5 | `NOT` |
| 6 | `AND` |
| 7 | `ALL` `ANY` `BETWEEN` `IN` `LIKE` `OR` `SOME` |
| 8 | `=` (대입) |

- 같은 수준 = 왼쪽→오른쪽. 2019는 `<<`/`>>` 없음.
- 복합 대입: `+=` `-=` `*=` `/=` `%=` `&=` `^=` `|=`.
- 문자열 연결: `+`(피연산자 하나라도 NULL이면 NULL — `CONCAT_NULL_YIELDS_NULL ON` 기본) · `CONCAT(a,b,…)`(NULL → 빈 문자열) · `CONCAT_WS(sep, …)` · **2025 `||`** · 숫자+문자 `+` = 데이터 타입 우선순위로 숫자 변환 시도(오류 흔함).
- 기타 술어: `IS [NOT] NULL` · `IS [NOT] DISTINCT FROM`(2022+, NULL 안전 비교) · `LIKE`(와일드카드 `%` `_` `[a-z]` `[^…]` · `ESCAPE`) · `EXISTS` · `CONTAINS`/`FREETEXT`(전체 텍스트).
- NULL 의미: `SET ANSI_NULLS ON`(기본·향후 OFF 불가 예정) → `= NULL`은 UNKNOWN. OFF면 `= NULL`이 참이 될 수 있음(레거시).
- 정렬: **오름차순에서 NULL이 먼저**(가장 작은 값 취급) · `NULLS FIRST/LAST` 절 **없음**(→ `ORDER BY CASE WHEN c IS NULL THEN 1 ELSE 0 END, c`).
- 나눗셈: 정수/정수 = 정수(버림) · 0으로 나누기 = 오류 8134(`ARITHABORT`/`ANSI_WARNINGS` 영향).
- 콜레이션 절: `expr COLLATE Latin1_General_CS_AS` · 컬럼/DB/서버 기본 → 콜레이션 우선순위 규칙. 이름 접미사 `_CI/_CS`(대소문자) `_AI/_AS`(악센트) `_KS` `_WS` `_SC`(보조 문자) `_UTF8`(2019+) `_BIN/_BIN2`.

출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/language-elements/operator-precedence-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/string-concatenation-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/string-concatenation-pipes-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/set-ansi-nulls-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/is-distinct-from-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/collations

---

### T-6. 내장 함수

#### T-6-1. 범주별 함수 이름(Microsoft 분류)

| 범주 | 함수 |
|---|---|
| 집계 | `AVG` `CHECKSUM_AGG` `COUNT` `COUNT_BIG` `GROUPING` `GROUPING_ID` `MAX` `MIN` `STDEV` `STDEVP` `STRING_AGG`(2017+) `SUM` `VAR` `VARP` `APPROX_COUNT_DISTINCT`(2019+) `APPROX_PERCENTILE_CONT`/`_DISC`(2022+) · `PRODUCT`(2025) · `ANY_VALUE` `APPROX_MEDIAN` `APPROX_QUANTILE` `MEDIAN` `QUANTILE`(문서 목록에 있으나 플랫폼 한정 — SQL Server 지원 여부 **추정 불가·개별 문서 확인 필요**) · `JSON_OBJECTAGG` `JSON_ARRAYAGG`(2025) |
| 분석(윈도) | `CUME_DIST` `FIRST_VALUE` `LAG` `LAST_VALUE` `LEAD` `PERCENTILE_CONT` `PERCENTILE_DISC` `PERCENT_RANK` · 2022+ `IGNORE NULLS`/`RESPECT NULLS` |
| 순위 | `RANK` `DENSE_RANK` `NTILE` `ROW_NUMBER` |
| 비트 조작(2022+) | `LEFT_SHIFT` `RIGHT_SHIFT` `BIT_COUNT` `GET_BIT` `SET_BIT` |
| 콜레이션 | `COLLATIONPROPERTY` `TERTIARY_WEIGHTS` |
| 구성 | `@@DATEFIRST` `@@DBTS` `@@LANGID` `@@LANGUAGE` `@@LOCK_TIMEOUT` `@@MAX_CONNECTIONS` `@@MAX_PRECISION` `@@NESTLEVEL` `@@OPTIONS` `@@REMSERVER` `@@SERVERNAME` `@@SERVICENAME` `@@SPID` `@@TEXTSIZE` `@@VERSION` |
| 변환 | `CAST` `CONVERT(type, expr[, style])` `PARSE` `TRY_CAST` `TRY_CONVERT` `TRY_PARSE` |
| 암호화 | `ENCRYPTBYKEY` `DECRYPTBYKEY` `DECRYPTBYKEYAUTOCERT` `DECRYPTBYKEYAUTOASYMKEY` `ENCRYPTBYPASSPHRASE` `DECRYPTBYPASSPHRASE` `ENCRYPTBYCERT` `DECRYPTBYCERT` `ENCRYPTBYASYMKEY` `DECRYPTBYASYMKEY` `KEY_ID` `KEY_GUID` `KEY_NAME` `SYMKEYPROPERTY` `CERT_ID` `CERTPROPERTY` `CERTENCODED` `CERTPRIVATEKEY` `ASYMKEY_ID` `ASYMKEYPROPERTY` `SIGNBYCERT` `SIGNBYASYMKEY` `VERIFYSIGNEDBYCERT` `VERIFYSIGNEDBYASYMKEY` `IS_OBJECTSIGNED` `HASHBYTES` `CRYPT_GEN_RANDOM` |
| 커서 | `@@CURSOR_ROWS` `@@FETCH_STATUS` `CURSOR_STATUS` |
| 데이터 타입 | `DATALENGTH` `IDENT_CURRENT` `IDENT_INCR` `IDENT_SEED` `IDENTITY`(SELECT INTO 전용) `SQL_VARIANT_PROPERTY` |
| 날짜·시간 | 시스템 시각: `SYSDATETIME` `SYSDATETIMEOFFSET` `SYSUTCDATETIME` `CURRENT_TIMESTAMP` `GETDATE` `GETUTCDATE` `CURRENT_DATE`(2025) · 부분: `DATE_BUCKET`(2022) `DATENAME` `DATEPART` `DATETRUNC`(2022) `DAY` `MONTH` `YEAR` · 구성: `DATEFROMPARTS` `DATETIME2FROMPARTS` `DATETIMEFROMPARTS` `DATETIMEOFFSETFROMPARTS` `SMALLDATETIMEFROMPARTS` `TIMEFROMPARTS` · 차이: `DATEDIFF` `DATEDIFF_BIG` · 변경: `DATEADD` `EOMONTH` `SWITCHOFFSET` `TODATETIMEOFFSET` · 검증: `ISDATE` · 식: `expr AT TIME ZONE 'Korea Standard Time'` |
| 그래프 | `EDGE_ID_FROM_PARTS` `GRAPH_ID_FROM_EDGE_ID` `GRAPH_ID_FROM_NODE_ID` `NODE_ID_FROM_PARTS` `OBJECT_ID_FROM_EDGE_ID` `OBJECT_ID_FROM_NODE_ID` · 술어 `MATCH` · `SHORTEST_PATH` |
| JSON | `ISJSON` `JSON_ARRAY`(2022) `JSON_ARRAYAGG`(2025) `JSON_MODIFY` `JSON_OBJECT`(2022) `JSON_OBJECTAGG`(2025) `JSON_PATH_EXISTS`(2022) `JSON_QUERY` `JSON_VALUE` `OPENJSON` |
| 수학 | `ABS` `ACOS` `ASIN` `ATAN` `ATN2` `CEILING` `COS` `COT` `DEGREES` `EXP` `FLOOR` `LOG` `LOG10` `PI` `POWER` `RADIANS` `RAND` `ROUND` `SIGN` `SIN` `SQRT` `SQUARE` `TAN` |
| 논리 | `CHOOSE` `IIF` `GREATEST`(2022) `LEAST`(2022) |
| 메타데이터 | `APP_NAME` `APPLOCK_MODE` `APPLOCK_TEST` `ASSEMBLYPROPERTY` `COL_LENGTH` `COL_NAME` `COLUMNPROPERTY` `DATABASE_PRINCIPAL_ID` `DATABASEPROPERTYEX` `DB_ID` `DB_NAME` `FILE_ID` `FILE_IDEX` `FILE_NAME` `FILEGROUP_ID` `FILEGROUP_NAME` `FILEGROUPPROPERTY` `FILEPROPERTY` `FULLTEXTCATALOGPROPERTY` `FULLTEXTSERVICEPROPERTY` `INDEX_COL` `INDEXKEY_PROPERTY` `INDEXPROPERTY` `NEXT VALUE FOR` `OBJECT_DEFINITION` `OBJECT_ID` `OBJECT_NAME` `OBJECT_SCHEMA_NAME` `OBJECTPROPERTY` `OBJECTPROPERTYEX` `ORIGINAL_DB_NAME` `PARSENAME` `SCHEMA_ID` `SCHEMA_NAME` `SCOPE_IDENTITY` `SERVERPROPERTY` `STATS_DATE` `TYPE_ID` `TYPE_NAME` `TYPEPROPERTY` `VERSION` `@@PROCID` |
| 복제 | `PUBLISHINGSERVERNAME` |
| 행 집합 | `OPENDATASOURCE` `OPENJSON` `OPENQUERY` `OPENROWSET` `OPENXML` `STRING_SPLIT`(2016+, 2022+ `enable_ordinal`) `GENERATE_SERIES`(2022) `CONTAINSTABLE` `FREETEXTTABLE` · 2025 `REGEXP_MATCHES` `REGEXP_SPLIT_TO_TABLE` |
| 보안 | `CURRENT_USER` `SESSION_USER` `SYSTEM_USER` `USER` `USER_ID`(사용 중단) `USER_NAME` `SUSER_ID` `SUSER_NAME` `SUSER_SID` `SUSER_SNAME` `ORIGINAL_LOGIN` `IS_MEMBER` `IS_ROLEMEMBER` `IS_SRVROLEMEMBER` `HAS_PERMS_BY_NAME` `HAS_DBACCESS` `PERMISSIONS`(사용 중단) `LOGINPROPERTY` `PWDCOMPARE` `PWDENCRYPT` `SCHEMA_ID` `SCHEMA_NAME` `fn_my_permissions` `sys.fn_builtin_permissions` |
| 문자열 | `ASCII` `CHAR` `CHARINDEX` `CONCAT` `CONCAT_WS` `DIFFERENCE` `FORMAT` `LEFT` `LEN` `LOWER` `LTRIM` `NCHAR` `PATINDEX` `QUOTENAME` `REPLACE` `REPLICATE` `REVERSE` `RIGHT` `RTRIM` `SOUNDEX` `SPACE` `STR` `STRING_AGG` `STRING_ESCAPE` `STRING_SPLIT` `STUFF` `SUBSTRING` `TRANSLATE` `TRIM` `UNICODE` `UPPER` · 2022 `LTRIM/RTRIM/TRIM` 문자 인자 · 2025 `UNISTR` `BASE64_ENCODE` `BASE64_DECODE` `REGEXP_LIKE` `REGEXP_REPLACE` `REGEXP_SUBSTR` `REGEXP_INSTR` `REGEXP_COUNT` · 미리 보기 `EDIT_DISTANCE` `EDIT_DISTANCE_SIMILARITY` `JARO_WINKLER_DISTANCE` `JARO_WINKLER_SIMILARITY` |
| 시스템 | `$PARTITION` `@@ERROR` `@@IDENTITY` `@@PACK_RECEIVED` `@@ROWCOUNT` `@@TRANCOUNT` `BINARY_CHECKSUM` `CHECKSUM` `COMPRESS` `DECOMPRESS` `CONNECTIONPROPERTY` `CONTEXT_INFO` `CURRENT_REQUEST_ID` `CURRENT_TRANSACTION_ID` `ERROR_LINE` `ERROR_MESSAGE` `ERROR_NUMBER` `ERROR_PROCEDURE` `ERROR_SEVERITY` `ERROR_STATE` `FORMATMESSAGE` `GET_FILESTREAM_TRANSACTION_CONTEXT` `GETANSINULL` `HOST_ID` `HOST_NAME` `ISNULL` `ISNUMERIC` `MIN_ACTIVE_ROWVERSION` `NEWID` `NEWSEQUENTIALID` `ROWCOUNT_BIG` `SESSION_CONTEXT` `SESSION_ID` `XACT_STATE` · 그 밖 `COALESCE` `NULLIF`(식) |
| 시스템 통계 | `@@CONNECTIONS` `@@CPU_BUSY` `@@IDLE` `@@IO_BUSY` `@@PACK_SENT` `@@PACKET_ERRORS` `@@TIMETICKS` `@@TOTAL_ERRORS` `@@TOTAL_READ` `@@TOTAL_WRITE` `fn_virtualfilestats` |
| 텍스트·이미지(사용 중단) | `TEXTPTR` `TEXTVALID` |
| 트리거 | `COLUMNS_UPDATED` `EVENTDATA` `TRIGGER_NESTLEVEL` `UPDATE(column)` |
| 벡터(2025) | `VECTOR_DISTANCE` `VECTOR_NORM` `VECTOR_NORMALIZE` `VECTORPROPERTY` · 미리 보기 `VECTOR_SEARCH` · AI `AI_GENERATE_CHUNKS` `AI_GENERATE_EMBEDDINGS` |

#### T-6-2. 자주 쓰는 함수 시그니처

| 함수 | 시그니처 | 반환 |
|---|---|---|
| `CONVERT` | `CONVERT(data_type[(len)], expr [, style])` | data_type · style 예 112=`yyyymmdd`, 120=`yyyy-mm-dd hh:mi:ss`, 126=ISO8601 |
| `CAST` / `TRY_CAST` | `CAST(expr AS type)` | 실패 시 오류 / NULL |
| `ISNULL` | `ISNULL(check, replacement)` | check의 타입(잘림 주의) |
| `COALESCE` | `COALESCE(e1, e2, …)` | 우선순위 최고 타입 |
| `IIF` | `IIF(cond, t, f)` | CASE로 변환 |
| `CHOOSE` | `CHOOSE(index, v1, v2, …)` | 1부터 |
| `DATEADD` | `DATEADD(datepart, number, date)` | date 타입(2025: number bigint) |
| `DATEDIFF` | `DATEDIFF(datepart, start, end)` | int(경계 개수) |
| `DATETRUNC` | `DATETRUNC(datepart, date)` | 입력 타입 |
| `EOMONTH` | `EOMONTH(start [, months])` | date |
| `FORMAT` | `FORMAT(value, format [, culture])` | nvarchar · .NET 서식 · 비결정적·느림 |
| `SUBSTRING` | `SUBSTRING(expr, start [, length])` | length 생략 = 2025 |
| `CHARINDEX` | `CHARINDEX(find, in [, start])` | int(1부터, 없으면 0) |
| `STRING_AGG` | `STRING_AGG(expr, sep) [WITHIN GROUP (ORDER BY …)]` | 입력 문자형 |
| `STRING_SPLIT` | `STRING_SPLIT(str, sep [, enable_ordinal])` | 테이블(`value`[, `ordinal`]) |
| `JSON_VALUE` | `JSON_VALUE(json, path)` | nvarchar(4000) · 2025 `RETURNING type` 추정 |
| `OPENJSON` | `OPENJSON(json [, path]) [WITH (col type 'path' [AS JSON], …)]` | 테이블 |
| `ROW_NUMBER` | `ROW_NUMBER() OVER ([PARTITION BY …] ORDER BY …)` | bigint |
| `LAG`/`LEAD` | `LAG(expr [, offset [, default]]) OVER (…)` | |
| `GREATEST` | `GREATEST(e1, …, eN)` (최대 254개) | 2022+ |
| `GENERATE_SERIES` | `GENERATE_SERIES(start, stop [, step])` | 테이블(`value`) · 2022+ |
| `OBJECT_ID` | `OBJECT_ID('[db.][schema.]name' [, 'type'])` | int |
| `OBJECT_DEFINITION` | `OBJECT_DEFINITION(object_id)` | nvarchar(max) 소스 |
| `SCOPE_IDENTITY` | `SCOPE_IDENTITY()` | numeric(38,0) · 같은 범위 마지막 IDENTITY |
| `ERROR_MESSAGE` 등 | `ERROR_NUMBER()`… | CATCH 블록 안에서만 의미 |

#### T-6-3. 주요 시스템 저장 프로시저

| 프로시저 | 용도 |
|---|---|
| `sp_executesql @stmt, @params, @p1=…` | 매개변수화 동적 SQL(OUTPUT 매개변수 지원) |
| `sp_help [obj]` · `sp_helptext obj` · `sp_helpindex` · `sp_helpconstraint` · `sp_helpdb` · `sp_helprotect` | 개체 정보 · 소스(줄 단위 행) · 인덱스 · 제약 · DB · 권한 |
| `sp_columns` · `sp_tables` · `sp_stored_procedures` · `sp_pkeys` · `sp_fkeys` · `sp_sproc_columns` | ODBC 카탈로그 프로시저 |
| `sp_describe_first_result_set` · `sp_describe_undeclared_parameters` | 결과 메타·매개변수 추론(실행 없이) |
| `sp_who` · `sp_who2`(미문서) · `sp_lock` · `sp_spaceused` · `sp_depends`(사용 중단) | 세션·잠금·용량·의존 |
| `sp_rename 'old', 'new' [, 'COLUMN'\|'INDEX'\|'OBJECT'…]` | 이름 바꾸기 |
| `sp_addlinkedserver` · `sp_addlinkedsrvlogin` · `sp_dropserver` · `sp_linkedservers` · `sp_testlinkedserver` | 연결된 서버 |
| `sp_configure` + `RECONFIGURE` | 서버 구성 |
| `sp_addextendedproperty` · `sp_updateextendedproperty` · `sp_dropextendedproperty` · `fn_listextendedproperty` | 확장 속성(설명 `MS_Description`) |
| `sp_set_session_context` | `SESSION_CONTEXT` 키 설정 |
| `sp_prepare` / `sp_execute` / `sp_unprepare` · `sp_prepexec` · `sp_cursor*` | 드라이버가 쓰는 RPC 준비 실행·API 커서 |
| `sp_getapplock` / `sp_releaseapplock` | 애플리케이션 잠금 |
| `sp_recompile` · `sp_refreshview` · `sp_refreshsqlmodule` | 재컴파일·메타 새로 고침 |
| `msdb.dbo.sp_add_job` · `sp_add_jobstep` · `sp_start_job` · `sp_help_job` | SQL Agent 작업 |
| `sp_invoke_external_rest_endpoint`(2025) | 외부 REST 호출 |

출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/functions/functions
출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/functions/json-functions-transact-sql
출처: ★ https://learn.microsoft.com/en-us/sql/t-sql/functions/date-and-time-data-types-and-functions-transact-sql
출처: ★ https://github.com/MicrosoftDocs/sql-docs/blob/live/docs/t-sql/functions/aggregate-functions-transact-sql.md
출처: ★ https://github.com/MicrosoftDocs/sql-docs/blob/live/docs/t-sql/functions/system-functions-transact-sql.md
출처: ★ https://github.com/MicrosoftDocs/sql-docs/blob/live/docs/t-sql/functions/metadata-functions-transact-sql.md
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/string-functions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/mathematical-functions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/logical-functions-iif-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/cryptographic-functions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/security-functions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/bit-manipulation-functions-overview
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/regular-expressions-functions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/system-stored-procedures-transact-sql

---

### T-7. 질의(SELECT)

```sql
[ WITH cte [(cols)] AS (…) [, …] ]
SELECT [ ALL | DISTINCT ] [ TOP (n) [PERCENT] [WITH TIES] ] select_list
[ INTO new_table ]
[ FROM table_source [ WITH (table_hint, …) ] … ]
[ WHERE … ] [ GROUP BY … [ROLLUP|CUBE|GROUPING SETS] ] [ HAVING … ]
[ WINDOW w AS (…) ]                       -- 2022+
[ ORDER BY … [ OFFSET n ROWS [ FETCH { FIRST | NEXT } m ROWS ONLY ] ] ]
[ FOR { XML … | JSON … | BROWSE } ]
[ OPTION ( query_hint, … ) ]
```

| 기능 | T-SQL 문법 | 주의 |
|---|---|---|
| 상위 N | `TOP (n)` · `TOP (n) PERCENT` · `WITH TIES`(ORDER BY 필요) | 괄호 없는 `TOP n`은 하위 호환 · DML에도 `TOP` 가능 |
| 페이징 | `ORDER BY … OFFSET n ROWS FETCH NEXT m ROWS ONLY` | **ORDER BY 필수** · `LIMIT` 없음 · TOP과 혼용 불가 |
| 테이블 힌트 | `FROM t WITH (NOLOCK)` · `UPDLOCK` `ROWLOCK` `HOLDLOCK` `READPAST` `TABLOCKX` `INDEX(ix)` `FORCESEEK` `SNAPSHOT` | `NOLOCK` = READ UNCOMMITTED |
| 질의 힌트 | `OPTION (RECOMPILE, MAXDOP 1, OPTIMIZE FOR (@p UNKNOWN), HASH JOIN, USE HINT('…'), MAXRECURSION n)` | 문장 끝 |
| 행 잠금 조회 | `FOR UPDATE` **없음**(커서 선언 외) → `WITH (UPDLOCK, ROWLOCK)` | |
| 피벗 | `FROM src PIVOT (SUM(v) FOR col IN ([a],[b])) AS p` · `UNPIVOT (v FOR col IN (…))` | IN 목록은 고정(동적은 동적 SQL) |
| 측면 조인 | `CROSS APPLY` / `OUTER APPLY` (TVF·상관 하위 질의) | `LATERAL` 키워드 없음 |
| 조인 | `INNER` `LEFT/RIGHT/FULL [OUTER]` `CROSS JOIN` · 조인 힌트 `INNER HASH JOIN` | `NATURAL JOIN`·`USING` 없음 · 옛 `*=` 제거됨 |
| 재귀 CTE | `WITH r AS (anchor UNION ALL recursive) SELECT …` | `RECURSIVE` 키워드 없음 · 기본 최대 100 단계(`OPTION (MAXRECURSION 0)`=무제한) · CTE 앞 문장은 `;` 필요 |
| 집합 연산 | `UNION [ALL]` `INTERSECT` `EXCEPT` | `MINUS` 없음 · `INTERSECT ALL`/`EXCEPT ALL` 없음 |
| XML 출력 | `FOR XML { RAW | AUTO | EXPLICIT | PATH } [, ROOT('r')] [, ELEMENTS] [, TYPE]` | |
| JSON 출력 | `FOR JSON { AUTO | PATH } [, ROOT('r')] [, INCLUDE_NULL_VALUES] [, WITHOUT_ARRAY_WRAPPER]` | 결과 = nvarchar(max) 한 열(긴 값은 여러 행으로 나뉘어 옴 · 클라이언트가 이어 붙여야 함) |
| 표본 | `FROM t TABLESAMPLE (10 PERCENT | n ROWS) [REPEATABLE (seed)]` | 페이지 단위 |
| 새 테이블 | `SELECT … INTO newtbl FROM …` | `CREATE TABLE AS` 없음 · 2017+ `ON filegroup` |
| 시점 질의 | `FROM t FOR SYSTEM_TIME AS OF '…' | BETWEEN … AND … | ALL` | 시스템 버전 임시 테이블 |
| 그래프 | `FROM n1, e, n2 WHERE MATCH(n1-(e)->n2)` | |
| 값 생성자 | `FROM (VALUES (1,'a'),(2,'b')) AS v(id, name)` | |
| 변수 대입 | `SELECT @a = col FROM …` (여러 행이면 마지막 값) | 대입과 결과 반환 혼용 불가 |
| `DISTINCT` 예외 | `COUNT(DISTINCT x)` | |
| 사용 중단 | `COMPUTE`(제거) · `FOR BROWSE` · `GROUP BY ALL` | |

출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/select-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/top-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/select-order-by-clause-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/hints-transact-sql-table
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/hints-transact-sql-query
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/from-using-pivot-and-unpivot
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/with-common-table-expression-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/select-window-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/json/format-query-results-as-json-with-for-json-sql-server
출처: https://learn.microsoft.com/en-us/sql/relational-databases/xml/for-xml-sql-server

---

### T-8. DML

| 문장 | 문법 요점 | 비고 |
|---|---|---|
| INSERT | `INSERT [TOP (n)] [INTO] t [(cols)] { VALUES (…),(…) | SELECT … | EXEC proc | DEFAULT VALUES }` | 다중 행 VALUES = 최대 1000행 · `INSERT … EXEC` = 프로시저 결과 집합 적재(중첩 불가) · IDENTITY 직접 값 = `SET IDENTITY_INSERT t ON` |
| OUTPUT 절 | `INSERT … OUTPUT inserted.* [INTO @tv] VALUES …` · `UPDATE … OUTPUT deleted.c, inserted.c` · `DELETE … OUTPUT deleted.*` | **`RETURNING` 없음** · 트리거가 있는 테이블에 `OUTPUT`(INTO 없이)은 제약 있음(오류 334) |
| UPDATE | `UPDATE [TOP (n)] t SET c = v [, @var = c = v] [OUTPUT …] [FROM t JOIN …] [WHERE …]` | `UPDATE … FROM` 조인 갱신 · `.WRITE(expr, off, len)`(max 타입 부분 갱신) · `col.modify()`(xml/json) · `WHERE CURRENT OF cursor` |
| DELETE | `DELETE [TOP (n)] [FROM] t [OUTPUT …] [FROM t JOIN …] [WHERE …]` | `FROM`이 두 번 나올 수 있음 |
| MERGE | `MERGE [TOP (n)] INTO tgt [WITH (HOLDLOCK)] USING src ON … WHEN MATCHED [AND …] THEN UPDATE/DELETE WHEN NOT MATCHED [BY TARGET] THEN INSERT … WHEN NOT MATCHED BY SOURCE THEN … [OUTPUT $action, inserted.*, deleted.*];` | **끝 `;` 필수** · `$action` = 'INSERT'/'UPDATE'/'DELETE' |
| TRUNCATE | `TRUNCATE TABLE t [WITH (PARTITIONS (1, 3 TO 5))]` | 트랜잭션 안 롤백 가능 · FK 참조 시 불가 · IDENTITY 재설정 |
| 대량 | `BULK INSERT t FROM 'file' WITH (…)` · `INSERT … SELECT * FROM OPENROWSET(BULK …)` | 서버 측 파일 경로 |
| 영향 행 수 | `@@ROWCOUNT` / `ROWCOUNT_BIG()` · TDS DONE 토큰 행 수 | `SET NOCOUNT ON`이면 DONE 토큰의 행 수 표시 비트가 꺼짐 |

출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/insert-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/output-clause-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/queries/update-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/delete-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/merge-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/truncate-table-transact-sql

---

### T-9. 개체 종류

공통: 이름 = `[server.][db.][schema.]name` · **스키마 범위 개체**는 DB 안 스키마에 속함(`sys.objects`) · 서버 범위(로그인·연결된 서버·서버 트리거·엔드포인트)는 `master`/`sys.server_*` · `CREATE OR ALTER` = 2016 SP1+ (프로시저·함수·트리거·뷰만) · `DROP … IF EXISTS` = 2016+ · **오버로드 불가**(같은 스키마 같은 이름 금지 · 번호 프로시저 `proc;2` = 사용 중단) · 상태 = 개체별 비활성 깃발 · 의존 = `sys.sql_expression_dependencies`(+ `sys.dm_sql_referenced_entities`/`referencing_entities`) · 소스 = `sys.sql_modules.definition` / `OBJECT_DEFINITION()` / `sp_helptext`(암호화 `WITH ENCRYPTION`이면 NULL).

| 개체 | CREATE / ALTER / DROP 요점 | OR ALTER | IF EXISTS | 상태·변형 | 카탈로그(`sys.objects.type`) |
|---|---|---|---|---|---|
| TABLE | `CREATE TABLE s.t (col type [NULL\|NOT NULL] [IDENTITY(1,1)] [CONSTRAINT …], …) [ON fg] [WITH (…)]` · `ALTER TABLE … ADD/ALTER COLUMN/DROP COLUMN/ADD CONSTRAINT/NOCHECK/SWITCH/REBUILD` | ✗ | ✓ | 변형: 로컬/전역 임시 `#t`/`##t` · 테이블 변수 `DECLARE @t TABLE(…)` · 분할(파티션 함수+스키마) · **시스템 버전 임시**(`PERIOD FOR SYSTEM_TIME` + `WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = …))`) · 메모리 최적화(`MEMORY_OPTIMIZED = ON`) · 그래프 `AS NODE`/`AS EDGE` · **Ledger**(2022 `LEDGER = ON [(APPEND_ONLY = ON)]`) · 외부 테이블 `CREATE EXTERNAL TABLE` | `sys.tables`(U) · `sys.columns` · `temporal_type`, `is_memory_optimized`, `is_node`/`is_edge`, `ledger_type` |
| VIEW | `CREATE VIEW v [(cols)] [WITH SCHEMABINDING\|ENCRYPTION\|VIEW_METADATA] AS select [WITH CHECK OPTION]` | ✓ | ✓ | **인덱싱된 뷰** = `SCHEMABINDING` + 첫 인덱스 고유 클러스터형 | `sys.views`(V) · `sys.sql_modules` |
| SEQUENCE | `CREATE SEQUENCE s AS bigint START WITH 1 INCREMENT BY 1 [MINVALUE][MAXVALUE][CYCLE][CACHE n]` · 사용 `NEXT VALUE FOR s` · `sp_sequence_get_range` | ✗ | ✓ | — | `sys.sequences`(SO) |
| IDENTITY | 컬럼 속성 `IDENTITY(seed, incr)` · `DBCC CHECKIDENT` · `IDENT_CURRENT` | — | — | 테이블당 1개 | `sys.identity_columns` |
| INDEX | `CREATE [UNIQUE] [CLUSTERED\|NONCLUSTERED] INDEX ix ON t (c [ASC\|DESC]) [INCLUDE (…)] [WHERE …](필터) [WITH (ONLINE=ON, FILLFACTOR=…, DROP_EXISTING=ON)]` · `ALTER INDEX … REBUILD\|REORGANIZE\|DISABLE` | ✗(대신 `DROP_EXISTING`) | ✓ | 종류: 클러스터형 · 비클러스터형 · 컬럼스토어(`CLUSTERED/NONCLUSTERED COLUMNSTORE` · 2022+ 정렬) · 필터 · XML(기본/보조) · 공간 · 전체 텍스트(`CREATE FULLTEXT INDEX` · 테이블당 1) · 메모리 최적화 해시 · 2025 벡터(미리 보기) · 상태 `is_disabled` | `sys.indexes` · `sys.index_columns` · `sys.fulltext_indexes` · `sys.xml_indexes` · `sys.spatial_indexes` |
| 제약 | `PRIMARY KEY` `UNIQUE` `FOREIGN KEY … REFERENCES … [ON DELETE/UPDATE CASCADE\|SET NULL\|SET DEFAULT\|NO ACTION]` `CHECK` `DEFAULT` · 이름 없으면 `PK__t__…` 자동 이름 | ✗ | `ALTER TABLE … DROP CONSTRAINT IF EXISTS` ✓ | `WITH NOCHECK`(신뢰 안 됨 `is_not_trusted`) · `NOCHECK CONSTRAINT`(`is_disabled`) | `sys.key_constraints`(PK, UQ) · `sys.foreign_keys`(F) + `sys.foreign_key_columns` · `sys.check_constraints`(C) · `sys.default_constraints`(D) |
| TRIGGER | DML: `CREATE TRIGGER tr ON t {AFTER\|FOR\|INSTEAD OF} {INSERT,UPDATE,DELETE} AS …`(가상 테이블 `inserted`/`deleted`) · DDL: `ON DATABASE\|ALL SERVER FOR CREATE_TABLE, DDL_DATABASE_LEVEL_EVENTS …`(`EVENTDATA()`) · 로그온: `ON ALL SERVER FOR LOGON` · `ENABLE/DISABLE TRIGGER tr ON …` | ✓ | ✓ | `is_disabled` · `is_instead_of_trigger` · 행 단위 트리거 없음(문장 단위) | `sys.triggers`(TR) · `sys.trigger_events` · 서버 = `sys.server_triggers` |
| PROCEDURE | `CREATE PROC[EDURE] s.p @a int = 0, @b int OUTPUT [WITH RECOMPILE\|ENCRYPTION\|EXECUTE AS …] AS …` · 네이티브 컴파일(`NATIVE_COMPILATION, SCHEMABINDING`) · CLR(`EXTERNAL NAME`) | ✓ | ✓ | 임시 `#p` · 시스템 `sp_` 접두(master 우선 탐색 · 사용자 이름에 쓰지 말 것) | `sys.procedures`(P, PC, X) · `sys.parameters` · `sys.sql_modules` |
| FUNCTION | 스칼라: `RETURNS type AS BEGIN … RETURN … END` · 인라인 TVF: `RETURNS TABLE AS RETURN (select)` · 다문 TVF: `RETURNS @r TABLE (…) AS BEGIN … RETURN END` · CLR 스칼라/TVF/집계 | ✓ | ✓ | 부작용 금지(DML은 테이블 변수만) · 2019+ 스칼라 UDF 인라인화(`INLINE = ON\|OFF`) | type `FN` 스칼라 · `IF` 인라인 · `TF` 다문 · `FS`/`FT` CLR · `AF` CLR 집계 |
| TYPE | alias `CREATE TYPE s.t FROM base [NOT NULL]` · 테이블 `CREATE TYPE s.t AS TABLE (…)` · CLR | ✗(ALTER TYPE 없음 → 삭제·재생성) | ✓ | — | `sys.types` · `sys.table_types` |
| SYNONYM | `CREATE SYNONYM s.syn FOR [srv.][db.][schema.]obj` | ✗ | ✓ | 대상 존재 검사 안 함(지연 바인딩) | `sys.synonyms`(SN) · `base_object_name` |
| LINKED SERVER | `sp_addlinkedserver @server, @srvproduct, @provider, @datasrc` · `sp_addlinkedsrvlogin` · `sp_dropserver` · 사용 `srv.db.s.t` / `OPENQUERY(srv, '…')` / `EXEC (…) AT srv` | — | — | `is_linked` · 2025 암호화 기본 변경(호환성 깨짐) | `sys.servers` · `sys.linked_logins` |
| SCHEMA | `CREATE SCHEMA s [AUTHORIZATION owner] [create_table/view/grant …]` · `ALTER SCHEMA s TRANSFER obj` | ✗ | ✓ | `CREATE SCHEMA`는 배치의 유일 문장 | `sys.schemas` |
| DATABASE | `CREATE DATABASE d [ON (FILENAME=…)] [COLLATE …]` · `ALTER DATABASE d SET …` · `ALTER DATABASE SCOPED CONFIGURATION` | ✗ | ✓ | `state_desc`(ONLINE/OFFLINE/RESTORING…) · `user_access_desc` · 스냅숏 | `sys.databases` · `sys.database_files` · `sys.master_files` |
| LOGIN / USER / ROLE | `CREATE LOGIN l WITH PASSWORD=… \| FROM WINDOWS \| FROM EXTERNAL PROVIDER` · `CREATE USER u FOR LOGIN l \| WITHOUT LOGIN \| WITH PASSWORD(포함 DB)` · `CREATE ROLE r` · `ALTER ROLE r ADD MEMBER u` · `CREATE SERVER ROLE` · `CREATE APPLICATION ROLE` | ✗ | ✓(USER·ROLE) | 로그인 `is_disabled`(`ALTER LOGIN l DISABLE`) | `sys.server_principals` · `sys.sql_logins` · `sys.database_principals` · `sys.database_role_members` · `sys.server_role_members` |
| SQL Agent JOB | T-SQL DDL 없음 → `msdb.dbo.sp_add_job/sp_add_jobstep/sp_add_schedule/sp_attach_schedule/sp_add_jobserver` | — | — | `enabled` | `msdb.dbo.sysjobs` · `sysjobsteps` · `sysjobhistory` · `sysschedules` |
| ASSEMBLY | `CREATE ASSEMBLY a FROM 0x… \| 'path' WITH PERMISSION_SET = SAFE` | ✗ | ✓ | `clr enabled` · `clr strict security` | `sys.assemblies` · `sys.assembly_modules` |
| 기타 | `PARTITION FUNCTION`/`SCHEME` · `FULLTEXT CATALOG` · `XML SCHEMA COLLECTION` · `CERTIFICATE`/`SYMMETRIC KEY`/`ASYMMETRIC KEY`/`MASTER KEY` · `CREDENTIAL` · `EXTERNAL DATA SOURCE`/`FILE FORMAT` · `STATISTICS` · `RULE`/`DEFAULT`(사용 중단 개체) · Service Broker(`QUEUE`·`SERVICE`·`CONTRACT`·`MESSAGE TYPE`·`ROUTE`) · `SECURITY POLICY`(RLS) · `EXTERNAL MODEL`(2025) · `EVENT SESSION` · `ENDPOINT` · `AVAILABILITY GROUP` | 대개 ✗ | 대개 ✓ | — | `sys.partition_functions` · `sys.fulltext_catalogs` · `sys.xml_schema_collections` · `sys.certificates` · `sys.stats` · `sys.service_queues`(SQ) · `sys.security_policies` |
| 표준 뷰 | `INFORMATION_SCHEMA.TABLES/COLUMNS/VIEWS/ROUTINES/PARAMETERS/TABLE_CONSTRAINTS/KEY_COLUMN_USAGE/REFERENTIAL_CONSTRAINTS/CHECK_CONSTRAINTS/SCHEMATA/DOMAINS/SEQUENCES` | — | — | ISO 호환 · SQL Server 고유 정보(인덱스·트리거 등)는 없음 → `sys.*` 권장 | — |

- `sys.objects.type` 주요 코드: `U` 사용자 테이블 · `S` 시스템 테이블 · `IT` 내부 테이블 · `V` 뷰 · `P` 프로시저 · `X` 확장 프로시저 · `PC` CLR 프로시저 · `FN`/`IF`/`TF`/`FS`/`FT`/`AF` 함수 · `TR` 트리거 · `TA` CLR 트리거 · `PK` `UQ` `F` `C` `D` 제약 · `SN` 시노님 · `SO` 시퀀스 · `TT` 테이블 타입 · `SQ` 서비스 큐 · `R` 규칙 · `ET` 외부 테이블 · `EC` 에지 제약 · `RF` 복제 필터 프로시저 · `PG` 계획 가이드.
- 생성·수정 시각 = `sys.objects.create_date`/`modify_date`. 확장 속성(설명) = `sys.extended_properties`(`MS_Description`).

출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-objects-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-table-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-view-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-index-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-trigger-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-procedure-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-function-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-synonym-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-sequence-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/tables/temporal-tables
출처: https://learn.microsoft.com/en-us/sql/relational-databases/security/ledger/ledger-overview
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-sql-expression-dependencies-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-sql-modules-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-information-schema-views/system-information-schema-views-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/sp-addlinkedserver-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/sp-add-job-transact-sql

---

### T-10. 루틴(프로시저 · 함수) 호출 규약

| 항목 | 규칙 |
|---|---|
| 매개변수 이름 | `@p` 필수 접두 · 최대 2,100개 |
| 방향 | 입력(기본) · `OUTPUT`(= `OUT`, 입출력 겸용) · 호출 측도 `EXEC p @x = @v OUTPUT`처럼 **OUTPUT을 다시 써야** 값이 돌아옴 |
| 기본값 | `@p int = 10` · 호출에서 생략 또는 `DEFAULT` 키워드 |
| TVP | `@rows dbo.MyTableType READONLY` (READONLY 필수) |
| 호출 | `EXEC[UTE] [@rc =] s.p 1, 'a'`(위치) · `EXEC s.p @a = 1, @b = 'a'`(이름) · 섞으면 위치가 앞에 · `EXEC p … WITH RECOMPILE` · `WITH RESULT SETS ((…))` |
| 동적 | `EXEC ('SELECT …')` · `EXEC sp_executesql N'… @x', N'@x int', @x = 1` · `EXEC (…) AT linked_srv` |
| 반환 | 프로시저: **정수 상태 코드**(`RETURN n`, 기본 0) + OUTPUT 매개변수 + **결과 집합 0~N개** · 함수: 스칼라 값 또는 테이블(`SELECT * FROM dbo.f(…)`) · 스칼라 UDF 호출엔 스키마 필수(`dbo.f(1)`) |
| 프로시저 안에서 결과 | `SELECT`마다 결과 집합 · `PRINT`/`RAISERROR(…,0~10)` = 정보 메시지(TDS INFO) |
| 본문 경계 | `CREATE PROCEDURE/FUNCTION/TRIGGER/VIEW`는 **배치의 첫 문장**(유일 문장)이어야 함 · 본문은 **배치 끝(GO)**까지 — `BEGIN…END`가 끝을 정하지 않음 → 클라이언트 분할기는 GO로만 끊어야 한다 |
| 오버로드 | 없음 |
| 실행 권한 컨텍스트 | `WITH EXECUTE AS { CALLER | SELF | OWNER | 'user' }` |
| 중첩 | 최대 32단계(`@@NESTLEVEL`) |

출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-procedure-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/execute-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/sp-executesql-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/tables/use-table-valued-parameters-database-engine
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/create-function-transact-sql

---

### T-11. 트랜잭션 · 세션

| 항목 | 내용 |
|---|---|
| 기본 모드 | **자동 커밋**(문장마다) · `SET IMPLICIT_TRANSACTIONS ON` = 암묵 트랜잭션(Oracle식) · 명시 `BEGIN TRAN` |
| 문장 | `BEGIN TRAN[SACTION] [name] [WITH MARK]` · `COMMIT [TRAN]` · `ROLLBACK [TRAN [savepoint]]` · `SAVE TRAN[SACTION] sp` · `COMMIT WORK`/`ROLLBACK WORK`(ISO) |
| 중첩 | `@@TRANCOUNT` 증가만 · 안쪽 COMMIT은 감소만 · **ROLLBACK은 전체 롤백**(세이브포인트 지정 제외) |
| 상태 | `XACT_STATE()` = 1 활성·커밋 가능 / -1 커밋 불가(롤백만) / 0 없음 |
| 오류 시 | `SET XACT_ABORT ON` → 런타임 오류에 전체 롤백·배치 중단(분산 트랜잭션 필수) · OFF(기본)면 문장만 롤백되는 오류가 많음 |
| 격리 수준 | `SET TRANSACTION ISOLATION LEVEL { READ UNCOMMITTED | READ COMMITTED | REPEATABLE READ | SNAPSHOT | SERIALIZABLE }` · 기본 READ COMMITTED(잠금) · DB 옵션 `READ_COMMITTED_SNAPSHOT ON` = RC를 행 버전으로 · SNAPSHOT은 `ALLOW_SNAPSHOT_ISOLATION ON` 필요 · Azure SQL DB 기본 RCSI |
| DDL | **트랜잭션 안 DDL 가능·롤백 가능**(`CREATE TABLE` 등) · 예외: `CREATE/ALTER/DROP DATABASE`, `BACKUP`, 전체 텍스트 일부 등 |
| 잠금 대기 | `SET LOCK_TIMEOUT ms`(-1 = 무한, 기본) · 교착 = 오류 1205 희생자 · `SET DEADLOCK_PRIORITY` |
| 세션 SET | `ANSI_NULLS` `ANSI_PADDING` `ANSI_WARNINGS` `ARITHABORT` `CONCAT_NULL_YIELDS_NULL` `QUOTED_IDENTIFIER` `NUMERIC_ROUNDABORT`(OFF) · `NOCOUNT` · `DATEFORMAT` `DATEFIRST` `LANGUAGE` · `TEXTSIZE` · `ROWCOUNT`(DML 영향 사용 중단) · `STATISTICS IO/TIME/XML` · `SHOWPLAN_XML` · `CONTEXT_INFO` · **모듈 생성 시점의 `ANSI_NULLS`·`QUOTED_IDENTIFIER`가 모듈에 저장**(`sys.sql_modules.uses_ansi_nulls`) |
| DB 전환 | `USE db` — 세션 현재 DB 변경 · TDS ENVCHANGE 토큰으로 클라이언트에 통지 · 스키마 전환 문 없음(기본 스키마 = 사용자 속성 `ALTER USER u WITH DEFAULT_SCHEMA = s`) |
| 세션 식별 | `@@SPID` = `SESSION_ID()`(2025 추정) · `sys.dm_exec_sessions` · `KILL spid` |

출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/transactions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/set-transaction-isolation-level-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/set-xact-abort-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/set-implicit-transactions-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/set-statements-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/xact-state-transact-sql

---

### T-12. DCL(권한)

| 문장 | 문법 |
|---|---|
| GRANT | `GRANT perm [, …] [ON [class::] securable] TO principal [WITH GRANT OPTION] [AS grantor]` |
| DENY | `DENY perm ON securable TO principal [CASCADE]` — **DENY가 GRANT보다 우선**(SQL Server 고유 3상태) |
| REVOKE | `REVOKE [GRANT OPTION FOR] perm ON securable {TO\|FROM} principal [CASCADE]` — 부여·거부 둘 다 지움 |

| 범주 | 내용 |
|---|---|
| 보안 개체 계층 | 서버(엔드포인트·로그인·서버 역할·DB) → DB(사용자·역할·애플리케이션 역할·어셈블리·인증서·키·스키마 …) → 스키마(테이블·뷰·프로시저·함수·시노님·시퀀스·타입·XML 스키마 컬렉션) → 컬럼 |
| 클래스 접두 | `ON SCHEMA::s` · `ON OBJECT::s.t` · `ON DATABASE::d` · `ON LOGIN::l` · `ON TYPE::s.t` · `ON SERVER`(생략형) |
| 대표 권한 | `SELECT` `INSERT` `UPDATE` `DELETE` `EXECUTE` `REFERENCES` `ALTER` `CONTROL` `TAKE OWNERSHIP` `VIEW DEFINITION` `IMPERSONATE` `CREATE TABLE` … · 서버 `CONNECT SQL` `VIEW SERVER STATE` `ALTER ANY LOGIN` |
| 주체 | 서버: 로그인(SQL·Windows·Entra) · 서버 역할(`sysadmin` `securityadmin` `serveradmin` `setupadmin` `processadmin` `diskadmin` `dbcreator` `bulkadmin` `public` · 2022+ `##MS_*##` 역할) · DB: 사용자 · 역할(`db_owner` `db_datareader` `db_datawriter` `db_ddladmin` `db_securityadmin` `db_accessadmin` `db_backupoperator` `db_denydatareader` `db_denydatawriter` `public`) · 애플리케이션 역할 |
| 카탈로그 | `sys.database_permissions` · `sys.server_permissions` · `fn_my_permissions(…)` · `HAS_PERMS_BY_NAME()` |
| 소유권 체인 | 같은 소유자 개체 간 권한 검사 생략 · DB 간 체인 옵션 |

출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/grant-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/deny-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/revoke-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/security/securables
출처: https://learn.microsoft.com/en-us/sql/relational-databases/security/authentication-access/principals-database-engine

---

### T-13. 오류

| 항목 | 내용 |
|---|---|
| 오류 구성 | **번호**(`sys.messages.message_id` · 사용자 정의 50001+) · **심각도** 0~25 · **상태** 0~255 · 프로시저 이름 · **줄 번호**(배치/모듈 기준) · 메시지 |
| 심각도 의미 | 0~10 정보(INFO 토큰·`PRINT`와 같은 길) · 11~16 사용자 수정 가능 오류 · 17~19 자원·소프트웨어 · 20~25 치명적(연결 끊김 · 기록은 `WITH LOG`) |
| `RAISERROR` | `RAISERROR('msg %s %d', severity, state, args…) [WITH NOWAIT\|LOG\|SETERROR]` · `WITH NOWAIT` = 즉시 전송(진행 메시지) · 번호 지정 시 `sys.messages`(`sp_addmessage`) |
| `THROW` | `THROW 50001, 'msg', state;` (심각도 16 고정 · 번호 ≥ 50000) · CATCH 안 인자 없는 `THROW;` = 재발생 · **앞 문장 `;` 필수** · `XACT_ABORT` 무관하게 배치 중단 |
| TRY/CATCH | `BEGIN TRY … END TRY BEGIN CATCH … END CATCH` · 심각도 11~19만 잡음 · 컴파일·이름 해석 오류(같은 수준)는 못 잡음 · CATCH 안 `ERROR_NUMBER()` `ERROR_SEVERITY()` `ERROR_STATE()` `ERROR_LINE()` `ERROR_PROCEDURE()` `ERROR_MESSAGE()` |
| `@@ERROR` | 직전 문장 오류 번호(다음 문장에서 0으로 초기화) |
| 메시지 언어 | `SET LANGUAGE`/로그인 기본 언어 → `sys.messages.language_id` |
| 대표 번호 | 102 구문 오류 · 156 키워드 근처 구문 오류 · 207 컬럼 없음 · 208 개체 없음 · 515 NULL 삽입 · 547 제약 충돌 · 2601/2627 고유 위반 · 1205 교착 · 1222 잠금 시간 초과 · 8134 0 나누기 · 8152/2628 문자열 잘림(2019+ 2628에 값 표시) · 3609 트리거에서 트랜잭션 종료 · 18456 로그인 실패 · 4060 DB 열 수 없음 · 3621 문 종료 |
| 줄 번호 기준 | 배치(GO 사이) 첫 줄 = 1 · 모듈은 `CREATE` 문 기준 → 클라이언트가 편집기 줄로 환산하려면 **배치 시작 줄 오프셋**을 더해야 함 |

출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/raiserror-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/throw-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/language-elements/try-catch-transact-sql
출처: https://learn.microsoft.com/en-us/sql/t-sql/functions/error-line-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/errors-events/database-engine-error-severities
출처: https://learn.microsoft.com/en-us/sql/relational-databases/errors-events/database-engine-events-and-errors

---

### T-14. 클라이언트 프로토콜(TDS)

| 항목 | 내용 |
|---|---|
| 프로토콜 | **TDS**(Tabular Data Stream) · 7.4 = 2012+ 기본 · **TDS 8.0** = 2022+ 엄격 암호화(TLS가 PRELOGIN 앞 · `Encrypt=Strict`) · 2025는 TLS 1.3 확대 · 기본 포트 1433 · 명명 인스턴스 = SQL Browser UDP 1434 |
| 요청 종류 | SQL Batch(문자열 그대로 · `GO` 없이 한 배치) · **RPC**(`sp_executesql`/`sp_prepexec`/임의 프로시저 + 형식화 매개변수) · Bulk Load · Attention · Transaction Manager 요청 |
| 응답 토큰 | `COLMETADATA` `ROW`/`NBCROW` `DONE`/`DONEPROC`/`DONEINPROC`(행 수·상태 · 결과 집합마다) `RETURNSTATUS`(프로시저 반환값) `RETURNVALUE`(OUTPUT 매개변수) `ERROR` `INFO`(PRINT·심각도 ≤10) `ENVCHANGE`(DB·언어·패킷 크기·트랜잭션 시작/끝) `ORDER` |
| 다중 결과 | 한 배치·프로시저가 결과 집합 여러 개 + 행 수 + 메시지를 **섞어** 반환 → 클라이언트는 DONE 단위로 순회 · OUTPUT 매개변수·반환값은 **모든 결과를 다 읽은 뒤** 도착 |
| 바인드 타입 대응(대표) | 정수 → `int`/`bigint`(`INTN`) · 소수 → `decimal(p,s)`/`float` · 문자열 → `nvarchar(n)`(기본 · `varchar` 컬럼과 비교 시 암시 변환으로 인덱스 못 탈 수 있음) / 4000자 초과 `nvarchar(max)` · 이진 → `varbinary` · 날짜 → `datetime2`/`date`/`time`/`datetimeoffset`(`datetime` 컬럼에 datetime2 바인드 시 반올림 차이 주의) · bool → `bit` · GUID → `uniqueidentifier` · NULL → 타입 지정 NULL · TVP = 테이블 타입 매개변수 |
| 대량 적재 | 서버 측 `BULK INSERT` / `OPENROWSET(BULK…)`(서버 파일) · 클라이언트 측 **TDS Bulk Load**(`INSERT BULK` 문 + 데이터 스트림 · `SqlBulkCopy`·bcp·ODBC `bcp_*`) · 옵션 `TABLOCK` `CHECK_CONSTRAINTS` `FIRE_TRIGGERS` `KEEP_NULLS` `KEEP_IDENTITY` · 배치 크기 |
| 취소 | **Attention** 패킷(같은 연결에 신호 → 서버가 `DONE`(attention ack)로 응답할 때까지 읽어야 함) · 트랜잭션은 자동 롤백되지 않음(열린 채 남을 수 있음 → `XACT_ABORT` 또는 클라이언트가 `IF @@TRANCOUNT>0 ROLLBACK`) |
| 시간 제한 | 클라이언트 쪽 명령 시간 제한(서버 측 제한 없음 · `SET LOCK_TIMEOUT`만) |
| MARS | Multiple Active Result Sets(`MultipleActiveResultSets=True`) — 한 연결에서 SMUX 세션 여러 개 · 결과를 다 읽기 전 다른 요청 가능 · 기본 꺼짐 · 다중 문장 인터리브는 제한적 |
| 인증 | SQL 인증 · Windows(SSPI/Kerberos/NTLM) · Microsoft Entra(토큰·MSI·대화형) |
| 연결 속성 | 응용 프로그램 이름(`APP_NAME()`) · 워크스테이션(`HOST_NAME()`) · 초기 DB · 언어 · 패킷 크기 · 읽기 전용 의도(`ApplicationIntent=ReadOnly`) · `MultiSubnetFailover` |
| 서버 측 커서 | API 커서 `sp_cursoropen/fetch/close`(드라이버 내부) · 기본은 firehose(정방향 읽기 전용 스트림) |

출처: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tds/b46a581a-39de-4745-b076-ec4dbb7d13ec
출처: https://learn.microsoft.com/en-us/sql/relational-databases/security/networking/tds-8
출처: https://learn.microsoft.com/en-us/sql/relational-databases/native-client/features/using-multiple-active-result-sets-mars
출처: https://learn.microsoft.com/en-us/sql/t-sql/statements/bulk-insert-transact-sql
출처: https://learn.microsoft.com/en-us/sql/relational-databases/import-export/bulk-import-and-export-of-data-sql-server
출처: https://learn.microsoft.com/en-us/dotnet/api/microsoft.data.sqlclient.sqlbulkcopy
출처: https://learn.microsoft.com/en-us/sql/connect/odbc/dsn-connection-string-attribute

### T-15 nexa-sql 지원 현황

방언 열 **MS**(SQL Server) — 항목별 ✅/부분/❌와 코드 자리는 §11 표의 `MS` 열 · 한 줄 판정은 §11 끝 "요약" · 결함은 §9.

## §4. PostgreSQL 15/16

> 기준 = PostgreSQL 16 공식 문서(15와 다른 점은 "16+"로 표시). 웹 확인 = 2026-10-10. 공식 문서로 확인하지 못하고 기억에 기댄 사실은 **추정**으로 표시했다.

### T-1. 버전 · 문서 위치

| 항목 | 내용 |
|---|---|
| 대상 버전 | 15(2022-10) · 16(2023-09) — 둘 다 v3 프로토콜 · 같은 문법 골격 · 16에서 추가된 것 = 16진·8진·2진 정수 리터럴과 `_` 숫자 구분(§4.1.2.4) · SQL/JSON 생성자(`JSON_ARRAY`·`JSON_OBJECT`·`JSON_ARRAYAGG`·`JSON_OBJECTAGG`)와 `IS JSON` · `any_value` · `random_normal` · `date_add`/`date_subtract` · `SYSTEM_USER` |
| 15에서 추가 | `MERGE` · `regexp_count/instr/like/substr` · `UNIQUE NULLS NOT DISTINCT` · `security_invoker` 뷰 · 공개 스키마 `public`의 CREATE 권한이 PUBLIC에서 빠짐 · 논리 복제 행 필터·열 목록(추정 = 릴리스 노트 미확인) |
| SQL 명령 참조 | Part VI. Reference → I. SQL Commands |
| 함수·연산자 | Chapter 9 |
| 시스템 카탈로그 | Chapter 53(16 기준 번호 · 추정) |
| 오류 코드 | Appendix A. PostgreSQL Error Codes |
| 키워드 | Appendix C. SQL Key Words |

출처: https://www.postgresql.org/docs/16/sql-commands.html · https://www.postgresql.org/docs/16/functions.html · https://www.postgresql.org/docs/16/catalogs.html · https://www.postgresql.org/docs/16/errcodes-appendix.html · https://www.postgresql.org/docs/16/sql-keywords-appendix.html · https://www.postgresql.org/docs/16/release-16.html · https://www.postgresql.org/docs/15/release-15.html

### T-2. 어휘 구조

| 요소 | 규칙 |
|---|---|
| 식별자 길이 | `NAMEDATALEN`−1 = **63바이트**(기본 64 · 바이트 단위라 한글은 21자 남짓) · 넘으면 **잘라서 받아들임**(오류 아님 · NOTICE) · 컴파일 상수 `pg_config_manual.h` |
| 대소문자 | 따옴표 없는 식별자·키워드 = **소문자로 접힘**(Oracle·표준의 대문자 접힘과 반대) |
| 따옴표 식별자 | `"…"` = 대소문자 보존 · 코드 0 외 모든 글자 · `""` = 큰따옴표 하나 |
| 유니코드 식별자 | `U&"d\0061t\+000061"` · `\XXXX`(4자리) / `\+XXXXXX`(6자리) · `UESCAPE '!'`로 이스케이프 문자 변경 |
| 문자열 | `'…'` · `''` = 작은따옴표 · 인접 문자열은 **줄바꿈이 낀 공백**일 때만 이어 붙음(`'a' 'b'` 한 줄 = 구문 오류) |
| E 문자열 | `E'…'` · `\b \f \n \r \t` · `\o..\ooo` 8진 · `\xh`·`\xhh` 16진 · `\uXXXX`·`\UXXXXXXXX` · `\'` 허용 · 기타 `\c` = c 그대로 · `standard_conforming_strings`=on(9.1+ 기본)이면 일반 문자열의 `\`는 글자 그대로 |
| U& 문자열 | `U&'…'` [`UESCAPE 'c'`] · `standard_conforming_strings`=on일 때만 |
| 달러 인용 | `$$…$$` · `$tag$…$tag$` · 태그 = 따옴표 없는 식별자 규칙(단 `$` 불가) · **대소문자 구분** · 내부 이스케이프 없음 · 태그를 달리해 중첩 · 앞이 식별자·키워드면 공백 필요 · 표준 아님 |
| 비트 문자열 | `B'1001'` · `X'1FF'`(16진 1자리 = 4비트) · 달러 인용 불가 |
| 숫자 | `42` · `3.5` · `4.` · `.001` · `5e2` · `1.925e-3` · **16+**: `0x42f`·`0o755`·`0b1011` · `1_500_000` · 앞의 `+`/`-` = 단항 연산자 · 소수점·지수 없으면 integer → bigint → numeric 순으로 맞는 형, 있으면 numeric |
| 형 지정 상수 | `type 'string'` · `'string'::type` · `CAST('string' AS type)` · 일부 형은 `typename('string')` · `type 'string'`은 배열형 불가 |
| 주석 | `-- …` 줄 끝까지 · `/* … */` **중첩 허용**(C와 다름 · 표준 따름) — 분할기는 깊이를 세야 한다 |
| 문장 끝 | `;` · 단순 질의 프로토콜은 `;`로 나뉜 여러 문장을 한 번에 받음 |
| 위치 매개변수 | `$1`, `$2`…(함수 본문·준비 문장·확장 질의) — `$1`은 달러 인용 `$tag$`와 구별해야 함(숫자 시작 = 매개변수) |
| psql 변수 | `:var` · `:'var'`(리터럴 인용) · `:"var"`(식별자 인용) = **psql 클라이언트 기능**, 서버 문법 아님 · `::`(형 변환)와 혼동 주의 |
| 연산자 글자 | `+ - * / < > = ~ ! @ # % ^ & \| \` ?` · 최대 63자 · `--`·`/*` 포함 불가 · 여러 글자 연산자는 `~ ! @ # % ^ & \| \` ?` 중 하나가 없으면 `+`/`-`로 끝날 수 없음(`@-` 가능 · `*-` 불가) |
| 특수 문자 | `$`(매개변수·달러 인용) · `()` · `[]`(배열 첨자) · `,` · `;` · `:`(배열 조각 `a[1:2]`) · `*` · `.` |

출처: https://www.postgresql.org/docs/16/sql-syntax-lexical.html · https://www.postgresql.org/docs/16/app-psql.html

### T-3. 키워드(Appendix C)

PostgreSQL 열의 분류 넷 + "requires `AS`"(14+ · 열 별칭에 `AS` 없이 쓸 수 없음) 꼬리표.

| 분류 | 뜻 | 개수(수기 집계) |
|---|---|---|
| reserved | 어떤 이름으로도 못 씀(따옴표 필요) | **78**(= 56 + requires AS 22) |
| reserved (can be function or type) | 함수·형 이름으로는 가능, 테이블·컬럼 이름 불가 | **23**(= 20 + requires AS 3) |
| non-reserved (cannot be function or type) | 컬럼·테이블 이름 가능, 함수·형 이름 불가 | **55**(= 52 + requires AS 3) |
| non-reserved | 거의 어디서나 이름으로 가능(일부 문맥 제외) | 약 300(추정 · 수기 집계 ±수 개) |
| (빈칸) | PostgreSQL에서는 일반 식별자 | — |

**reserved (78)**
ALL, ANALYSE, ANALYZE, AND, ANY, ARRAY\*, AS\*, ASC, ASYMMETRIC, BOTH, CASE, CAST, CHECK, COLLATE, COLUMN, CONSTRAINT, CREATE\*, CURRENT_CATALOG, CURRENT_DATE, CURRENT_ROLE, CURRENT_TIME, CURRENT_TIMESTAMP, CURRENT_USER, DEFAULT, DEFERRABLE, DESC, DISTINCT, DO, ELSE, END, EXCEPT\*, FALSE, FETCH\*, FOR\*, FOREIGN, FROM\*, GRANT\*, GROUP\*, HAVING\*, IN, INITIALLY, INTERSECT\*, INTO\*, LATERAL, LEADING, LIMIT\*, LOCALTIME, LOCALTIMESTAMP, NOT, NULL, OFFSET\*, ON\*, ONLY, OR, ORDER\*, PLACING, PRIMARY, REFERENCES, RETURNING\*, SELECT, SESSION_USER, SOME, SYMMETRIC, SYSTEM_USER(16+), TABLE, THEN, TO\*, TRAILING, TRUE, UNION\*, UNIQUE, USER, USING, VARIADIC, WHEN, WHERE\*, WINDOW\*, WITH\*

**reserved (can be function or type) (23)**
AUTHORIZATION, BINARY, COLLATION, CONCURRENTLY, CROSS, CURRENT_SCHEMA, FREEZE, FULL, ILIKE, INNER, IS, ISNULL\*, JOIN, LEFT, LIKE, NATURAL, NOTNULL\*, OUTER, OVERLAPS\*, RIGHT, SIMILAR, TABLESAMPLE, VERBOSE

**non-reserved (cannot be function or type) (55)**
BETWEEN, BIGINT, BIT, BOOLEAN, CHAR\*, CHARACTER\*, COALESCE, DEC, DECIMAL, EXISTS, EXTRACT, FLOAT, GREATEST, GROUPING, INOUT, INT, INTEGER, INTERVAL, JSON_ARRAY, JSON_ARRAYAGG, JSON_OBJECT, JSON_OBJECTAGG(16+ 넷), LEAST, NATIONAL, NCHAR, NONE, NORMALIZE, NULLIF, NUMERIC, OUT, OVERLAY, POSITION, PRECISION\*, REAL, ROW, SETOF, SMALLINT, SUBSTRING, TIME, TIMESTAMP, TREAT, TRIM, VALUES, VARCHAR, XMLATTRIBUTES, XMLCONCAT, XMLELEMENT, XMLEXISTS, XMLFOREST, XMLNAMESPACES, XMLPARSE, XMLPI, XMLROOT, XMLSERIALIZE, XMLTABLE

**non-reserved 중 requires AS (11)**: DAY, FILTER, HOUR, MINUTE, MONTH, OVER, SECOND, VARYING, WITHIN, WITHOUT, YEAR

(\* = requires `AS` · 일반 non-reserved 약 300개 = ABORT … ZONE 은 원문 표 참조)

함의: Oracle·SQL Server와 달리 `USER`·`TABLE`·`COLUMN`·`ORDER`·`LIMIT`·`OFFSET`·`WINDOW`·`RETURNING`은 PG에서 예약어 → 그 이름의 컬럼은 `"…"` 필요. 반대로 `NAME`·`TYPE`·`KEY`·`LEVEL`·`COMMENT` 등은 비예약.

출처: https://www.postgresql.org/docs/16/sql-keywords-appendix.html

### T-4. 데이터 형

| 군 | 형(별칭) | 비고 |
|---|---|---|
| 정수 | `smallint`(int2) · `integer`(int, int4) · `bigint`(int8) | 2/4/8바이트 |
| 일련 | `smallserial`(serial2) · `serial`(serial4) · `bigserial`(serial8) | 진짜 형 아님 = 정수 + 소유 시퀀스 + `DEFAULT nextval()` · 권장 대체 = `GENERATED … AS IDENTITY` |
| 정밀 숫자 | `numeric(p,s)` = `decimal(p,s)` | p ≤ 1000(선언) · 15+ = 음수 scale·s>p 허용 · `NaN`·(14+) `Infinity` |
| 부동 | `real`(float4) · `double precision`(float8) · `float(p)` = p≤24 → real, 25~53 → double | `NaN` · `Infinity` |
| 화폐 | `money` | lc_monetary 의존 · 권장 안 함 |
| 문자 | `character varying(n)`(varchar) · `character(n)`(char, bpchar) · `text` | n = 글자 수 · 상한 약 1 GB · `varchar` 길이 없음 = 무제한 · char(n) 뒤 공백 무의미 |
| 내부 문자 | `"char"`(1바이트) · `name`(64바이트) | 카탈로그용 |
| 이진 | `bytea` | 출력 = hex(`\x…`) 기본 · escape 형식 선택(`bytea_output`) |
| 날짜·시각 | `date` · `time[(p)] [without time zone]` · `time[(p)] with time zone`(timetz) · `timestamp[(p)] [without time zone]` · `timestamp[(p)] with time zone`(timestamptz) · `interval [fields] [(p)]` | p = 0~6 · timestamptz = UTC 저장 + 세션 `TimeZone`으로 표시 · 특수값 `infinity`·`-infinity`·`epoch`·`now`·`today` |
| interval fields | `YEAR` `MONTH` `DAY` `HOUR` `MINUTE` `SECOND` `YEAR TO MONTH` `DAY TO HOUR` `DAY TO MINUTE` `DAY TO SECOND` `HOUR TO MINUTE` `HOUR TO SECOND` `MINUTE TO SECOND` | |
| 참거짓 | `boolean`(bool) | 입력 `t/true/yes/on/1` 등 |
| 열거 | `CREATE TYPE … AS ENUM (…)` | 정렬 = 선언 순 |
| 기하 | `point` `line` `lseg` `box` `path` `polygon` `circle` | |
| 네트워크 | `inet` `cidr` `macaddr` `macaddr8` | |
| 비트 | `bit(n)` · `bit varying(n)`(varbit) | |
| 전문 검색 | `tsvector` · `tsquery` | |
| UUID | `uuid` | 생성 `gen_random_uuid()`(13+ 내장) |
| XML | `xml` | `--with-libxml` 빌드 필요 |
| JSON | `json`(원문 보존) · `jsonb`(이진 · 키 정렬·중복 제거) · `jsonpath` | |
| 배열 | 모든 형 `T[]` · `T ARRAY[n]` | 차원·크기 선언은 강제 안 됨 · 첨자 1부터 |
| 복합 | `CREATE TYPE … AS (…)` · 모든 테이블의 행 형 | `(v).field` |
| 범위 | `int4range` `int8range` `numrange` `tsrange` `tstzrange` `daterange` | 사용자 정의 `CREATE TYPE … AS RANGE` |
| 다중 범위(14+) | `int4multirange` `int8multirange` `nummultirange` `tsmultirange` `tstzmultirange` `datemultirange` | |
| 도메인 | `CREATE DOMAIN` | 기반형 + 제약 |
| OID 별칭 | `oid` `regclass` `regcollation` `regconfig` `regdictionary` `regnamespace` `regoper` `regoperator` `regproc` `regprocedure` `regrole` `regtype` | `'t'::regclass`, `'f(int)'::regprocedure` |
| 기타 | `pg_lsn` · `pg_snapshot` · `txid_snapshot` · `xid` `xid8` `cid` `tid` · 의사형 `any` `anyelement` `anyarray` `anynonarray` `anyenum` `anyrange` `anymultirange` `anycompatible*` `record` `trigger` `event_trigger` `void` `internal` `cstring` `refcursor` | `refcursor` = 커서 이름(text) |

출처: https://www.postgresql.org/docs/16/datatype.html · https://www.postgresql.org/docs/16/datatype-numeric.html · https://www.postgresql.org/docs/16/datatype-datetime.html · https://www.postgresql.org/docs/16/datatype-oid.html · https://www.postgresql.org/docs/16/datatype-pseudo.html

### T-5. 연산자 · 우선순위 · NULL

| 순위(높음 → 낮음) | 연산자 | 결합 |
|---|---|---|
| 1 | `.` | 왼쪽 |
| 2 | `::` | 왼쪽 |
| 3 | `[ ]` | 왼쪽 |
| 4 | 단항 `+` `-` | 오른쪽 |
| 5 | `COLLATE` | 왼쪽 |
| 6 | `AT`(`AT TIME ZONE`) | 왼쪽 |
| 7 | `^` | 왼쪽(수학과 반대 · `2^3^2` = 64) |
| 8 | `*` `/` `%` | 왼쪽 |
| 9 | `+` `-` | 왼쪽 |
| 10 | 그 밖의 모든 연산자(`\|\|` · `~` · `@>` 등) | 왼쪽 |
| 11 | `BETWEEN` `IN` `LIKE` `ILIKE` `SIMILAR` | — |
| 12 | `<` `>` `=` `<=` `>=` `<>` | — |
| 13 | `IS` `ISNULL` `NOTNULL` | — |
| 14 | `NOT` | 오른쪽 |
| 15 | `AND` | 왼쪽 |
| 16 | `OR` | 왼쪽 |

| 주제 | 내용 |
|---|---|
| 같은 이름 사용자 연산자 | 내장과 같은 우선순위 · `OPERATOR(schema.op)` = "그 밖" 순위 |
| 문자열 연결 | `\|\|` · 피연산자 중 NULL이면 결과 NULL(Oracle과 다름 · `concat()`은 NULL 무시) · 빈 문자열 `''` ≠ NULL |
| 형 변환 | `expr::type` · `CAST(expr AS type)` |
| 패턴 | `LIKE` · `ILIKE`(대소문자 무시) · `~~` `~~*` `!~~` `!~~*` = 그 연산자 형 · `SIMILAR TO` · POSIX `~` `~*` `!~` `!~*` · `^@` 접두 |
| NULL 비교 | `=`·`<>` 결과 NULL · `IS [NOT] NULL` · `IS [NOT] DISTINCT FROM` · `ISNULL`/`NOTNULL`(비표준) · `transform_null_equals` |
| 정렬 NULL 위치 | **ASC(기본) = NULLS LAST · DESC = NULLS FIRST**(NULL을 가장 큰 값처럼) · `NULLS FIRST/LAST`로 지정 · `USING op`이면 연산자에 따라 |
| 콜레이션 | 열·식마다 `COLLATE "C"` / `"und-x-icu"` 등 · 제공자 libc/ICU(16 = ICU 기본 빌드) · 비결정적 콜레이션(대소문자 무시 등)은 ICU만 · 기본 = DB 생성 시 지정 |
| 비교 체인 | 행 생성자 비교 `(a,b) < (c,d)` · `= ANY(array)` · `<> ALL(...)` |

출처: https://www.postgresql.org/docs/16/sql-syntax-lexical.html#SQL-PRECEDENCE · https://www.postgresql.org/docs/16/queries-order.html · https://www.postgresql.org/docs/16/sql-select.html · https://www.postgresql.org/docs/16/functions-comparison.html · https://www.postgresql.org/docs/16/collation.html · https://www.postgresql.org/docs/16/functions-matching.html

### T-6. 함수(Chapter 9 분류)

| 절 | 분류 | 함수·구문 이름 |
|---|---|---|
| 9.1 | 논리 | `AND` `OR` `NOT` · `IS [NOT] TRUE/FALSE/UNKNOWN` |
| 9.2 | 비교 | `BETWEEN [SYMMETRIC]` · `IS [NOT] DISTINCT FROM` · `IS [NOT] NULL` · `num_nonnulls(VARIADIC "any")` · `num_nulls(...)` |
| 9.3 | 수학 | abs, cbrt, ceil, ceiling, degrees, div, erf, erfc, exp, factorial, floor, gcd, lcm, ln, log, log10, min_scale, mod, pi, power, radians, round, scale, sign, sqrt, trim_scale, trunc, width_bucket · 난수 random, random_normal(16+), setseed · 삼각 acos(d), asin(d), atan(d), atan2(d), cos(d), cot(d), sin(d), tan(d) · 쌍곡 sinh, cosh, tanh, asinh, acosh, atanh · 연산자 `+ - * / % ^ \|/ \|\|/ @ & \| # ~ << >>` |
| 9.4 | 문자열(SQL) | `\|\|`, btrim, `IS [NOT] [form] NORMALIZED`, bit_length, char_length, character_length, lower, lpad, ltrim, normalize, octet_length, `overlay(s PLACING t FROM n [FOR m])`, `position(sub IN s)`, rpad, rtrim, `substring(s [FROM n] [FOR m])` / `substring(s SIMILAR p ESCAPE e)`, `trim([LEADING\|TRAILING\|BOTH] [chars] FROM s)`, upper |
| 9.4 | 문자열(기타) | `^@`, ascii, chr, concat, concat_ws, `format(fmt, …)`(`%s %I %L`), initcap, left, length, md5, parse_ident, pg_client_encoding, quote_ident, quote_literal, quote_nullable, regexp_count, regexp_instr, regexp_like, regexp_match, regexp_matches, regexp_replace, regexp_split_to_array, regexp_split_to_table, regexp_substr, repeat, replace, reverse, right, split_part, starts_with, string_to_array, string_to_table, strpos, substr, to_ascii, to_hex, translate, unistr |
| 9.5 | 이진 문자열 | `\|\|`, bit_count, btrim, get_bit, get_byte, length, ltrim, md5, octet_length, overlay, position, rtrim, set_bit, set_byte, sha224, sha256, sha384, sha512, substr, substring, trim · convert, convert_from, convert_to, encode, decode(`base64` `escape` `hex`)(추정 = 목록은 기억) |
| 9.6 | 비트 문자열 | `\|\| & \| # ~ << >>`, bit_count, bit_length, length, octet_length, overlay, position, substring, get_bit, set_bit(추정) |
| 9.7 | 패턴 | `LIKE`/`ILIKE` [`ESCAPE`] · `SIMILAR TO` · POSIX `~ ~* !~ !~*` · regexp_* (9.4와 공유) |
| 9.8 | 형식화 | to_char, to_date, to_number, to_timestamp |
| 9.9 | 날짜·시각 | age, clock_timestamp, current_date, current_time, current_timestamp, date_add(16+), date_bin, date_part, date_subtract(16+), date_trunc, `EXTRACT(field FROM src)`, isfinite, justify_days, justify_hours, justify_interval, localtime, localtimestamp, make_date, make_interval, make_time, make_timestamp, make_timestamptz, now, statement_timestamp, timeofday, transaction_timestamp, to_timestamp, `AT TIME ZONE`, `OVERLAPS` · 지연 pg_sleep, pg_sleep_for, pg_sleep_until |
| 9.10 | 열거 | enum_first, enum_last, enum_range |
| 9.11 | 기하 | area, center, diagonal, diameter, height, isclosed, isopen, length, npoints, pclose, popen, radius, slope, width, box, bound_box, circle, line, lseg, path, point, polygon · 연산자 `@-@ @@ ## <-> && << >> &< &> <<\| \|>> &<\| \|&> <^ >^ ?# ?- ?\| ?-\| ?\|\| @> <@ ~=`(추정 = 목록은 기억) |
| 9.12 | 네트워크 | abbrev, broadcast, family, host, hostmask, inet_merge, inet_same_family, masklen, netmask, network, set_masklen, text, trunc(macaddr), macaddr8_set7bit · 연산자 `<< <<= >> >>= &&` |
| 9.13 | 전문 검색 | array_to_tsvector, get_current_ts_config, length, numnode, plainto_tsquery, phraseto_tsquery, websearch_to_tsquery, querytree, setweight, strip, to_tsquery, to_tsvector, json(b)_to_tsvector, ts_delete, ts_filter, ts_headline, ts_rank, ts_rank_cd, ts_rewrite, tsquery_phrase, tsvector_to_array, unnest, ts_debug, ts_lexize, ts_parse, ts_token_type, ts_stat · 연산자 `@@ \|\| && !! <-> @> <@` |
| 9.14 | UUID | gen_random_uuid |
| 9.15 | XML | xmltext(추정 = 17+), xmlcomment, xmlconcat, xmlelement, xmlforest, xmlpi, xmlroot, xmlagg, `IS DOCUMENT`, `XMLEXISTS`, xml_is_well_formed(_document/_content), xpath, xpath_exists, `XMLTABLE`, table_to_xml, query_to_xml, cursor_to_xml, schema_to_xml, database_to_xml 등 *_to_xml(schema) |
| 9.16 | JSON | 연산자 `-> ->> #> #>> @> <@ ? ?\| ?& \|\| - #- @? @@` · 생성 to_json(b), array_to_json, row_to_json, json(b)_build_array, json(b)_build_object, json(b)_object, **JSON_ARRAY · JSON_OBJECT(16+)** · `IS [NOT] JSON [VALUE\|ARRAY\|OBJECT\|SCALAR]`(16+) · 처리 json(b)_array_elements(_text), json(b)_array_length, json(b)_each(_text), json(b)_extract_path(_text), json(b)_object_keys, json(b)_populate_record(set), json(b)_to_record(set), jsonb_set, jsonb_set_lax, jsonb_insert, json(b)_strip_nulls, jsonb_path_exists/match/query/query_array/query_first(+`_tz`), jsonb_pretty, json(b)_typeof · `JSON_TABLE`·`JSON_QUERY`·`JSON_VALUE`·`JSON_EXISTS` = **17+(16에 없음)** |
| 9.17 | 시퀀스 | nextval, currval, setval, lastval |
| 9.18 | 조건 | `CASE` · `COALESCE` · `NULLIF` · `GREATEST` · `LEAST`(NULL 무시 · Oracle과 다름) |
| 9.19 | 배열 | 연산자 `@> <@ && \|\|` · array_append, array_cat, array_dims, array_fill, array_length, array_lower, array_ndims, array_position, array_positions, array_prepend, array_remove, array_replace, array_sample(16+), array_shuffle(16+), array_to_string, array_upper, cardinality, trim_array, unnest |
| 9.20 | 범위·다중 범위 | lower, upper, isempty, lower_inc, upper_inc, lower_inf, upper_inf, range_merge, multirange, unnest · 연산자 `@> <@ && << >> &< &> -\|- + * -` |
| 9.21 | 집계(일반) | any_value(16+), array_agg, avg, bit_and, bit_or, bit_xor, bool_and, bool_or, count, every, json_agg, jsonb_agg, json(b)_agg_strict(16+), JSON_ARRAYAGG·JSON_OBJECTAGG(16+), json(b)_object_agg(+ _strict/_unique/_unique_strict 16+), max, min, range_agg, range_intersect_agg, string_agg, sum, xmlagg |
| 9.21 | 집계(통계) | corr, covar_pop, covar_samp, regr_avgx, regr_avgy, regr_count, regr_intercept, regr_r2, regr_slope, regr_sxx, regr_sxy, regr_syy, stddev, stddev_pop, stddev_samp, variance, var_pop, var_samp |
| 9.21 | 순서 집합 · 가상 집합 | `mode() WITHIN GROUP (ORDER BY …)`, percentile_cont, percentile_disc · rank, dense_rank, percent_rank, cume_dist · `GROUPING(…)` · 집계 공통 `FILTER (WHERE …)` · `ORDER BY` 인자 내부 |
| 9.22 | 윈도 | row_number, rank, dense_rank, percent_rank, cume_dist, ntile, lag, lead, first_value, last_value, nth_value · `OVER (PARTITION BY … ORDER BY … frame)` · frame `ROWS\|RANGE\|GROUPS` · `EXCLUDE CURRENT ROW\|GROUP\|TIES\|NO OTHERS` · `IGNORE NULLS` 없음 |
| — | MERGE 지원 | `merge_action()` = **17+**(16에 해당 절 없음) |
| 9.23 | 서브쿼리 식 | `EXISTS` `IN` `NOT IN` `ANY`/`SOME` `ALL` · 단일 행 비교 |
| 9.24 | 행·배열 비교 | `IN` `NOT IN` `ANY`/`SOME (array)` `ALL (array)` · 행 생성자 비교 · 복합형 비교 |
| 9.25 | 집합 반환 | generate_series(정수·numeric·timestamp), generate_subscripts · `WITH ORDINALITY` · `ROWS FROM(...)` |
| 9.26 | 시스템 정보 | current_catalog, current_database, current_query, current_role, current_schema, current_schemas, current_user, inet_client_addr/port, inet_server_addr/port, pg_backend_pid, pg_blocking_pids, pg_conf_load_time, pg_current_logfile, pg_my_temp_schema, pg_is_other_temp_schema, pg_jit_available, pg_listening_channels, pg_notification_queue_usage, pg_postmaster_start_time, pg_safe_snapshot_blocking_pids, pg_trigger_depth, session_user, system_user(16+), user, version · has_*_privilege(any_column, column, database, foreign_data_wrapper, function, language, parameter, schema, sequence, server, table, tablespace, type), pg_has_role, row_security_active · pg_*_is_visible · format_type, pg_get_catalog_foreign_keys, **pg_get_constraintdef, pg_get_expr, pg_get_functiondef, pg_get_function_arguments, pg_get_function_identity_arguments, pg_get_function_result, pg_get_indexdef, pg_get_keywords, pg_get_ruledef, pg_get_serial_sequence, pg_get_statisticsobjdef, pg_get_triggerdef, pg_get_userbyid, pg_get_viewdef**, pg_index_column_has_property, pg_index_has_property, pg_indexam_has_property, pg_options_to_table, pg_settings_get_flags, pg_tablespace_databases, pg_tablespace_location, pg_typeof, COLLATION FOR, to_regclass/regcollation/regnamespace/regoper/regoperator/regproc/regprocedure/regrole/regtype, to_regtypemod(추정 17+) · pg_describe_object, pg_identify_object(_as_address), pg_get_object_address · col_description, obj_description, shobj_description · pg_input_is_valid, pg_input_error_info(16+) · pg_current_xact_id, pg_xact_status, pg_current_snapshot 등 · pg_control_* |
| 9.27 | 시스템 관리 | current_setting, set_config · pg_cancel_backend, pg_terminate_backend, pg_reload_conf, pg_rotate_logfile, pg_log_backend_memory_contexts · pg_backup_start/stop(15+ 이름) · pg_is_in_recovery, pg_wal_replay_pause/resume, pg_promote · pg_export_snapshot · pg_create_*_replication_slot, pg_drop_replication_slot, pg_logical_slot_get_changes 등 · pg_column_size, pg_database_size, pg_indexes_size, pg_relation_size, pg_size_bytes, pg_size_pretty, pg_table_size, pg_tablespace_size, pg_total_relation_size, pg_relation_filenode, pg_relation_filepath, pg_filenode_relation · brin_summarize_new_values, gin_clean_pending_list 등 · pg_ls_dir, pg_read_file, pg_read_binary_file, pg_stat_file, pg_ls_logdir 등 · pg_advisory_lock(_shared), pg_advisory_unlock(_all), pg_try_advisory_lock, pg_advisory_xact_lock 등 |
| 9.28 | 트리거 | suppress_redundant_updates_trigger, tsvector_update_trigger(_column) |
| 9.29 | 이벤트 트리거 | pg_event_trigger_ddl_commands, pg_event_trigger_dropped_objects, pg_event_trigger_table_rewrite_oid, pg_event_trigger_table_rewrite_reason |
| 9.30 | 통계 | pg_mcv_list_items |

자주 쓰는 서명:

| 함수 | 서명 |
|---|---|
| `to_char` | `to_char(timestamp\|interval\|numeric, text) → text` |
| `date_trunc` | `date_trunc(field text, timestamp[tz] [, zone text]) → timestamp[tz]` |
| `string_agg` | `string_agg(value text, delimiter text [ORDER BY …]) → text` |
| `generate_series` | `generate_series(start, stop [, step]) → setof …` |
| `coalesce` | `COALESCE(v1, v2, …)` — 첫 비 NULL |
| `format` | `format(fmt text, VARIADIC "any") → text` · `%I` 식별자 인용 · `%L` 리터럴 인용 |
| `regexp_replace` | `regexp_replace(s, pattern, repl [, start [, N]] [, flags])` · flags `g i …` |
| `pg_get_functiondef` | `pg_get_functiondef(func oid) → text`(CREATE OR REPLACE 전체) |
| `pg_get_viewdef` | `pg_get_viewdef(view oid [, pretty bool \| wrap int]) → text`(SELECT 본문만) |

출처: https://www.postgresql.org/docs/16/functions.html · https://www.postgresql.org/docs/16/functions-math.html · https://www.postgresql.org/docs/16/functions-string.html · https://www.postgresql.org/docs/16/functions-binarystring.html · https://www.postgresql.org/docs/16/functions-datetime.html · https://www.postgresql.org/docs/16/functions-json.html · https://www.postgresql.org/docs/16/functions-array.html · https://www.postgresql.org/docs/16/functions-aggregate.html · https://www.postgresql.org/docs/16/functions-window.html · https://www.postgresql.org/docs/16/functions-info.html · https://www.postgresql.org/docs/16/functions-admin.html · https://www.postgresql.org/docs/16/functions-event-triggers.html · https://www.postgresql.org/docs/17/functions-merge-support.html

### T-7. 질의(SELECT)

| 항목 | 문법 · 메모 |
|---|---|
| 절 순서 | `[WITH [RECURSIVE] …] SELECT [ALL\|DISTINCT [ON (…)]] 목록 [FROM …] [WHERE …] [GROUP BY [ALL\|DISTINCT] …] [HAVING …] [WINDOW w AS (…)] [{UNION\|INTERSECT\|EXCEPT} [ALL\|DISTINCT] select] [ORDER BY … [ASC\|DESC\|USING op] [NULLS {FIRST\|LAST}]] [LIMIT {n\|ALL}] [OFFSET n [ROW\|ROWS]] [FETCH {FIRST\|NEXT} [n] {ROW\|ROWS} {ONLY\|WITH TIES}] [FOR …]` |
| FROM 생략 | 가능(`SELECT 1` · `DUAL` 불필요) |
| 행 제한 | `LIMIT/OFFSET`(PG 고유) · `OFFSET … FETCH FIRST n ROWS ONLY`(표준) · `WITH TIES`(13+ · ORDER BY 필요) |
| DISTINCT ON | `SELECT DISTINCT ON (a) …` — 각 a의 첫 행 · ORDER BY 앞부분이 일치해야 |
| CTE | `WITH q [(cols)] AS [[NOT] MATERIALIZED] (select\|values\|insert\|update\|delete)` · 데이터 변경 CTE + RETURNING · 12+ 기본 인라인(1회 참조·부작용 없을 때) |
| 재귀 CTE | `WITH RECURSIVE` · `SEARCH {BREADTH\|DEPTH} FIRST BY … SET col` · `CYCLE … SET mark [TO v DEFAULT d] USING path`(14+) |
| 집합 연산 | `UNION` / `INTERSECT` / `EXCEPT`(`MINUS` 없음) [`ALL`] · `INTERSECT`가 먼저 결합 |
| 조인 | `[INNER] JOIN` · `LEFT/RIGHT/FULL [OUTER] JOIN` · `CROSS JOIN` · `NATURAL` · `USING (…) [AS alias]`(14+) · `LATERAL` 서브쿼리·함수 · `(+)` 외부 조인 표기 없음 · `CROSS/OUTER APPLY` 없음(= `LATERAL`) |
| 표본 | `TABLESAMPLE {BERNOULLI\|SYSTEM} (pct) [REPEATABLE (seed)]` |
| 잠금 | `FOR UPDATE` / `FOR NO KEY UPDATE` / `FOR SHARE` / `FOR KEY SHARE` [`OF t…`] [`NOWAIT`\|`SKIP LOCKED`] — 여러 개 나열 가능 |
| 윈도 | `WINDOW w AS (PARTITION BY … ORDER BY … frame)` · `OVER w` |
| 그룹 | `GROUPING SETS (…)` · `ROLLUP (…)` · `CUBE (…)` · `()` · `GROUPING()` |
| 기타 | `TABLE t` = `SELECT * FROM t` · `VALUES (…),(…)` 독립 문장 · `SELECT INTO new_table`(= CREATE TABLE AS · PL/pgSQL 안에서는 변수 대입) · `ONLY t` / `t*` 상속 |
| PIVOT | **없음** — `tablefunc` 확장의 `crosstab()` 또는 `FILTER`/`CASE` 집계 |
| 계층 질의 | `CONNECT BY` 없음 — 재귀 CTE |

출처: https://www.postgresql.org/docs/16/sql-select.html · https://www.postgresql.org/docs/16/queries-with.html · https://www.postgresql.org/docs/16/tablefunc.html

### T-8. DML

| 문 | 문법 핵심 | 메모 |
|---|---|---|
| INSERT | `INSERT INTO t [AS a] [(cols)] [OVERRIDING {SYSTEM\|USER} VALUE] {DEFAULT VALUES\|VALUES (…),…\|query} [ON CONFLICT [target] action] [RETURNING …]` | 다중 행 VALUES |
| ON CONFLICT | target = `(col\|(expr) [, …]) [WHERE pred]` 또는 `ON CONSTRAINT name` · action = `DO NOTHING` / `DO UPDATE SET … [WHERE …]` · 새 값 = `EXCLUDED.col` | UPSERT · DO UPDATE는 target 필수 |
| UPDATE | `UPDATE [ONLY] t [AS a] SET col = expr \| (c1,c2) = (…) \| (c1,c2) = (subquery) [FROM …] [WHERE … \| WHERE CURRENT OF cursor] [RETURNING …]` | FROM 조인 갱신 |
| DELETE | `DELETE FROM [ONLY] t [AS a] [USING …] [WHERE … \| WHERE CURRENT OF c] [RETURNING …]` | USING 조인 삭제 · LIMIT 없음 |
| RETURNING | INSERT/UPDATE/DELETE 모두 · `*` 또는 식 · 결과 집합을 돌려줌(클라이언트는 SELECT처럼 받음) | 16의 MERGE엔 없음(17+) |
| MERGE(15+) | `[WITH …] MERGE INTO t [AS a] USING src ON cond WHEN MATCHED [AND c] THEN {UPDATE SET …\|DELETE\|DO NOTHING} WHEN NOT MATCHED [AND c] THEN {INSERT … \| DO NOTHING}` | 16엔 `WHEN NOT MATCHED BY SOURCE`·`RETURNING`·뷰 대상 없음(17+) · 동시성 = ON CONFLICT보다 약함 |
| TRUNCATE | `TRUNCATE [TABLE] [ONLY] t [*], … [RESTART\|CONTINUE IDENTITY] [CASCADE\|RESTRICT]` | **트랜잭션 안 · 롤백 가능** |
| COPY | `COPY t [(cols)] FROM {'file'\|PROGRAM 'cmd'\|STDIN} [WITH (FORMAT text\|csv\|binary, HEADER [MATCH], DELIMITER, NULL, DEFAULT(16+), QUOTE, ESCAPE, FORCE_QUOTE, FORCE_NOT_NULL, FORCE_NULL, ENCODING, FREEZE)] [WHERE …]` · `COPY {t\|(query)} TO {'file'\|PROGRAM\|STDOUT}` | 서버 파일 = 슈퍼유저/`pg_read_server_files` · 클라이언트는 STDIN/STDOUT(psql `\copy`) |

출처: https://www.postgresql.org/docs/16/sql-insert.html · https://www.postgresql.org/docs/16/sql-update.html · https://www.postgresql.org/docs/16/sql-delete.html · https://www.postgresql.org/docs/16/sql-merge.html · https://www.postgresql.org/docs/16/sql-truncate.html · https://www.postgresql.org/docs/16/sql-copy.html

### T-9. 객체 종류

공통: 이름 = `[database.]schema.object` — **다른 DB 참조 불가**(database 부분은 현재 DB와 같아야 함 · 다른 DB = `dblink`/`postgres_fdw`) · 스키마 없는 이름은 `search_path`로 해석 · 의존성 = `pg_depend`(객체 간) · `pg_shdepend`(공유 객체 · 역할) · DROP 기본 `RESTRICT` / `CASCADE` · 거의 모든 DROP에 `IF EXISTS` · **DDL은 트랜잭션 안에서 롤백 가능**(예외 = `CREATE/DROP DATABASE`·`TABLESPACE`, `CREATE INDEX CONCURRENTLY`, `ALTER SYSTEM`, `VACUUM` 등).

| 객체 | CREATE / ALTER / DROP 핵심 | OR REPLACE · IF [NOT] EXISTS | 카탈로그 · 소스 | 메모 |
|---|---|---|---|---|
| TABLE | `CREATE [{TEMP\|UNLOGGED}] TABLE [IF NOT EXISTS] t (col type [constraints], … [LIKE src], table_constraint) [INHERITS (p)] [PARTITION BY {RANGE\|LIST\|HASH} (…)] [USING am] [WITH (…)] [ON COMMIT {PRESERVE ROWS\|DELETE ROWS\|DROP}] [TABLESPACE ts]` · `CREATE TABLE p PARTITION OF parent FOR VALUES {IN(…)\|FROM(…) TO(…)\|WITH (MODULUS m, REMAINDER r)\|DEFAULT}` · `CREATE TABLE AS` · `ALTER TABLE … ADD/DROP/ALTER COLUMN, ADD/DROP CONSTRAINT, ATTACH/DETACH PARTITION [CONCURRENTLY], RENAME, SET SCHEMA, OWNER TO, ENABLE/DISABLE TRIGGER, ENABLE ROW LEVEL SECURITY` | IF NOT EXISTS · OR REPLACE 없음 | `pg_class`(relkind `r` 일반 · `p` 분할) · `pg_attribute` · `pg_inherits` · `pg_partitioned_table` · **DDL 생성 함수 없음**(클라이언트가 카탈로그로 조립 · pg_dump 방식) | 임시 = `pg_temp_N` 스키마 · UNLOGGED = WAL 없음 · 상속(레거시) |
| FOREIGN TABLE | `CREATE FOREIGN TABLE t (…) SERVER s OPTIONS (…)` · `IMPORT FOREIGN SCHEMA` | IF NOT EXISTS | `pg_class` relkind `f` · `pg_foreign_table` | |
| VIEW | `CREATE [OR REPLACE] [TEMP] [RECURSIVE] VIEW v [(cols)] [WITH (check_option, security_barrier, security_invoker(15+))] AS query [WITH [CASCADED\|LOCAL] CHECK OPTION]` | OR REPLACE(컬럼 추가만 · 이름·형 변경 불가) · DROP IF EXISTS | relkind `v` · `pg_rewrite` · `pg_get_viewdef` · `pg_views` | 단순 뷰 = 자동 갱신 가능 · 그 밖 = INSTEAD OF 트리거/규칙 |
| MATERIALIZED VIEW | `CREATE MATERIALIZED VIEW [IF NOT EXISTS] mv AS query [WITH [NO] DATA]` · `REFRESH MATERIALIZED VIEW [CONCURRENTLY] mv [WITH [NO] DATA]` | IF NOT EXISTS · OR REPLACE 없음 | relkind `m` · `pg_matviews` · `pg_get_viewdef` | CONCURRENTLY = 유니크 인덱스 필요 · 자동 갱신 없음 |
| SEQUENCE | `CREATE [TEMP\|UNLOGGED] SEQUENCE [IF NOT EXISTS] s [AS type] [INCREMENT] [MINVALUE] [MAXVALUE] [START] [CACHE] [[NO] CYCLE] [OWNED BY t.c]` | IF NOT EXISTS | relkind `S` · `pg_sequence` · `pg_sequences` | `nextval('s')` · 롤백되지 않음 |
| IDENTITY / serial | `col int GENERATED {ALWAYS\|BY DEFAULT} AS IDENTITY [(seq opts)]` · serial = 시퀀스 + DEFAULT | — | `pg_attribute.attidentity` · `pg_get_serial_sequence` | 생성 열 `GENERATED ALWAYS AS (expr) STORED`(12+ · STORED만) |
| INDEX | `CREATE [UNIQUE] INDEX [CONCURRENTLY] [IF NOT EXISTS] [name] ON [ONLY] t [USING {btree\|hash\|gist\|spgist\|gin\|brin}] ({col\|(expr)} [COLLATE] [opclass] [ASC\|DESC] [NULLS …], …) [INCLUDE (…)] [NULLS [NOT] DISTINCT(15+)] [WITH (…)] [TABLESPACE] [WHERE pred]` · `REINDEX [CONCURRENTLY]` · `ALTER INDEX` | IF NOT EXISTS | relkind `i`/`I`(분할) · `pg_index` · `pg_am` · `pg_get_indexdef` · `pg_indexes` | 부분 = WHERE · 식 = `(expr)` · 덮개 = INCLUDE · 이름은 스키마 안에서 테이블과 같은 이름공간 |
| 제약 | `PRIMARY KEY` · `UNIQUE [NULLS NOT DISTINCT]` · `FOREIGN KEY … REFERENCES … [MATCH FULL\|SIMPLE] [ON DELETE/UPDATE {NO ACTION\|RESTRICT\|CASCADE\|SET NULL [(cols)]\|SET DEFAULT}]` · `CHECK (…) [NO INHERIT]` · `EXCLUDE USING gist (c WITH &&, …)` · `NOT NULL`(16 = 제약 행 없음 · `attnotnull`) · `[NOT] DEFERRABLE [INITIALLY {DEFERRED\|IMMEDIATE}]` · `NOT VALID` → `VALIDATE CONSTRAINT` | ALTER TABLE 안 | `pg_constraint`(contype p u f c x t) · `pg_get_constraintdef` | `SET CONSTRAINTS … DEFERRED` |
| TRIGGER | `CREATE [OR REPLACE(14+)] [CONSTRAINT] TRIGGER name {BEFORE\|AFTER\|INSTEAD OF} {INSERT\|UPDATE [OF cols]\|DELETE\|TRUNCATE} [OR …] ON t [REFERENCING {OLD\|NEW} TABLE AS n] [FOR [EACH] {ROW\|STATEMENT}] [WHEN (cond)] EXECUTE {FUNCTION\|PROCEDURE} f(args)` · `ALTER TRIGGER … RENAME` · `ALTER TABLE … {ENABLE\|DISABLE} TRIGGER` · `DROP TRIGGER [IF EXISTS] name ON t` | OR REPLACE(14+) · IF EXISTS | `pg_trigger`(tgenabled O/D/R/A) · `pg_get_triggerdef` | 본문 = 별도 `RETURNS trigger` 함수 · 이름은 **테이블 안에서** 유일 · INSTEAD OF = 뷰·행만 |
| EVENT TRIGGER | `CREATE EVENT TRIGGER n ON {ddl_command_start\|ddl_command_end\|sql_drop\|table_rewrite} [WHEN TAG IN (…)] EXECUTE FUNCTION f()` | — | `pg_event_trigger` | 슈퍼유저 · 함수 `RETURNS event_trigger` |
| FUNCTION | `CREATE [OR REPLACE] FUNCTION f(args) RETURNS … LANGUAGE … [IMMUTABLE\|STABLE\|VOLATILE] [STRICT] [SECURITY DEFINER] [PARALLEL …] [COST/ROWS] [SET param …] AS $$…$$ \| BEGIN ATOMIC … END` | OR REPLACE(반환형 변경 불가) · DROP IF EXISTS | `pg_proc`(prokind `f`) · `pg_get_functiondef` · `pg_get_function_identity_arguments` | **오버로드 가능**(인자 형 = 식별 · `f(int)`·`f(text)`) · 식별 키 = oid / `regprocedure` 서명 · DROP·ALTER·GRANT·COMMENT에 인자 형 목록 필요(유일하면 생략 가능 10+) |
| PROCEDURE(11+) | `CREATE [OR REPLACE] PROCEDURE p(args) LANGUAGE … AS …` · `CALL p(…)` | 같음 | `pg_proc` prokind `p` · `pg_get_functiondef` | 오버로드 가능 · 본문에서 COMMIT/ROLLBACK 가능(CALL이 트랜잭션 블록 밖일 때) · 반환값 없음(OUT 매개변수는 행으로) |
| AGGREGATE | `CREATE [OR REPLACE] AGGREGATE a(type) (SFUNC=…, STYPE=…, FINALFUNC=…, INITCOND=…, …)` | OR REPLACE(12+ · 추정) | `pg_aggregate` + `pg_proc` prokind `a` | 오버로드 가능 · 윈도 함수 = prokind `w` |
| OPERATOR | `CREATE OPERATOR op (LEFTARG, RIGHTARG, FUNCTION, COMMUTATOR, NEGATOR, …)` · CLASS / FAMILY | — | `pg_operator` · `pg_opclass` · `pg_opfamily` | 피연산자 형으로 오버로드 · 식별 = `regoperator` `+(int,int)` |
| TYPE | `CREATE TYPE t AS (…)`(복합) · `AS ENUM (…)` · `AS RANGE (SUBTYPE=…)` · 기본형(입출력 함수 · C) · 껍데기 `CREATE TYPE t` · `ALTER TYPE … ADD VALUE` | DROP IF EXISTS | `pg_type`(typtype b c d e p r m) · `pg_enum` · `pg_range` | 테이블마다 같은 이름의 복합형이 생김(이름 충돌) |
| DOMAIN | `CREATE DOMAIN d AS base [DEFAULT] [CONSTRAINT … CHECK (VALUE …)] [NOT NULL]` | DROP IF EXISTS | `pg_type` typtype `d` · `pg_constraint` | |
| SCHEMA | `CREATE SCHEMA [IF NOT EXISTS] s [AUTHORIZATION role]` | IF NOT EXISTS | `pg_namespace` | 기본 `public` · 사용자별 스키마 관례 `"$user"` |
| DATABASE | `CREATE DATABASE d [OWNER] [TEMPLATE] [ENCODING] [LOCALE/LC_COLLATE/LC_CTYPE] [LOCALE_PROVIDER] [ICU_LOCALE] [STRATEGY(15+)] [TABLESPACE] [CONNECTION LIMIT]` | DROP IF EXISTS [WITH (FORCE)] | `pg_database`(공유) | 트랜잭션 블록 안 불가 · 접속 = DB 단위(세션 중 전환 불가 · `USE` 없음) |
| ROLE / USER / GROUP | `CREATE ROLE r [LOGIN] [SUPERUSER] [CREATEDB] [CREATEROLE] [REPLICATION] [BYPASSRLS] [PASSWORD '…'] [VALID UNTIL] [IN ROLE …]` · `CREATE USER` = `LOGIN` 기본 · `ALTER ROLE … SET param` | DROP IF EXISTS | `pg_authid`/`pg_roles` · `pg_auth_members` | 클러스터 공유 · 16 = 역할 부여 `WITH ADMIN/INHERIT/SET` 옵션 |
| EXTENSION | `CREATE EXTENSION [IF NOT EXISTS] e [SCHEMA s] [VERSION v] [CASCADE]` · `ALTER EXTENSION … UPDATE` | IF NOT EXISTS | `pg_extension` · `pg_available_extensions` | 소속 객체 = `pg_depend` deptype `e` |
| FDW / SERVER / USER MAPPING | `CREATE FOREIGN DATA WRAPPER` · `CREATE SERVER [IF NOT EXISTS] s FOREIGN DATA WRAPPER w OPTIONS (…)` · `CREATE USER MAPPING [IF NOT EXISTS] FOR role SERVER s OPTIONS (user, password)` | IF NOT EXISTS | `pg_foreign_data_wrapper` · `pg_foreign_server` · `pg_user_mapping`(`pg_user_mappings` 뷰) | |
| PUBLICATION / SUBSCRIPTION | `CREATE PUBLICATION p FOR {ALL TABLES\|TABLE t [(cols)] [WHERE (…)]\|TABLES IN SCHEMA s}` · `CREATE SUBSCRIPTION s CONNECTION '…' PUBLICATION p` | — | `pg_publication` · `pg_publication_rel` · `pg_subscription` | 논리 복제 · 열 목록·행 필터·스키마 = 15+ · CREATE SUBSCRIPTION은 트랜잭션 블록 불가(슬롯 생성 시 · 추정) |
| POLICY(RLS) | `CREATE POLICY n ON t [AS {PERMISSIVE\|RESTRICTIVE}] [FOR {ALL\|SELECT\|INSERT\|UPDATE\|DELETE}] [TO roles] [USING (…)] [WITH CHECK (…)]` + `ALTER TABLE t ENABLE [FORCE] ROW LEVEL SECURITY` | DROP IF EXISTS | `pg_policy` · `pg_policies` | 소유자·BYPASSRLS는 우회(FORCE 제외) |
| RULE | `CREATE [OR REPLACE] RULE n AS ON {SELECT\|INSERT\|UPDATE\|DELETE} TO t [WHERE] DO [ALSO\|INSTEAD] {NOTHING\|command}` | OR REPLACE | `pg_rewrite` · `pg_get_ruledef` · `pg_rules` | 레거시 · 뷰의 내부 구현 |
| COLLATION | `CREATE COLLATION [IF NOT EXISTS] c (provider = icu, locale = '…', deterministic = false)` · `FROM existing` | IF NOT EXISTS | `pg_collation` | |
| CAST | `CREATE CAST (src AS tgt) {WITH FUNCTION f\|WITHOUT FUNCTION\|WITH INOUT} [AS ASSIGNMENT\|AS IMPLICIT]` | — | `pg_cast` | 이름 없음 · 식별 = 형 쌍 |
| CONVERSION | `CREATE [DEFAULT] CONVERSION c FOR 'src' TO 'dst' FROM f` | — | `pg_conversion` | |
| TABLESPACE | `CREATE TABLESPACE ts [OWNER] LOCATION '/dir'` | DROP IF EXISTS | `pg_tablespace`(공유) | 트랜잭션 블록 불가 |
| 기타 | STATISTICS(`pg_statistic_ext`) · TEXT SEARCH CONFIGURATION/DICTIONARY/PARSER/TEMPLATE · LANGUAGE · ACCESS METHOD · TRANSFORM · LARGE OBJECT · COMMENT ON | — | `pg_description` · `obj_description()` | |

상태·유효성: 오라클식 `VALID/INVALID` 상태 **없음** — 의존 객체는 DROP 시 오류(또는 CASCADE로 함께 삭제)라 깨진 객체가 남지 않음 · 단 PL/pgSQL 본문은 생성 때 구문만 검사하고 참조 객체는 실행 때 해석(깨진 함수가 생길 수 있음 · `check_function_bodies`) · 트리거 활성 = `pg_trigger.tgenabled` · 인덱스 = `pg_index.indisvalid`(CONCURRENTLY 실패 시 invalid) · 제약 = `pg_constraint.convalidated`(NOT VALID).

information_schema: `tables` · `columns` · `views` · `routines` · `parameters` · `table_constraints` · `key_column_usage` · `referential_constraints` · `triggers` · `sequences` · `schemata` 등 — 표준 · 이식성 있으나 PG 고유 객체(인덱스·정책 등) 없음 · 권한 있는 객체만 보임.

출처: https://www.postgresql.org/docs/16/sql-createtable.html · https://www.postgresql.org/docs/16/sql-createview.html · https://www.postgresql.org/docs/16/sql-creatematerializedview.html · https://www.postgresql.org/docs/16/sql-createsequence.html · https://www.postgresql.org/docs/16/sql-createindex.html · https://www.postgresql.org/docs/16/ddl-constraints.html · https://www.postgresql.org/docs/16/sql-createtrigger.html · https://www.postgresql.org/docs/16/sql-createeventtrigger.html · https://www.postgresql.org/docs/16/sql-createfunction.html · https://www.postgresql.org/docs/16/sql-createprocedure.html · https://www.postgresql.org/docs/16/sql-createaggregate.html · https://www.postgresql.org/docs/16/sql-createtype.html · https://www.postgresql.org/docs/16/sql-createpolicy.html · https://www.postgresql.org/docs/16/sql-createpublication.html · https://www.postgresql.org/docs/16/ddl-partitioning.html · https://www.postgresql.org/docs/16/catalog-pg-class.html · https://www.postgresql.org/docs/16/catalog-pg-proc.html · https://www.postgresql.org/docs/16/catalog-pg-depend.html · https://www.postgresql.org/docs/16/information-schema.html · https://www.postgresql.org/docs/16/sql-syntax-lexical.html#SQL-SYNTAX-IDENTIFIERS

### T-10. 루틴(함수·프로시저)

| 항목 | 내용 |
|---|---|
| 인자 모드 | `IN`(기본) · `OUT` · `INOUT` · `VARIADIC`(마지막 · 배열) · 함수의 OUT = 결과 행 열 · 프로시저 OUT = 11~13은 불가, **14+ 가능**(CALL 결과 행) |
| 기본값 | `arg type DEFAULT expr` 또는 `= expr` · 뒤쪽 인자만 생략 가능 |
| 이름 표기 | `f(a => 1, b => 2)` · 옛 `:=` 도 허용 · 위치 + 이름 혼합(위치 먼저) |
| 반환 | 스칼라 · `SETOF type` · `TABLE (col type, …)` · `record`(호출 때 열 정의) · `refcursor` · `void` · `trigger` · `event_trigger` |
| 언어 | `sql` · `plpgsql` · `c` · `internal` · 확장 `plpython3u` `plperl` `pltcl` 등 · `pg_language` |
| 본문 | `AS $$ … $$`(문자열 · 생성 때 plpgsql은 구문만 검사) · **SQL 표준 본문 `BEGIN ATOMIC … END`(14+ · LANGUAGE sql만 · 생성 때 해석·의존성 기록)** · `RETURN expr`(14+ sql 한 줄) |
| 특성 | `IMMUTABLE/STABLE/VOLATILE` · `[NOT] LEAKPROOF` · `STRICT`(= RETURNS NULL ON NULL INPUT) · `SECURITY {INVOKER\|DEFINER}` · `PARALLEL {UNSAFE\|RESTRICTED\|SAFE}` · `COST` · `ROWS` · `SUPPORT` · `SET param = v` |
| 익명 블록 | `DO [LANGUAGE plpgsql] $$ … $$` — 인자·반환 없음 · 바인드 매개변수 불가 |
| 호출 | 함수 = `SELECT f(…)` / `SELECT * FROM f(…)`(집합) · 프로시저 = `CALL p(…)`(SELECT 불가) · 함수는 `CALL` 불가 |
| 결과 집합 | 함수 `RETURNS SETOF`/`TABLE` = 행 직접 · refcursor 반환 = 같은 트랜잭션 안에서 `FETCH ALL FROM "커서명"`(자동 커밋이면 커서가 사라짐) · `RETURN QUERY` |
| PL/pgSQL | `DECLARE … BEGIN … EXCEPTION WHEN … THEN … END` · `RAISE` · `PERFORM` · `EXECUTE … USING` · `GET DIAGNOSTICS` · `FOUND` · `%TYPE`/`%ROWTYPE` · 패키지 없음(스키마로 대체) |

출처: https://www.postgresql.org/docs/16/sql-createfunction.html · https://www.postgresql.org/docs/16/sql-createprocedure.html · https://www.postgresql.org/docs/16/sql-call.html · https://www.postgresql.org/docs/16/sql-do.html · https://www.postgresql.org/docs/16/sql-syntax-calling-funcs.html · https://www.postgresql.org/docs/16/xfunc-sql.html · https://www.postgresql.org/docs/16/plpgsql-cursors.html

### T-11. 트랜잭션 · 세션

| 항목 | 내용 |
|---|---|
| 자동 커밋 | 서버는 블록 밖 각 문장을 자체 트랜잭션으로(자동 커밋이 기본) · "수동 커밋" = 클라이언트가 `BEGIN`을 먼저 보냄(psql `\set AUTOCOMMIT off` · JDBC `setAutoCommit(false)`) · 서버 쪽 autocommit 설정 없음(7.4에서 제거) |
| 시작 | `BEGIN [WORK\|TRANSACTION] [mode]` · `START TRANSACTION [ISOLATION LEVEL …] [READ WRITE\|READ ONLY] [[NOT] DEFERRABLE]` |
| 끝 | `COMMIT` / `END` · `ROLLBACK` / `ABORT` · `COMMIT AND CHAIN`(12+) · 2단계 `PREPARE TRANSACTION 'id'` / `COMMIT PREPARED` |
| 세이브포인트 | `SAVEPOINT s` · `ROLLBACK TO [SAVEPOINT] s` · `RELEASE [SAVEPOINT] s` |
| 트랜잭션 DDL | 대부분 DDL이 롤백 가능(위 T-9 예외 제외) · 그 안에서 `CREATE INDEX CONCURRENTLY`·`VACUUM`·`CREATE DATABASE` = 오류 25001 |
| 격리 수준 | `READ COMMITTED`(기본) · `REPEATABLE READ`(스냅숏 · 직렬화 실패 40001 가능) · `SERIALIZABLE`(SSI) · `READ UNCOMMITTED` = READ COMMITTED로 동작 · `SET TRANSACTION …` / `SET SESSION CHARACTERISTICS AS TRANSACTION …` |
| 실패 상태 | 블록 안 오류 뒤 모든 문장 = **25P02 in_failed_sql_transaction**("current transaction is aborted, commands ignored until end of transaction block") → ROLLBACK(또는 세이브포인트) 필요 · 클라이언트 자동 세이브포인트 = psql `ON_ERROR_ROLLBACK` |
| 설정 | `SET [SESSION\|LOCAL] name {TO\|=} value` · `SHOW name\|ALL` · `RESET name\|ALL` · `current_setting()`/`set_config()` · `SET LOCAL` = 트랜잭션 끝까지 · 사용자 정의 `myapp.var` 가능 |
| search_path | 기본 `"$user", public` · `SET search_path TO s1, s2` · `pg_catalog`·`pg_temp` 암묵 포함 · 현재 스키마 = `current_schema()`(Oracle `CURRENT_SCHEMA` 대응) |
| 그 밖 | `SET ROLE` · `SET SESSION AUTHORIZATION` · `statement_timeout` · `lock_timeout` · `idle_in_transaction_session_timeout` · `LISTEN/NOTIFY` · `DISCARD ALL` · `LOCK TABLE … IN … MODE [NOWAIT]` |

출처: https://www.postgresql.org/docs/16/sql-begin.html · https://www.postgresql.org/docs/16/sql-start-transaction.html · https://www.postgresql.org/docs/16/sql-savepoint.html · https://www.postgresql.org/docs/16/transaction-iso.html · https://www.postgresql.org/docs/16/sql-set.html · https://www.postgresql.org/docs/16/ddl-schemas.html · https://www.postgresql.org/docs/16/errcodes-appendix.html

### T-12. DCL

| 대상 | 권한(약어) | 기본 PUBLIC 권한 |
|---|---|---|
| TABLE·뷰·MV·외부 테이블 | `SELECT`(r) `INSERT`(a) `UPDATE`(w) `DELETE`(d) `TRUNCATE`(D) `REFERENCES`(x) `TRIGGER`(t) | 없음 |
| 테이블 열 | `SELECT` `INSERT` `UPDATE` `REFERENCES` (열 목록) | 없음 |
| SEQUENCE | `USAGE`(U) `SELECT` `UPDATE` | 없음 |
| DATABASE | `CREATE`(C) `CONNECT`(c) `TEMPORARY`(T) | `CONNECT` `TEMPORARY` |
| SCHEMA | `CREATE`(C) `USAGE`(U) | 없음(15+ = `public` 스키마도 CREATE 안 줌) |
| FUNCTION·PROCEDURE | `EXECUTE`(X) | `EXECUTE` |
| DOMAIN·TYPE·LANGUAGE | `USAGE` | `USAGE` |
| FDW·FOREIGN SERVER | `USAGE` | 없음 |
| TABLESPACE | `CREATE` | 없음 |
| LARGE OBJECT | `SELECT` `UPDATE` | 없음 |
| PARAMETER(15+) | `SET`(s) `ALTER SYSTEM`(A) | 없음 |

| 구문 | 내용 |
|---|---|
| GRANT | `GRANT {privs\|ALL [PRIVILEGES]} ON {[TABLE] t\|ALL TABLES IN SCHEMA s\|SEQUENCE\|FUNCTION f(args)\|SCHEMA\|DATABASE\|…} TO {role\|PUBLIC\|CURRENT_ROLE\|CURRENT_USER\|SESSION_USER} [WITH GRANT OPTION] [GRANTED BY role]` |
| REVOKE | `REVOKE [GRANT OPTION FOR] … FROM … [CASCADE\|RESTRICT]` |
| 역할 | `GRANT role TO member [WITH {ADMIN\|INHERIT\|SET} {TRUE\|FALSE}]`(16+ 옵션) · `REVOKE role FROM member` · 사전 정의 역할 `pg_read_all_data` `pg_write_all_data`(14+) `pg_monitor` `pg_read_server_files` `pg_signal_backend` `pg_create_subscription`(16+) 등 |
| 기본 권한 | `ALTER DEFAULT PRIVILEGES [FOR ROLE r] [IN SCHEMA s] GRANT … ON {TABLES\|SEQUENCES\|FUNCTIONS\|ROUTINES\|TYPES\|SCHEMAS} TO …` · `pg_default_acl` |
| 소유권 | 소유자 = 모든 권한 + DROP/ALTER · `ALTER … OWNER TO` · `REASSIGN OWNED BY` · `DROP OWNED BY` |
| RLS | `ALTER TABLE t ENABLE/FORCE ROW LEVEL SECURITY` + `CREATE POLICY`(T-9) · `row_security` 설정 |
| 조회 | `relacl`·`proacl` 등 `aclitem[]`(`user=arwdDxt/grantor`) · `aclexplode()` · `has_table_privilege()` 등 · `information_schema.table_privileges` · psql `\dp` |

출처: https://www.postgresql.org/docs/16/ddl-priv.html · https://www.postgresql.org/docs/16/sql-grant.html · https://www.postgresql.org/docs/16/sql-revoke.html · https://www.postgresql.org/docs/16/sql-alterdefaultprivileges.html · https://www.postgresql.org/docs/16/predefined-roles.html · https://www.postgresql.org/docs/16/ddl-rowsecurity.html

### T-13. 오류

| 항목 | 내용 |
|---|---|
| SQLSTATE | 5글자(클래스 2 + 하위 3) · 표준 기반 · 조건 이름(`unique_violation` 등) = PL/pgSQL `EXCEPTION WHEN` |
| 주요 클래스 | `00` 성공 · `01` 경고 · `02` 데이터 없음 · `08` 접속 예외(08006 연결 실패 · 08P01 프로토콜 위반) · `0A` 미지원 · `22` 데이터 예외(22001 문자열 너무 김 · 22003 숫자 범위 · 22012 0으로 나눔 · 22P02 잘못된 텍스트 표현) · `23` 무결성(23502 not_null · 23503 foreign_key · 23505 unique · 23514 check · 23P01 exclusion) · `25` 트랜잭션 상태(25001 활성 트랜잭션 · 25006 읽기 전용 · **25P02 실패 트랜잭션**) · `28` 인증(28P01 비밀번호) · `3D000` DB 없음 · `3F000` 스키마 없음 · `40` 롤백(40001 직렬화 실패 · 40P01 교착) · `42` 구문·권한(42601 구문 · 42501 권한 · 42P01 테이블 없음 · 42703 열 없음 · 42883 함수 없음 · 42P07 중복 테이블 · 42710 중복 객체 · 42804 형 불일치) · `53` 자원 · `54` 한계 초과 · `55P03` 잠금 불가(NOWAIT) · `57` 운영자 개입(**57014 query_canceled** = 취소·statement_timeout · 57P01 관리자 종료) · `58` 시스템 · `P0` PL/pgSQL(P0001 raise_exception · P0002 no_data_found · P0003 too_many_rows) · `XX` 내부 |
| 심각도 | 오류: `ERROR` · `FATAL`(세션 종료) · `PANIC`(서버 재시작) / 알림: `WARNING` · `NOTICE` · `DEBUG` · `INFO` · `LOG` · 프로토콜 필드 `S`(지역화) · `V`(9.6+ 비지역화) |
| 필드 | `C` 코드 · `M` 메시지 · `D` detail · `H` hint · **`P` position = 원 질의 문자열의 1부터 세는 문자 위치**(바이트 아님) · `p`/`q` 내부 질의 위치·글 · `W` where(문맥 · PL/pgSQL 줄) · `s` schema · `t` table · `c` column · `d` datatype · `n` constraint · `F` `L` `R` 소스 위치 |
| RAISE | `RAISE [level] 'fmt %', args [USING ERRCODE='…', MESSAGE, DETAIL, HINT, COLUMN, CONSTRAINT, DATATYPE, TABLE, SCHEMA]` · level = DEBUG/LOG/INFO/NOTICE/WARNING/EXCEPTION(기본) · `RAISE SQLSTATE '22012'` · `RAISE condition_name` · `RAISE;`(재발생) · `ASSERT cond [, msg]` · NOTICE는 클라이언트에 비동기 메시지로 감(`client_min_messages`) |
| 잡기 | `EXCEPTION WHEN cond [OR cond] THEN …` · `GET STACKED DIAGNOSTICS v = RETURNED_SQLSTATE, MESSAGE_TEXT, PG_EXCEPTION_DETAIL, PG_EXCEPTION_HINT, PG_EXCEPTION_CONTEXT` · `SQLSTATE`·`SQLERRM` 변수 |

출처: https://www.postgresql.org/docs/16/errcodes-appendix.html · https://www.postgresql.org/docs/16/protocol-error-fields.html · https://www.postgresql.org/docs/16/plpgsql-errors-and-messages.html · https://www.postgresql.org/docs/16/plpgsql-control-structures.html#PLPGSQL-ERROR-TRAPPING · https://www.postgresql.org/docs/16/runtime-config-client.html

### T-14. 클라이언트 · 프로토콜

| 항목 | 내용 |
|---|---|
| 프로토콜 | 프론트엔드/백엔드 v3.0(7.4+ · 15·16 그대로 · 3.2는 18+ 추정) · TCP 5432 / 유닉스 소켓 · `SSLRequest` · GSSAPI · 인증 SCRAM-SHA-256(14+ 기본) / md5 / trust 등 |
| 단순 질의 | `Query`('Q') 한 메시지에 `;`로 나뉜 **여러 문장** 가능 → 문장마다 RowDescription/DataRow/CommandComplete · 여러 문장은 BEGIN이 없으면 **하나의 암묵 트랜잭션** · 결과 = 텍스트 형식만 · 매개변수 없음 |
| 확장 질의 | `Parse`(이름 붙은/없는 준비 문장 · 매개변수 형 OID) → `Bind`(포털 · 매개변수 값 · 형식 텍스트/이진 · 결과 형식) → `Describe` → `Execute`(최대 행 수 → `PortalSuspended`로 나눠 받기) → `Sync` · 문장 **하나만** · 매개변수 = `$1..$n` · 형 추론 = 서버(OID 0) |
| 형 식별 | `RowDescription` = 열 이름 · 테이블 OID · 열 번호 · **형 OID** · 형 크기 · typmod · 형식 코드 · 주요 OID: bool 16 · bytea 17 · int8 20 · int2 21 · int4 23 · text 25 · oid 26 · json 114 · xml 142 · float4 700 · float8 701 · money 790 · bpchar 1042 · varchar 1043 · date 1082 · time 1083 · timestamp 1114 · timestamptz 1184 · interval 1186 · timetz 1266 · bit 1560 · varbit 1562 · numeric 1700 · refcursor 1790 · uuid 2950 · jsonb 3802(추정 = `pg_type` 값을 기억으로 적음 · `SELECT oid, typname FROM pg_type`로 확인) |
| 대량 적재 | `COPY … FROM STDIN` → `CopyInResponse` → `CopyData`… → `CopyDone`/`CopyFail` · `COPY … TO STDOUT` → `CopyOutResponse` → `CopyData` · 형식 text/csv/binary |
| 취소 | 새 연결을 열어 `CancelRequest`(백엔드 PID + 비밀 키 · 접속 때 `BackendKeyData`로 받음) · 응답 없음 · 성공하면 원 질의가 57014 · 취소 키는 평문(SSL 없이도 · 추정 = 16에서 SSL 사용 가능 여부 미확인) |
| 비동기 메시지 | `NoticeResponse`(NOTICE·WARNING · RAISE) · `NotificationResponse`(LISTEN/NOTIFY) · `ParameterStatus`(server_version · client_encoding · TimeZone · `DateStyle` · `integer_datetimes` · `standard_conforming_strings` 등 변경 통지) |
| 상태 표시 | `ReadyForQuery`의 트랜잭션 상태 바이트 = `I` 유휴 · `T` 트랜잭션 블록 · `E` **실패 트랜잭션**(25P02 상태) — 클라이언트가 트랜잭션 표시·"ROLLBACK 필요" 판단에 바로 쓸 수 있음 |
| 행 수 | `CommandComplete` 태그 = `INSERT 0 n` · `UPDATE n` · `DELETE n` · `SELECT n` · `MERGE n` · `COPY n` |
| 커서 | 확장 질의 포털(최대 행 수로 페이지) 또는 SQL `DECLARE c [SCROLL] CURSOR [WITH HOLD] FOR …` + `FETCH n FROM c`(트랜잭션 안 · WITH HOLD = 커밋 뒤에도) |
| 파이프라인 | 여러 Parse/Bind/Execute를 Sync 하나로 묶어 왕복 줄임(libpq 14+ pipeline mode) |

출처: https://www.postgresql.org/docs/16/protocol.html · https://www.postgresql.org/docs/16/protocol-flow.html · https://www.postgresql.org/docs/16/protocol-message-formats.html · https://www.postgresql.org/docs/16/sql-copy.html · https://www.postgresql.org/docs/16/libpq-pipeline-mode.html · https://www.postgresql.org/docs/16/sql-declare.html · https://www.postgresql.org/docs/16/catalog-pg-type.html

### T-15 nexa-sql 지원 현황

방언 열 **PG**(PostgreSQL) — 항목별 ✅/부분/❌와 코드 자리는 §11 표의 `PG` 열 · 한 줄 판정은 §11 끝 "요약" · 결함은 §9.

## §5. SQLite 3

> 기준 = SQLite 3.46 계열(2024-05) 문법 · 이후 3.54.0(2026-10-09)까지 추가분은 "버전" 열에 표기. 조사일 2026-10-10 · sqlite.org 공식 문서만 사용. 공식 문서에서 직접 확인하지 못한 항목은 **추정**으로 표시.

### T-1. 버전 · 문서

| 항목 | 내용 |
|---|---|
| 기준 판 | 3.46.0(2024-05-23) · 현재 최신 3.54.0(2026-10-09) · 3.53.x 버그 수정판 계열 |
| 단일 파일 DB | 서버 없음 · 프로세스 안 라이브러리 · 파일 하나 = DB 하나(+ `-wal`/`-shm`/`-journal`) |
| 문법 총람 | https://www.sqlite.org/lang.html |
| 키워드 | https://www.sqlite.org/lang_keywords.html |
| 타입·친화성 | https://www.sqlite.org/datatype3.html · STRICT = https://www.sqlite.org/stricttables.html |
| 식·연산자 | https://www.sqlite.org/lang_expr.html |
| 함수 | 핵심 lang_corefunc.html · 집계 lang_aggfunc.html · 창 windowfunctions.html · 날짜 lang_datefunc.html · 수학 lang_mathfunc.html · JSON json1.html |
| 결과 코드 | https://www.sqlite.org/rescode.html |
| PRAGMA | https://www.sqlite.org/pragma.html |
| 스키마 표 | https://www.sqlite.org/schematab.html |
| 변경 이력 | https://www.sqlite.org/changes.html |

출처: https://www.sqlite.org/changes.html · https://www.sqlite.org/lang.html

### T-2. 어휘 규칙

| 항목 | 규칙 |
|---|---|
| 식별자 길이 | 실질 제한 없음(SQL 문 전체 길이 상한 `SQLITE_MAX_SQL_LENGTH` 기본 1,000,000,000 바이트 안) — 추정(식별자 전용 상한 없음) |
| 식별자 인용 | `"이름"`(표준) · `[이름]`(SQL Server/Access 호환) · `` `이름` ``(MySQL 호환) — 셋 다 식별자 |
| 대소문자 | 키워드·식별자 = ASCII 범위만 대소문자 무시(인용해도 비교는 무시 · 원문 철자는 보존) · 비ASCII 글자는 구분 — 추정(문서 근거 = `NOCASE`가 ASCII만 접는 규칙과 동일) |
| 문자열 | `'...'` · 안의 `'` = `''` 두 번 · 백슬래시 이스케이프 없음 · `unistr('\u....')`(3.50)로 유니코드 이스케이프 |
| 큰따옴표 문자열 오용 | `"x"`가 식별자로 풀리지 않으면 문자열로 받아 줌(역사적 오용 · DQS) · 컴파일 옵션 `SQLITE_DQS=0` / `sqlite3_db_config(SQLITE_DBCONFIG_DQS_DML/DDL)`로 끔 |
| 작은따옴표 식별자 | 식별자만 올 수 있는 자리의 `'key'`는 식별자로 받아 줌(역시 호환용 · 쓰지 말 것) |
| BLOB 리터럴 | `X'0A1B'` / `x'..'`(16진 짝수 자리) |
| 숫자 | 정수 · 실수(`1.5e10`) · 16진 `0x1F`(정수만 · 64비트 2의 보수) · 3.46부터 숫자 안 `_` 구분자 허용 — 추정(3.46 변경 이력 기억) |
| 불리언 | 별도 타입 없음 · `TRUE`=1 · `FALSE`=0(3.23+ 키워드 · 그 이름의 컬럼이 있으면 컬럼 우선) · `IS TRUE`/`IS FALSE` |
| NULL | `NULL` 리터럴 |
| 주석 | `-- 줄 끝까지` · `/* ... */`(중첩 불가 · 닫지 않으면 입력 끝까지) |
| 문장 종결 | `;` · `sqlite3_prepare`는 한 문장씩 · 나머지는 `pzTail`로 반환 |
| 매개변수 | `?` · `?NNN`(1~`SQLITE_MAX_VARIABLE_NUMBER` 기본 32766) · `:AAA` · `@AAA` · `$AAA`(Tcl식 `::`·`(...)` 접미 허용) · 이름 매개변수도 번호를 가짐 |

출처: https://www.sqlite.org/lang_keywords.html · https://www.sqlite.org/lang_expr.html · https://www.sqlite.org/quirks.html · https://www.sqlite.org/limits.html

### T-3. 키워드 (147개)

`ABORT ACTION ADD AFTER ALL ALTER ALWAYS ANALYZE AND AS ASC ATTACH AUTOINCREMENT BEFORE BEGIN BETWEEN BY CASCADE CASE CAST CHECK COLLATE COLUMN COMMIT CONFLICT CONSTRAINT CREATE CROSS CURRENT CURRENT_DATE CURRENT_TIME CURRENT_TIMESTAMP DATABASE DEFAULT DEFERRABLE DEFERRED DELETE DESC DETACH DISTINCT DO DROP EACH ELSE END ESCAPE EXCEPT EXCLUDE EXCLUSIVE EXISTS EXPLAIN FAIL FILTER FIRST FOLLOWING FOR FOREIGN FROM FULL GENERATED GLOB GROUP GROUPS HAVING IF IGNORE IMMEDIATE IN INDEX INDEXED INITIALLY INNER INSERT INSTEAD INTERSECT INTO IS ISNULL JOIN KEY LAST LEFT LIKE LIMIT MATCH MATERIALIZED NATURAL NO NOT NOTHING NOTNULL NULL NULLS OF OFFSET ON OR ORDER OTHERS OUTER OVER PARTITION PLAN PRAGMA PRECEDING PRIMARY QUERY RAISE RANGE RECURSIVE REFERENCES REGEXP REINDEX RELEASE RENAME REPLACE RESTRICT RETURNING RIGHT ROLLBACK ROW ROWS SAVEPOINT SELECT SET TABLE TEMP TEMPORARY THEN TIES TO TRANSACTION TRIGGER UNBOUNDED UNION UNIQUE UPDATE USING VACUUM VALUES VIEW VIRTUAL WHEN WHERE WINDOW WITH WITHOUT`

| 구분 | 내용 |
|---|---|
| 개수 | 147(문서 명시 · `sqlite3_keyword_count()`로 런타임 확인 가능) · 컴파일 옵션(`SQLITE_OMIT_*`)에 따라 줄어듦 |
| 없는 것 | `TRUE`/`FALSE`는 목록에 없음(특수 식별자 취급) · `MERGE`·`TRUNCATE`·`GRANT`·`PROCEDURE`·`FUNCTION`·`SEQUENCE`·`LATERAL` 없음 |
| 인용 없이 식별자로 | 파서의 "fallback" 규칙으로 상당수 키워드(예 `ABORT ACTION AFTER ASC BEFORE BEGIN CASCADE CONFLICT DATABASE DEFERRED DESC DO EACH END FAIL FIRST FOLLOWING IGNORE KEY LAST NO OF PLAN PRAGMA QUERY RAISE RECURSIVE RELEASE RENAME REPLACE RESTRICT ROW ROWS SAVEPOINT TEMP TRIGGER VACUUM VIEW VIRTUAL WITHOUT` 등)가 문맥상 식별자로 쓰일 수 있음 · 정확한 목록은 문서에 없고 `parse.y` `%fallback ID` 줄이 원천 — **추정**(목록 범위) |
| 공식 권고 | 키워드를 이름으로 쓰려면 반드시 인용 · fallback 동작은 버전마다 바뀔 수 있음 |
| 함수형 | `sqlite3_keyword_name(i)` / `sqlite3_keyword_check(z,n)`으로 클라이언트가 목록을 런타임 획득 가능 |

출처: https://www.sqlite.org/lang_keywords.html · https://www.sqlite.org/c3ref/keyword_check.html

### T-4. 타입 · 친화성

**저장 클래스(값의 타입)**: `NULL` · `INTEGER`(1~8바이트) · `REAL`(IEEE 8바이트) · `TEXT`(DB 인코딩 UTF-8/16) · `BLOB`. 타입은 값에 붙고 컬럼은 "친화성"만 가진다(동적 타입).

| 친화성 결정 규칙(선언 타입 이름 기준 · 순서대로 첫 일치) | 친화성 | 예 |
|---|---|---|
| 1. 이름에 `INT` 포함 | INTEGER | INT, BIGINT, UNSIGNED BIG INT, INT8 |
| 2. `CHAR`·`CLOB`·`TEXT` 포함 | TEXT | VARCHAR(255), NCHAR(55), CLOB |
| 3. `BLOB` 포함 또는 선언 타입 없음 | BLOB(옛 이름 NONE) | BLOB, (없음) |
| 4. `REAL`·`FLOA`·`DOUB` 포함 | REAL | DOUBLE, FLOAT |
| 5. 그 밖 | NUMERIC | NUMERIC, DECIMAL(10,5), BOOLEAN, DATE, DATETIME |

| 항목 | 내용 |
|---|---|
| 길이 제약 | `VARCHAR(10)`의 길이는 무시(검사 없음) · 값 상한 = `SQLITE_MAX_LENGTH` 기본 1e9 바이트 |
| 날짜 타입 | 없음 — TEXT(ISO-8601) · REAL(율리우스일) · INTEGER(유닉스 초)로 저장 · 날짜 함수가 해석 |
| 불리언 | 없음 — 정수 0/1 |
| DECIMAL | 없음(NUMERIC 친화성 · 정밀 십진은 `decimal` 확장) |
| STRICT 테이블(3.37) | `CREATE TABLE ... STRICT` · 허용 타입 = `INT` `INTEGER` `REAL` `TEXT` `BLOB` `ANY` · 맞지 않는 값 = `SQLITE_CONSTRAINT_DATATYPE` · ANY = 값 그대로 보존 |
| INTEGER PRIMARY KEY | 정확히 `INTEGER PRIMARY KEY`(INT 아님 · DESC 아님)면 rowid의 별칭 · 64비트 부호 정수 |
| rowid | 모든 일반 테이블에 숨은 `rowid`/`oid`/`_rowid_` · `WITHOUT ROWID` 테이블엔 없음 |
| 비교 친화성 | 비교 전 피연산자에 친화성 적용(컬럼 쪽 친화성 우선) · 정렬 순서 NULL < 숫자 < TEXT < BLOB |
| `typeof(X)` | 값의 저장 클래스 이름 반환 |

출처: https://www.sqlite.org/datatype3.html · https://www.sqlite.org/stricttables.html · https://www.sqlite.org/lang_createtable.html#rowid

### T-5. 연산자 · 우선순위 · NULL · 콜레이션

| 우선순위(높음→낮음) | 연산자 |
|---|---|
| 1 | 단항 `~` `+` `-` |
| 2 | `COLLATE 이름`(후치) |
| 3 | `||`(문자열 연결) · `->` · `->>`(JSON · 3.38) |
| 4 | `*` `/` `%` |
| 5 | `+` `-` |
| 6 | `&` `|` `<<` `>>` |
| 7 | `ESCAPE`(LIKE 후치) |
| 8 | `<` `>` `<=` `>=` |
| 9 | `=` `==` `<>` `!=` `IS` `IS NOT` `IS DISTINCT FROM` `IS NOT DISTINCT FROM`(3.39) |
| 10 | `BETWEEN … AND` |
| 11 | `IN` `MATCH` `LIKE` `REGEXP` `GLOB`(모두 `NOT` 접두 가능) |
| 12 | `ISNULL` `NOTNULL` `NOT NULL`(후치) |
| 13 | `NOT` |
| 14 | `AND` |
| 15 | `OR` |

| 항목 | 내용 |
|---|---|
| 연결 | `||` (NULL이 끼면 NULL) · `concat()`은 NULL 무시(3.44) |
| 비트 XOR | 연산자 없음(`(a|b)-(a&b)`) |
| LIKE | 기본 ASCII 대소문자 무시 · `PRAGMA case_sensitive_like`(폐기 예정) · `REGEXP`·`MATCH`는 사용자 함수가 있어야 동작 |
| GLOB | 대소문자 구분 · `*` `?` `[...]` |
| NULL | 3값 논리 · `=`/`<>`는 NULL이면 NULL · `IS`/`IS NOT` = NULL 안전 비교(= `IS [NOT] DISTINCT FROM`) |
| NULL 정렬 | NULL이 가장 작은 값 → `ASC` 기본 = NULLS FIRST · `DESC` = NULLS LAST · `NULLS FIRST/LAST` 명시 가능(3.30) |
| 정수 나눗셈 | 정수/정수 = 정수(절삭) · 0으로 나누기 = NULL(오류 아님) |
| 콜레이션 | `BINARY`(기본 · memcmp) · `NOCASE`(ASCII 26자만 접음) · `RTRIM`(뒤 공백 무시) · 그 밖 = `sqlite3_create_collation`(예 ICU 확장) |
| 콜레이션 지정 | 컬럼 정의 `COLLATE` · 식 `x COLLATE NOCASE` · `ORDER BY ... COLLATE` · 인덱스 컬럼 |

출처: https://www.sqlite.org/lang_expr.html · https://www.sqlite.org/datatype3.html#collation · https://www.sqlite.org/lang_select.html#orderby · https://www.sqlite.org/changes.html

### T-6. 함수

**핵심 스칼라(60 서명)**

| 분류 | 서명 |
|---|---|
| 수치 | `abs(X)` · `random()` · `round(X)` · `round(X,Y)` · `sign(X)` · `max(X,Y,...)` · `min(X,Y,...)` |
| 문자열 | `char(X1,...,XN)` · `concat(X,...)`(3.44) · `concat_ws(SEP,X,...)`(3.44) · `format(FORMAT,...)` · `printf(FORMAT,...)` · `instr(X,Y)` · `length(X)` · `octet_length(X)`(3.43) · `lower(X)` · `upper(X)` · `ltrim(X)` · `ltrim(X,Y)` · `rtrim(X)` · `rtrim(X,Y)` · `trim(X)` · `trim(X,Y)` · `replace(X,Y,Z)` · `substr(X,Y)` · `substr(X,Y,Z)` · `substring(X,Y)` · `substring(X,Y,Z)` · `soundex(X)`(컴파일 옵션) · `unicode(X)` · `unistr(X)`(3.50) · `unistr_quote(X)` · `quote(X)` |
| 패턴 | `glob(X,Y)` · `like(X,Y)` · `like(X,Y,Z)` |
| BLOB | `hex(X)` · `unhex(X)` · `unhex(X,Y)` · `randomblob(N)` · `zeroblob(N)` |
| NULL·조건 | `coalesce(X,Y,...)` · `ifnull(X,Y)` · `nullif(X,Y)` · `iif(B1,V1,...)` · `if(B1,V1,...)`(3.48 · 다중 분기형) |
| 타입 | `typeof(X)` |
| 플래너 힌트 | `likelihood(X,Y)` · `likely(X)` · `unlikely(X)` |
| 연결 상태 | `changes()` · `total_changes()` · `last_insert_rowid()` |
| 시스템 | `sqlite_version()` · `sqlite_source_id()` · `sqlite_compileoption_get(N)` · `sqlite_compileoption_used(X)` · `sqlite_offset(X)`(옵션) · `load_extension(X)` · `load_extension(X,Y)`(기본 비활성) |

**집계**

| 서명 | 비고 |
|---|---|
| `avg(X)` · `count(*)` · `count(X)` · `max(X)` · `min(X)` · `sum(X)` · `total(X)` | `sum` 정수 넘침 = 오류 · `total` = 늘 REAL |
| `group_concat(X)` · `group_concat(X,Y)` · `string_agg(X,Y)`(3.44) | 호출 안 `ORDER BY`(3.44) |
| `median(X)` · `percentile(Y,P)` · `percentile_cont(Y,P)` · `percentile_disc(Y,P)` | 3.51+ · `SQLITE_ENABLE_PERCENTILE` 필요 |
| `json_group_array(V)` · `jsonb_group_array(V)` · `json_group_object(L,V)` · `jsonb_group_object(L,V)` | JSON |
| 공통 수식 | `DISTINCT`(1인자) · `FILTER (WHERE …)` · 모든 집계는 `OVER`로 창 함수 겸용 |

**창 함수(3.25+ · EXCLUDE·GROUPS 3.28)**: `row_number()` · `rank()` · `dense_rank()` · `percent_rank()` · `cume_dist()` · `ntile(N)` · `lag(expr[,offset[,default]])` · `lead(expr[,offset[,default]])` · `first_value(expr)` · `last_value(expr)` · `nth_value(expr,N)` — 내장 창 함수엔 `FILTER` 불가.

**날짜·시간**

| 서명 | 비고 |
|---|---|
| `date(t, mod...)` · `time(t, mod...)` · `datetime(t, mod...)` · `julianday(t, mod...)` · `unixepoch(t, mod...)`(3.38) · `strftime(fmt, t, mod...)` · `timediff(t1, t2)`(3.43) | t 생략 = `'now'` |
| 수식어 | `NNN days/hours/minutes/seconds/months/years` · `±HH:MM[:SS[.SSS]]` · `±YYYY-MM-DD[ HH:MM[:SS[.SSS]]]` · `ceiling` · `floor` · `start of month/year/day` · `end of month/year/day`(3.54) · `weekday N`(음수 3.54) · `unixepoch` · `julianday` · `auto` · `localtime` · `utc` · `subsec`/`subsecond` |
| 키워드 상수 | `CURRENT_DATE` · `CURRENT_TIME` · `CURRENT_TIMESTAMP`(UTC 텍스트) |

**수학(3.35 · `SQLITE_ENABLE_MATH_FUNCTIONS` 컴파일 시만)**: `acos` `acosh` `asin` `asinh` `atan` `atan2(Y,X)` `atanh` `ceil` `ceiling` `cos` `cosh` `degrees` `exp` `floor` `ln` `log(X)` `log(B,X)` `log10` `log2` `mod(X,Y)` `pi()` `pow(X,Y)` `power(X,Y)` `radians` `sin` `sinh` `sqrt` `tan` `tanh` `trunc` (인자 표기 없는 것은 `(X)`).

**JSON(3.38부터 기본 내장 · JSONB 3.45)**

| 분류 | 서명 |
|---|---|
| 생성·변환 | `json(j)` · `jsonb(j)` · `json_array(v,...)` · `jsonb_array(v,...)` · `json_object(l,v,...)` · `jsonb_object(l,v,...)` · `json_quote(v)` · `json_pretty(j)`(3.46) |
| 조회 | `json_extract(j,path,...)` · `jsonb_extract(j,path,...)` · `j -> path` · `j ->> path` · `json_array_length(j[,path])` · `json_type(j[,path])` · `json_valid(j[,flags])` · `json_error_position(j)`(3.42) |
| 수정 | `json_insert` · `json_replace` · `json_set` · `json_remove` · `json_patch(j1,j2)` · `json_array_insert(j,path,v,...)` + 각 `jsonb_` 짝 |
| 집계 | `json_group_array` · `json_group_object` + `jsonb_` 짝 |
| 테이블 값 | `json_each(j[,path])` · `json_tree(j[,path])` · `jsonb_each`/`jsonb_tree`(3.51) |

출처: https://www.sqlite.org/lang_corefunc.html · https://www.sqlite.org/lang_aggfunc.html · https://www.sqlite.org/windowfunctions.html · https://www.sqlite.org/lang_datefunc.html · https://www.sqlite.org/lang_mathfunc.html · https://www.sqlite.org/json1.html · https://www.sqlite.org/changes.html

### T-7. 질의

| 항목 | 지원 |
|---|---|
| 절 순서 | `WITH [RECURSIVE]` → `SELECT [DISTINCT|ALL]` → `FROM` → `WHERE` → `GROUP BY` → `HAVING` → `WINDOW` → (복합 연산) → `ORDER BY` → `LIMIT` |
| 행 제한 | `LIMIT n [OFFSET m]` 또는 `LIMIT m, n`(MySQL식 · 순서 주의) · `FETCH FIRST`·`TOP` 없음 |
| CTE | `WITH` · `WITH RECURSIVE`(재귀 = `UNION [ALL]` 복합) · `AS [NOT] MATERIALIZED` 힌트(3.35) |
| 복합 | `UNION` · `UNION ALL` · `INTERSECT` · `EXCEPT`(`MINUS` 없음 · `INTERSECT ALL`/`EXCEPT ALL` 없음) |
| 조인 | `[INNER] JOIN` · `LEFT [OUTER]` · `RIGHT`·`FULL [OUTER]`(3.39) · `CROSS JOIN`(플래너 순서 고정 의미) · `NATURAL` · `ON`/`USING` · 쉼표 조인 |
| LATERAL / APPLY | 없음(테이블 값 함수의 상관 인자로 일부 대체) |
| 창 | `OVER (PARTITION BY … ORDER BY … frame)` · `WINDOW w AS (…)` 명명 창 · 프레임 `ROWS|RANGE|GROUPS` + `EXCLUDE` |
| VALUES | 독립 문장 `VALUES (…),(…)` · FROM 안 서브쿼리 · 컬럼 이름 `column1…` |
| 인덱스 지정 | `FROM t INDEXED BY idx` · `NOT INDEXED`(힌트가 아니라 강제 · 못 쓰면 오류) |
| 잠금 | `FOR UPDATE`·`NOWAIT` 없음(DB 단위 잠금) |
| 서브쿼리 | 스칼라·`IN`·`EXISTS`·FROM 서브쿼리 · 행 값(row value `(a,b) = (1,2)` · 3.15) |
| 기타 | 집계 없는 bare 컬럼 허용(min/max와 함께면 그 행 값) · `EXPLAIN` / `EXPLAIN QUERY PLAN` |

출처: https://www.sqlite.org/lang_select.html · https://www.sqlite.org/lang_with.html · https://www.sqlite.org/windowfunctions.html · https://www.sqlite.org/lang_indexedby.html · https://www.sqlite.org/rowvalue.html

### T-8. DML

| 문장 | 내용 |
|---|---|
| INSERT | `INSERT [OR ABORT|FAIL|IGNORE|REPLACE|ROLLBACK] INTO t [(cols)] VALUES … | SELECT … | DEFAULT VALUES` |
| REPLACE | `REPLACE INTO …` = `INSERT OR REPLACE`(충돌 행 삭제 후 삽입 · 삭제 트리거는 `recursive_triggers`일 때만) |
| 충돌 절 | `ON CONFLICT` 알고리즘 5종 = ROLLBACK · ABORT(기본) · FAIL · IGNORE · REPLACE — 문장 또는 제약 정의에 |
| UPSERT | `INSERT … ON CONFLICT [(target) [WHERE]] DO NOTHING | DO UPDATE SET … [WHERE]`(3.24 · 다중 ON CONFLICT 절 3.35) · `excluded.컬럼` |
| RETURNING | INSERT/UPDATE/DELETE `RETURNING expr|*`(3.35 · 트리거 효과 전 값 · 가상 테이블 불가) |
| UPDATE | `UPDATE [OR …] t SET … [FROM …](3.33) [WHERE] [RETURNING]` · `ORDER BY/LIMIT`은 컴파일 옵션 `SQLITE_ENABLE_UPDATE_DELETE_LIMIT` |
| DELETE | `DELETE FROM t [WHERE] [RETURNING]` · WHERE 없으면 "truncate 최적화"(행별 삭제 생략 · 트리거 있으면 비활성) |
| MERGE / TRUNCATE | 없음 |

출처: https://www.sqlite.org/lang_insert.html · https://www.sqlite.org/lang_upsert.html · https://www.sqlite.org/lang_returning.html · https://www.sqlite.org/lang_update.html · https://www.sqlite.org/lang_delete.html · https://www.sqlite.org/lang_conflict.html · https://www.sqlite.org/lang_replace.html

### T-9. 객체 종류

| 객체 | CREATE / DROP / ALTER | IF [NOT] EXISTS | 이름 꼴 | 카탈로그 | 소스 |
|---|---|---|---|---|---|
| TABLE | `CREATE [TEMP] TABLE` · 옵션 `WITHOUT ROWID` · `STRICT`(3.37) · 생성 컬럼 `GENERATED ALWAYS AS (…) [VIRTUAL|STORED]`(3.31) · `AS SELECT` · ALTER = `RENAME TO` · `RENAME COLUMN`(3.25) · `ADD COLUMN` · `DROP COLUMN`(3.35)만 — 그 밖 변경 = 새 테이블 복사 절차 | 둘 다 | `스키마.테이블` | `sqlite_schema`(type='table') · `PRAGMA table_info`/`table_xinfo`(숨은·생성 컬럼) · `table_list`(3.37) · `foreign_key_list` | `sqlite_schema.sql` |
| VIEW | `CREATE [TEMP] VIEW v [(cols)] AS SELECT` · 읽기 전용(INSTEAD OF 트리거로 쓰기) · ALTER 없음 | 둘 다 | 같음 | type='view' · `table_info` | `sql` 열 |
| INDEX | `CREATE [UNIQUE] INDEX … ON t (col|expr [COLLATE][ASC|DESC]) [WHERE …]`(부분 3.8 · 식 3.9) · ALTER 없음 · `REINDEX` | 둘 다 | `스키마.인덱스`(테이블과 같은 스키마) | type='index' · `index_list` · `index_info` · `index_xinfo` | `sql`(자동 인덱스 `sqlite_autoindex_*` = NULL) |
| TRIGGER | `CREATE [TEMP] TRIGGER … BEFORE|AFTER|INSTEAD OF INSERT|DELETE|UPDATE [OF cols] ON t [FOR EACH ROW] [WHEN] BEGIN … END` · 행 단위만(문장 단위 없음) · `RAISE()` | 둘 다 | 같음 | type='trigger' | `sql` |
| VIRTUAL TABLE | `CREATE VIRTUAL TABLE t USING 모듈(args)` — fts5 · fts3/4 · rtree · geopoly · dbstat · csv(확장) 등 · 그림자 테이블 동반 | NOT EXISTS | 같음 | type='table'(sql이 `CREATE VIRTUAL`) | `sql` |
| 시퀀스 | 없음 — `INTEGER PRIMARY KEY AUTOINCREMENT` + 내부 `sqlite_sequence(name, seq)` | – | – | `sqlite_sequence` | – |
| 프로시저·함수 | SQL로 만들 수 없음 — 앱 정의 함수(`sqlite3_create_function`) | – | – | `PRAGMA function_list`(옵션) | – |
| 스키마(=DB) | `ATTACH 'file' AS 이름` / `DETACH` · 고정 `main` · `temp` | – | `스키마.객체`(카탈로그 단계 없음) | `PRAGMA database_list` | – |
| 오버로드 | 해당 없음(앱 함수는 인자 수별 등록 가능) | | | | |

| 카탈로그 | 내용 |
|---|---|
| `sqlite_schema`(= 옛 이름 `sqlite_master` · 3.33부터 새 이름) | 열 `type, name, tbl_name, rootpage, sql` · temp = `sqlite_temp_schema` · 첨부 = `스키마.sqlite_schema` |
| PRAGMA 테이블 값 함수 | `SELECT * FROM pragma_table_info('t')` 꼴로 조인 가능 |

출처: https://www.sqlite.org/lang_createtable.html · https://www.sqlite.org/lang_altertable.html · https://www.sqlite.org/lang_createview.html · https://www.sqlite.org/lang_createindex.html · https://www.sqlite.org/lang_createtrigger.html · https://www.sqlite.org/lang_createvtab.html · https://www.sqlite.org/autoinc.html · https://www.sqlite.org/lang_attach.html · https://www.sqlite.org/schematab.html · https://www.sqlite.org/pragma.html · https://www.sqlite.org/gencol.html · https://www.sqlite.org/withoutrowid.html

### T-10. 루틴

| 항목 | 내용 |
|---|---|
| 저장 프로시저·SQL 함수 | 없음 |
| 절차적 블록 | 없음(변수·IF·LOOP·예외 처리 없음) — 트리거 본문 `BEGIN … END`만 여러 DML 나열 |
| 앱 정의 함수 | C API `sqlite3_create_function_v2` / `create_window_function` · rusqlite `Connection::create_scalar_function` 등 — 연결마다 등록(DB 파일에 저장 안 됨) |
| 확장 | `load_extension`(기본 꺼짐) · 정적 링크 확장 |
| 클라이언트 영향 | `BEGIN`으로 시작하는 문장 = 트랜잭션(블록 아님) · 문장 분할 = `;` 단순(트리거 본문 안 `;`만 예외 → `sqlite3_complete()`) |

출처: https://www.sqlite.org/appfunc.html · https://www.sqlite.org/c3ref/create_function.html · https://www.sqlite.org/c3ref/complete.html

### T-11. 트랜잭션

| 항목 | 내용 |
|---|---|
| 기본 | 자동 커밋(문장마다 암시 트랜잭션) |
| 시작 | `BEGIN [DEFERRED|IMMEDIATE|EXCLUSIVE] [TRANSACTION]` · 기본 DEFERRED(첫 읽기/쓰기 때 잠금) |
| 끝 | `COMMIT` = `END [TRANSACTION]` · `ROLLBACK [TRANSACTION]` |
| 세이브포인트 | `SAVEPOINT 이름` · `RELEASE [SAVEPOINT] 이름` · `ROLLBACK TO [SAVEPOINT] 이름` · 바깥 트랜잭션 없이 SAVEPOINT = BEGIN DEFERRED 역할 |
| DDL | 트랜잭션 안에서 롤백 가능(transactional DDL) |
| 중첩 BEGIN | 오류("cannot start a transaction within a transaction") |
| 동시성 | 쓰기 = DB 단위 단일 작성자 · WAL 모드 = 읽기·쓰기 동시 · `SQLITE_BUSY` → `busy_timeout` |
| 격리 | 사실상 SERIALIZABLE · `PRAGMA read_uncommitted`(공유 캐시 한정) |
| 상태 확인 | `sqlite3_get_autocommit()` · `sqlite3_txn_state()` |
| 주요 PRAGMA | `foreign_keys`(**기본 OFF · 연결마다 켜야 함 · 트랜잭션 안에선 바뀌지 않음**) · `journal_mode`(DELETE 기본 · WAL · MEMORY · OFF …) · `synchronous` · `busy_timeout` · `defer_foreign_keys` · `recursive_triggers` · `locking_mode` · `user_version` · `integrity_check` · `optimize` |

출처: https://www.sqlite.org/lang_transaction.html · https://www.sqlite.org/lang_savepoint.html · https://www.sqlite.org/pragma.html · https://www.sqlite.org/foreignkeys.html · https://www.sqlite.org/wal.html · https://www.sqlite.org/isolation.html

### T-12. DCL

| 항목 | 내용 |
|---|---|
| GRANT/REVOKE/사용자/역할 | 없음 — 권한 = 파일 시스템 권한 |
| 대안 | 읽기 전용 열기(`SQLITE_OPEN_READONLY` · `?mode=ro`) · 권한 콜백 `sqlite3_set_authorizer` · `PRAGMA query_only` · 암호화는 별도 제품(SEE · SQLCipher) |

출처: https://www.sqlite.org/c3ref/set_authorizer.html · https://www.sqlite.org/pragma.html#pragma_query_only · https://www.sqlite.org/omitted.html

### T-13. 오류

| 항목 | 내용 |
|---|---|
| 주 결과 코드(하위 8비트) | `SQLITE_OK 0` · `ERROR 1` · `INTERNAL 2` · `PERM 3` · `ABORT 4` · `BUSY 5` · `LOCKED 6` · `NOMEM 7` · `READONLY 8` · `INTERRUPT 9` · `IOERR 10` · `CORRUPT 11` · `NOTFOUND 12` · `FULL 13` · `CANTOPEN 14` · `PROTOCOL 15` · `EMPTY 16` · `SCHEMA 17` · `TOOBIG 18` · `CONSTRAINT 19` · `MISMATCH 20` · `MISUSE 21` · `NOLFS 22` · `AUTH 23` · `FORMAT 24` · `RANGE 25` · `NOTADB 26` · `NOTICE 27` · `WARNING 28` · `ROW 100` · `DONE 101` |
| 확장 코드 | `주코드 | (n<<8)` · `sqlite3_extended_result_codes(db,1)` 또는 `sqlite3_extended_errcode()` · 예 `SQLITE_CONSTRAINT_UNIQUE`(2067) · `_PRIMARYKEY`(1555) · `_FOREIGNKEY`(787) · `_NOTNULL`(1299) · `_CHECK`(275) · `_DATATYPE`(3091) · `SQLITE_BUSY_SNAPSHOT` · `SQLITE_IOERR_*` 다수 |
| 메시지 | `sqlite3_errmsg()` 영어 한 줄(예 `near "FROM": syntax error` · `UNIQUE constraint failed: t.a`) · 위치 번호·SQLSTATE 없음 |
| 위치 | `sqlite3_error_offset()`(3.38) = 오류 토큰의 바이트 오프셋(없으면 -1) · rusqlite 지원 여부 = **추정**(최근 판에서 `SqlInputError{offset}` 제공으로 기억) |
| 경고 로그 | `sqlite3_config(SQLITE_CONFIG_LOG)` |

출처: https://www.sqlite.org/rescode.html · https://www.sqlite.org/c3ref/errcode.html · https://www.sqlite.org/c3ref/error_offset.html

### T-14. 클라이언트

| 항목 | 내용 |
|---|---|
| API | C API(`sqlite3_open_v2` · `prepare_v2/v3` · `bind_*` · `step` · `column_*` · `finalize`) · Rust = `rusqlite`(번들 빌드 `bundled` 기능) — nexa-sql 드라이버 채택 여부는 코드 확인 필요(**추정** = rusqlite) |
| 바인드 | 위치·이름 매개변수 · `bind_int64/double/text/blob/null/zeroblob` · `sqlite3_bind_parameter_name/index/count` |
| 결과 타입 | 행마다 값별 저장 클래스(`sqlite3_column_type`) — 열 단위 타입 고정 아님 · 선언 타입 `sqlite3_column_decltype` |
| BLOB 증분 I/O | `sqlite3_blob_open/read/write/close`(rowid 지정 · 크기 변경 불가 → `zeroblob(N)`로 미리 확보) |
| 취소 | `sqlite3_interrupt(db)`(다른 스레드에서 호출 가능 · `SQLITE_INTERRUPT`) · 진행 콜백 `sqlite3_progress_handler` |
| 대량 적재 | 전용 API 없음 — 트랜잭션 하나 + 준비된 INSERT 재사용(`reset`/`clear_bindings`)이 표준 기법 · CLI `.import` · 백업 = `sqlite3_backup_*` / `VACUUM INTO` |
| 다중 결과·출력 매개변수 | 없음(한 문장 = 한 결과) |
| 서버 메시지 | 없음(`PRINT`류 없음) |
| 스레드 | 연결 단위 직렬화 모드(`SQLITE_OPEN_FULLMUTEX`/`NOMUTEX`) |

출처: https://www.sqlite.org/cintro.html · https://www.sqlite.org/c3ref/blob_open.html · https://www.sqlite.org/c3ref/interrupt.html · https://www.sqlite.org/c3ref/progress_handler.html · https://www.sqlite.org/faq.html#q19 · https://www.sqlite.org/backup.html · https://docs.rs/rusqlite

### T-15 nexa-sql 지원 현황

방언 열 **LT**(SQLite) — 항목별 ✅/부분/❌와 코드 자리는 §11 표의 `LT` 열 · 한 줄 판정은 §11 끝 "요약" · 결함은 §9.

## §6. MySQL 8

> 범위 = MySQL 8.0(GA 2018 · 2026-04 지원 종료) · 8.4 LTS(2024-04 GA). 표 아래 "출처:"는 공식 문서. 확인하지 못한 항목은 **추정**.
> 기준 문서 = `https://dev.mysql.com/doc/refman/8.0/en/` (8.4 = 경로 `8.4`).

### T-1. 버전 · 문서

| 항목 | 내용 |
|---|---|
| 8.0 | 2018 GA · Innovation 이전 계열 · 8.0.x 패치가 기능도 추가(8.0.13 함수 인덱스 · 8.0.14 LATERAL · 8.0.16 CHECK · 8.0.31 INTERSECT/EXCEPT) · 지원 종료 2026-04(**추정**) |
| 8.4 LTS | 2024-04 · LTS 계열(패치만) · 8.0 대비 `MASTER_*` 용어 제거(→ `SOURCE_*`) · 예약어 `QUALIFY`·`TABLESAMPLE` 추가 |
| 9.x | Innovation 계열(분기 출시) — 본 문서 범위 밖 |
| 시스템 카탈로그 | `information_schema`(표준) · `mysql`(내부) · `performance_schema` · `sys` |
| 버전 조회 | `SELECT VERSION()` · `@@version` · `@@version_comment` |

출처: https://dev.mysql.com/doc/refman/8.0/en/ · https://dev.mysql.com/doc/refman/8.4/en/ · https://dev.mysql.com/doc/refman/8.4/en/keywords.html

### T-2. 어휘(렉시컬)

| 항목 | 규칙 | 비고(파서·하이라이트에 영향) |
|---|---|---|
| 식별자 길이 | 64자(DB·테이블·컬럼·인덱스·루틴·트리거·뷰) · 별칭 256 · 사용자 이름 32 | |
| 식별자 인용 | `` `이름` `` (백틱) · 안의 백틱 = ``` `` ``` | 항상 유효 |
| ANSI_QUOTES | `sql_mode`에 있으면 `"이름"` = 식별자(문자열 아님) | 세션별 → 서버에서 `@@sql_mode` 읽어야 판정 |
| 대소문자 | 테이블·DB 이름 = 파일 시스템 의존 · `lower_case_table_names` 0(Linux 구분)/1(Windows 소문자 저장)/2(macOS 비교만 소문자) · **8.0부터 초기화 때만 설정 가능** · 컬럼·인덱스·루틴 이름 = 구분 안 함 · 별칭 = 테이블 별칭은 OS 따름 | |
| 문자열 | `'…'` · `"…"`(ANSI_QUOTES 없을 때) · `''` 이중화 | |
| 백슬래시 이스케이프 | `\0 \' \" \b \n \r \t \Z \\ \% \_` · `NO_BACKSLASH_ESCAPES`면 일반 글자 | 분할기 = 모드 의존 |
| 문자셋 소개자 | `_utf8mb4'abc'` · `_binary'..'` · `N'…'`(= `_utf8`/national) · `COLLATE` 접미 | |
| 16진 | `X'4D'` · `0x4D` | |
| 비트 | `b'0101'` · `0b0101` | |
| 숫자 | `1` `1.5` `1e3` · `.5` | |
| 날짜 리터럴 | `DATE '2026-01-01'` · `TIME '…'` · `TIMESTAMP '…'` · ODBC `{d '…'}`도 수용 | |
| 주석 | `# …` · `-- `(**뒤에 공백/제어 문자 필수**) · `/* … */` · `/*! 실행되는 주석 */` · `/*!80013 버전 조건 */` · `/*+ 옵티마이저 힌트 */` | `/*!`·`/*+`는 주석이 아님(실행됨) |
| 문 종결 | `;` · 클라이언트 `\g`·`\G` | |
| DELIMITER | `DELIMITER $$` = **mysql 클라이언트 명령**(서버 문법 아님) · 루틴 본문의 `;` 보호 | 클라이언트(=nexa-sql)가 처리해야 함 |
| 자리표시자 | `?`(위치) — 이름 붙은 매개변수 없음 · 사용자 변수 `@v` · 시스템 변수 `@@g`/`@@session.x` | |

출처: https://dev.mysql.com/doc/refman/8.0/en/identifiers.html · https://dev.mysql.com/doc/refman/8.0/en/identifier-length.html · https://dev.mysql.com/doc/refman/8.0/en/identifier-case-sensitivity.html · https://dev.mysql.com/doc/refman/8.0/en/string-literals.html · https://dev.mysql.com/doc/refman/8.0/en/charset-introducer.html · https://dev.mysql.com/doc/refman/8.0/en/hexadecimal-literals.html · https://dev.mysql.com/doc/refman/8.0/en/bit-value-literals.html · https://dev.mysql.com/doc/refman/8.0/en/comments.html · https://dev.mysql.com/doc/refman/8.0/en/stored-programs-defining.html · https://dev.mysql.com/doc/refman/8.0/en/sql-mode.html

### T-3. 키워드 · 예약어

- 8.0 예약어(R) = **260개**(아래 · 공식 목록의 (R) 표시 집계). 비예약 키워드(예: `ACTION`, `BIT`, `DATE`, `TEXT`, `TIMESTAMP`, `STATUS`, `COMMENT`…)는 인용 없이 식별자로 쓸 수 있다. 전체·여부 = `INFORMATION_SCHEMA.KEYWORDS`(8.0.13+ · `WORD`, `RESERVED`) — **런타임 조회를 원천으로 삼는 것을 권장**.
- 8.0에서 새로 예약(5.7 대비 23): `CUME_DIST DENSE_RANK EMPTY EXCEPT FIRST_VALUE GROUPING GROUPS INTERSECT JSON_TABLE LAG LAST_VALUE LATERAL LEAD NTH_VALUE NTILE OF OVER PERCENT_RANK RANK RECURSIVE ROW_NUMBER SYSTEM WINDOW`.
- 8.4 차이: 예약 추가 `QUALIFY`·`TABLESAMPLE` · 예약 제거 `MASTER_BIND`·`MASTER_SSL_VERIFY_SERVER_CERT` → 8.4 = 260개(**추정** · 순증감 0으로 계산).

```
ACCESSIBLE ADD ALL ALTER ANALYZE AND AS ASC ASENSITIVE BEFORE BETWEEN BIGINT BINARY BLOB BOTH BY
CALL CASCADE CASE CHANGE CHAR CHARACTER CHECK COLLATE COLUMN CONDITION CONSTRAINT CONTINUE CONVERT
CREATE CROSS CUBE CUME_DIST CURRENT_DATE CURRENT_TIME CURRENT_TIMESTAMP CURRENT_USER CURSOR
DATABASE DATABASES DAY_HOUR DAY_MICROSECOND DAY_MINUTE DAY_SECOND DEC DECIMAL DECLARE DEFAULT
DELAYED DELETE DENSE_RANK DESC DESCRIBE DETERMINISTIC DISTINCT DISTINCTROW DIV DOUBLE DROP DUAL
EACH ELSE ELSEIF EMPTY ENCLOSED ESCAPED EXCEPT EXISTS EXIT EXPLAIN FALSE FETCH FIRST_VALUE FLOAT
FLOAT4 FLOAT8 FOR FORCE FOREIGN FROM FULLTEXT FUNCTION GENERATED GET GRANT GROUP GROUPING GROUPS
HAVING HIGH_PRIORITY HOUR_MICROSECOND HOUR_MINUTE HOUR_SECOND IF IGNORE IN INFILE INNER INOUT
INSENSITIVE INSERT INT INT1 INT2 INT3 INT4 INT8 INTEGER INTERSECT INTERVAL INTO IO_AFTER_GTIDS
IO_BEFORE_GTIDS IS ITERATE JOIN JSON_TABLE KEY KEYS KILL LAG LAST_VALUE LATERAL LEAD LEADING LEAVE
LEFT LIKE LIMIT LINEAR LINES LOAD LOCALTIME LOCALTIMESTAMP LOCK LONG LONGBLOB LONGTEXT LOOP
LOW_PRIORITY MASTER_BIND MASTER_SSL_VERIFY_SERVER_CERT MATCH MAXVALUE MEDIUMBLOB MEDIUMINT
MEDIUMTEXT MIDDLEINT MINUTE_MICROSECOND MINUTE_SECOND MOD MODIFIES NATURAL NOT NO_WRITE_TO_BINLOG
NTH_VALUE NTILE NULL NUMERIC OF ON OPTIMIZE OPTIMIZER_COSTS OPTION OPTIONALLY OR ORDER OUT OUTER
OUTFILE OVER PARTITION PERCENT_RANK PRECISION PRIMARY PROCEDURE PURGE RANGE RANK READ READS
READ_WRITE REAL RECURSIVE REFERENCES REGEXP RELEASE RENAME REPEAT REPLACE REQUIRE RESIGNAL RETURN
REVOKE RIGHT RLIKE ROW ROWS ROW_NUMBER SCHEMA SCHEMAS SECOND_MICROSECOND SELECT SENSITIVE SEPARATOR
SET SHOW SIGNAL SMALLINT SPATIAL SPECIFIC SQL SQLEXCEPTION SQLSTATE SQLWARNING SQL_BIG_RESULT
SQL_CALC_FOUND_ROWS SQL_SMALL_RESULT SSL STARTING STORED STRAIGHT_JOIN SYSTEM TABLE TERMINATED THEN
TINYBLOB TINYINT TINYTEXT TO TRAILING TRIGGER TRUE UNDO UNION UNIQUE UNLOCK UNSIGNED UPDATE USAGE
USE USING UTC_DATE UTC_TIME UTC_TIMESTAMP VALUES VARBINARY VARCHAR VARCHARACTER VARYING VIRTUAL
WHEN WHERE WHILE WINDOW WITH WRITE XOR YEAR_MONTH ZEROFILL
```

출처: https://dev.mysql.com/doc/refman/8.0/en/keywords.html · https://dev.mysql.com/doc/refman/8.4/en/keywords.html · https://dev.mysql.com/doc/refman/8.0/en/information-schema-keywords-table.html

### T-4. 데이터 타입

| 분류 | 타입 | 메모 |
|---|---|---|
| 정수 | `TINYINT`(1B) `SMALLINT`(2) `MEDIUMINT`(3) `INT`/`INTEGER`(4) `BIGINT`(8) · 각 `[UNSIGNED] [ZEROFILL]` | 표시 폭 `INT(11)`·`ZEROFILL` = 8.0.17 사용 중단 · `SERIAL` = `BIGINT UNSIGNED NOT NULL AUTO_INCREMENT UNIQUE` |
| 참거짓 | `BOOL`/`BOOLEAN` = `TINYINT(1)` 별칭 · `TRUE`=1 `FALSE`=0 | 진짜 불리언 타입 없음 |
| 고정 소수 | `DECIMAL(M,D)`/`DEC`/`NUMERIC`/`FIXED` · M ≤ 65 · D ≤ 30 | |
| 부동 소수 | `FLOAT`(4B) · `DOUBLE`/`DOUBLE PRECISION`/`REAL`(8B · `REAL_AS_FLOAT`면 FLOAT) · `FLOAT(p)` | `FLOAT(M,D)`·UNSIGNED = 사용 중단 |
| 비트 | `BIT(M)` M 1~64 | |
| 날짜·시간 | `DATE` · `TIME[(fsp)]`(−838:59:59~) · `DATETIME[(fsp)]`(1000~9999) · `TIMESTAMP[(fsp)]`(1970~2038 · UTC 저장·세션 TZ 변환) · `YEAR`(4자리) · fsp 0~6 | 0 날짜 `'0000-00-00'` = `NO_ZERO_DATE` 모드 의존 |
| 문자 | `CHAR(M)` M ≤ 255 · `VARCHAR(M)` ≤ 65,535바이트(행 한도) · `NATIONAL CHAR`/`NCHAR` = utf8 | 기본 문자셋 8.0 = `utf8mb4` · 기본 콜레이션 `utf8mb4_0900_ai_ci` |
| 텍스트 | `TINYTEXT`(255B) `TEXT`(64KB) `MEDIUMTEXT`(16MB) `LONGTEXT`(4GB) | |
| 이진 | `BINARY(M)` · `VARBINARY(M)` · `TINYBLOB` `BLOB` `MEDIUMBLOB` `LONGBLOB` | |
| 열거·집합 | `ENUM('a','b',…)` ≤ 65,535 · `SET('a',…)` ≤ 64 멤버 | 값 목록 = 타입 일부(`COLUMN_TYPE`) |
| JSON | `JSON`(이진 저장 · 최대 `max_allowed_packet`) | |
| 공간 | `GEOMETRY POINT LINESTRING POLYGON MULTIPOINT MULTILINESTRING MULTIPOLYGON GEOMETRYCOLLECTION`(`GEOMCOLLECTION`) · SRID 속성(8.0) | |
| 생성 컬럼 | `col 타입 AS (식) [VIRTUAL|STORED]` | |
| 없음 | `INTERVAL` 타입 · 배열 · UUID 타입(함수 `UUID_TO_BIN`) · 시간대 포함 타입 | |

출처: https://dev.mysql.com/doc/refman/8.0/en/data-types.html · https://dev.mysql.com/doc/refman/8.0/en/numeric-type-syntax.html · https://dev.mysql.com/doc/refman/8.0/en/date-and-time-type-syntax.html · https://dev.mysql.com/doc/refman/8.0/en/string-type-syntax.html · https://dev.mysql.com/doc/refman/8.0/en/spatial-type-overview.html · https://dev.mysql.com/doc/refman/8.0/en/json.html

### T-5. 연산자 · 우선순위 · NULL · 콜레이션

| 우선순위(높음→낮음) | 연산자 |
|---|---|
| 1 | `INTERVAL` |
| 2 | `BINARY`, `COLLATE` |
| 3 | `!` |
| 4 | 단항 `-`, `~` |
| 5 | `^` |
| 6 | `*` `/` `DIV` `%` `MOD` |
| 7 | `-` `+` |
| 8 | `<<` `>>` |
| 9 | `&` |
| 10 | `\|` |
| 11 | `=`(비교) `<=>` `>=` `>` `<=` `<` `<>` `!=` `IS` `LIKE` `REGEXP` `IN` `MEMBER OF` |
| 12 | `BETWEEN` `CASE` `WHEN` `THEN` `ELSE` |
| 13 | `NOT` |
| 14 | `AND` `&&` |
| 15 | `XOR` |
| 16 | `OR` `\|\|` |
| 17 | `=`(대입) `:=` |

| 항목 | 규칙 |
|---|---|
| `\|\|` | 기본 = 논리 OR(8.0.17 사용 중단 경고) · `PIPES_AS_CONCAT` 모드면 문자열 연결 · `HIGH_NOT_PRECEDENCE`는 `NOT` 우선순위 변경 |
| 연결 | `CONCAT(a,b,…)`(NULL 하나면 NULL) · `CONCAT_WS(sep,…)`(NULL 건너뜀) |
| NULL | 3값 논리 · `NULL = NULL` → NULL · `<=>` = NULL 안전 동등(`NULL<=>NULL` = 1) · `IS [NOT] NULL` · `IFNULL`/`COALESCE`/`NULLIF` |
| NULL 정렬 | `ASC` = NULL **먼저** · `DESC` = 나중 · `NULLS FIRST/LAST` 구문 없음(우회 `ORDER BY x IS NULL, x`) |
| 빈 문자열 | `''` ≠ NULL(Oracle과 다름) |
| JSON | `col->'$.a'` = `JSON_EXTRACT` · `->>` = `JSON_UNQUOTE(JSON_EXTRACT())` |
| 정수 나눗셈 | `/` = 소수 결과 · `DIV` = 정수 |
| 콜레이션 | 서버→DB→테이블→컬럼 상속 · 접미 `_ci`/`_cs`/`_bin`/`_ai`/`_as` · `_0900_` = UCA 9.0 · NO PAD(0900 계열 = 뒤 공백 의미 있음) vs PAD SPACE · 식에 `COLLATE x` · 강제성(coercibility) 충돌 = 오류 1267 |

출처: https://dev.mysql.com/doc/refman/8.0/en/operator-precedence.html · https://dev.mysql.com/doc/refman/8.0/en/comparison-operators.html · https://dev.mysql.com/doc/refman/8.0/en/logical-operators.html · https://dev.mysql.com/doc/refman/8.0/en/working-with-null.html · https://dev.mysql.com/doc/refman/8.0/en/sorting-rows.html · https://dev.mysql.com/doc/refman/8.0/en/charset-collation-names.html

### T-6. 함수(내장 함수·연산자 참조 전수 · 분류는 MySQL 장 기준)

| 분류(장) | 이름 |
|---|---|
| 흐름 제어 | `CASE` `IF(c,a,b)` `IFNULL(a,b)` `NULLIF(a,b)` |
| 비교 | `COALESCE` `GREATEST` `LEAST` `INTERVAL(n,n1,…)` `ISNULL` `IN` `BETWEEN` `STRCMP` |
| 문자열 | `ASCII BIN BIT_LENGTH CHAR CHAR_LENGTH CHARACTER_LENGTH CONCAT CONCAT_WS ELT EXPORT_SET FIELD FIND_IN_SET FORMAT FROM_BASE64 HEX INSERT INSTR LCASE LEFT LENGTH LIKE LOAD_FILE LOCATE LOWER LPAD LTRIM MAKE_SET MATCH MID NOT LIKE OCT OCTET_LENGTH ORD POSITION QUOTE REPEAT REPLACE REVERSE RIGHT RPAD RTRIM SOUNDEX SOUNDS LIKE SPACE STRCMP SUBSTR SUBSTRING SUBSTRING_INDEX TO_BASE64 TRIM UCASE UNHEX UPPER WEIGHT_STRING` |
| 정규식 | `REGEXP RLIKE NOT REGEXP REGEXP_INSTR REGEXP_LIKE REGEXP_REPLACE REGEXP_SUBSTR`(8.0 = ICU) |
| 수치 | `ABS ACOS ASIN ATAN ATAN2 CEIL CEILING CONV COS COT CRC32 DEGREES DIV EXP FLOOR LN LOG LOG10 LOG2 MOD PI POW POWER RADIANS RAND ROUND SIGN SIN SQRT TAN TRUNCATE` |
| 날짜·시간 | `ADDDATE ADDTIME CONVERT_TZ CURDATE CURRENT_DATE CURRENT_TIME CURRENT_TIMESTAMP CURTIME DATE DATE_ADD DATE_FORMAT DATE_SUB DATEDIFF DAY DAYNAME DAYOFMONTH DAYOFWEEK DAYOFYEAR EXTRACT FROM_DAYS FROM_UNIXTIME GET_FORMAT HOUR LAST_DAY LOCALTIME LOCALTIMESTAMP MAKEDATE MAKETIME MICROSECOND MINUTE MONTH MONTHNAME NOW PERIOD_ADD PERIOD_DIFF QUARTER SEC_TO_TIME SECOND STR_TO_DATE SUBDATE SUBTIME SYSDATE TIME TIME_FORMAT TIME_TO_SEC TIMEDIFF TIMESTAMP TIMESTAMPADD TIMESTAMPDIFF TO_DAYS TO_SECONDS UNIX_TIMESTAMP UTC_DATE UTC_TIME UTC_TIMESTAMP WEEK WEEKDAY WEEKOFYEAR YEAR YEARWEEK` |
| 형 변환·문자셋 | `CAST(x AS t)` `CONVERT(x, t)` `CONVERT(x USING cs)` `BINARY` · CAST 대상 = `BINARY CHAR DATE DATETIME DECIMAL DOUBLE FLOAT(8.0.17) JSON NCHAR REAL SIGNED [INTEGER] UNSIGNED TIME YEAR(8.0.22)` + 공간 타입(8.0.24) |
| 집계 | `AVG BIT_AND BIT_OR BIT_XOR COUNT COUNT(DISTINCT) GROUP_CONCAT(… ORDER BY … SEPARATOR …) JSON_ARRAYAGG JSON_OBJECTAGG MAX MIN STD STDDEV STDDEV_POP STDDEV_SAMP SUM VAR_POP VAR_SAMP VARIANCE` · `GROUPING`(ROLLUP) |
| 윈도우 | `CUME_DIST DENSE_RANK FIRST_VALUE LAG LAST_VALUE LEAD NTH_VALUE NTILE PERCENT_RANK RANK ROW_NUMBER` + 집계의 `OVER` |
| JSON | `JSON_ARRAY JSON_ARRAY_APPEND JSON_ARRAY_INSERT JSON_CONTAINS JSON_CONTAINS_PATH JSON_DEPTH JSON_EXTRACT JSON_INSERT JSON_KEYS JSON_LENGTH JSON_MERGE(사용 중단) JSON_MERGE_PATCH JSON_MERGE_PRESERVE JSON_OBJECT JSON_OVERLAPS JSON_PRETTY JSON_QUOTE JSON_REMOVE JSON_REPLACE JSON_SCHEMA_VALID JSON_SCHEMA_VALIDATION_REPORT JSON_SEARCH JSON_SET JSON_STORAGE_FREE JSON_STORAGE_SIZE JSON_TABLE JSON_TYPE JSON_UNQUOTE JSON_VALID JSON_VALUE MEMBER OF -> ->>` |
| 암호·압축 | `AES_DECRYPT AES_ENCRYPT COMPRESS MD5 RANDOM_BYTES SHA SHA1 SHA2 STATEMENT_DIGEST STATEMENT_DIGEST_TEXT UNCOMPRESS UNCOMPRESSED_LENGTH VALIDATE_PASSWORD_STRENGTH` |
| 정보 | `BENCHMARK CHARSET COERCIBILITY COLLATION CONNECTION_ID CURRENT_ROLE CURRENT_USER DATABASE FOUND_ROWS ICU_VERSION LAST_INSERT_ID ROLES_GRAPHML ROW_COUNT SCHEMA SESSION_USER SYSTEM_USER USER VERSION` |
| 잠금 | `GET_LOCK IS_FREE_LOCK IS_USED_LOCK RELEASE_ALL_LOCKS RELEASE_LOCK` |
| 기타 | `ANY_VALUE BIN_TO_UUID DEFAULT GROUPING INET_ATON INET_NTOA INET6_ATON INET6_NTOA IS_IPV4 IS_IPV4_COMPAT IS_IPV4_MAPPED IS_IPV6 IS_UUID NAME_CONST SLEEP UUID UUID_SHORT UUID_TO_BIN VALUES` |
| 비트 연산 | `& \| ^ ~ << >> BIT_COUNT` |
| XML | `ExtractValue UpdateXML` |
| 전문 검색 | `MATCH(cols) AGAINST(expr [IN NATURAL LANGUAGE MODE \| IN BOOLEAN MODE \| WITH QUERY EXPANSION])` |
| 성능 스키마 | `FORMAT_BYTES FORMAT_PICO_TIME PS_CURRENT_THREAD_ID PS_THREAD_ID` |
| 복제·GTID | `GTID_SUBSET GTID_SUBTRACT MASTER_POS_WAIT SOURCE_POS_WAIT WAIT_FOR_EXECUTED_GTID_SET WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS(8.4 제거 · 추정)` · `asynchronous_connection_failover_*` 5종 · `group_replication_*` 10종 |
| 공간(약 130) | 생성자 `Point LineString Polygon MultiPoint MultiLineString MultiPolygon GeomCollection GeometryCollection` · `MBR*`(Contains CoveredBy Covers Disjoint Equals Intersects Overlaps Touches Within) · `ST_*`(Area AsBinary/AsWKB AsGeoJSON AsText/AsWKT Buffer Buffer_Strategy Centroid Collect Contains ConvexHull Crosses Difference Dimension Disjoint Distance Distance_Sphere EndPoint Envelope Equals ExteriorRing FrechetDistance GeoHash GeomFrom{Text,WKB,GeoJSON} … Intersection Intersects IsClosed IsEmpty IsSimple IsValid Latitude Longitude Length LineInterpolatePoint(s) MakeEnvelope NumGeometries NumInteriorRing(s) NumPoints Overlaps PointAtDistance PointN SRID Simplify StartPoint SwapXY SymDifference Touches Transform Union Validate Within X Y 등) |
| 내부(사용자 비공개) | `INTERNAL_*` 21종 · `GET_DD_*` 3종 · `CAN_ACCESS_*` 5종 — `information_schema` 뷰 구현용 · 완성 목록에서 제외 권장 |

자주 쓰는 시그니처: `SUBSTRING(s, pos [, len])`/`SUBSTRING(s FROM pos FOR len)` · `LOCATE(sub, s [, pos])` · `DATE_FORMAT(d, '%Y-%m-%d %H:%i:%s')` · `STR_TO_DATE(s, fmt)` · `DATE_ADD(d, INTERVAL n unit)` · `TIMESTAMPDIFF(unit, a, b)` · `GROUP_CONCAT([DISTINCT] x [ORDER BY …] [SEPARATOR ','])` · `IF(cond, a, b)` · `JSON_EXTRACT(doc, path, …)` · `LAST_INSERT_ID()`.

출처: https://dev.mysql.com/doc/refman/8.0/en/built-in-function-reference.html · https://dev.mysql.com/doc/refman/8.0/en/functions.html · https://dev.mysql.com/doc/refman/8.0/en/cast-functions.html · https://dev.mysql.com/doc/refman/8.0/en/aggregate-functions.html · https://dev.mysql.com/doc/refman/8.0/en/window-function-descriptions.html

### T-7. 질의

| 항목 | 내용 |
|---|---|
| 절 순서 | `WITH` → `SELECT [ALL\|DISTINCT\|DISTINCTROW] [HIGH_PRIORITY] [STRAIGHT_JOIN] [SQL_*]` → `FROM` → `WHERE` → `GROUP BY … [WITH ROLLUP]` → `HAVING` → `WINDOW` → `ORDER BY` → `LIMIT` → `FOR UPDATE\|SHARE` · `INTO`(변수·파일) = 끝 또는 FROM 앞 |
| LIMIT | `LIMIT n` · `LIMIT off, n` · `LIMIT n OFFSET off` · `FETCH FIRST` **없음** |
| CTE | `WITH [RECURSIVE]` (8.0) · 재귀 깊이 `cte_max_recursion_depth` 1000 |
| 윈도우 | `OVER (PARTITION BY … ORDER BY … ROWS\|RANGE …)` · 이름 붙은 `WINDOW w AS (…)` (8.0) |
| 집합 연산 | `UNION [ALL\|DISTINCT]` · `INTERSECT`·`EXCEPT` = **8.0.31** · 괄호 묶음 |
| 조인 | `INNER/CROSS/LEFT/RIGHT [OUTER]/NATURAL` · `STRAIGHT_JOIN` · **FULL OUTER 없음**(UNION 우회) · `USING`/`ON` |
| LATERAL | 파생 테이블 `LATERAL (…)` = 8.0.14 · `JSON_TABLE` |
| `VALUES` 문 | `VALUES ROW(1,2), ROW(3,4)` = 8.0.19 · `TABLE t` 문(8.0.19) |
| 인덱스 힌트 | `t USE\|FORCE\|IGNORE {INDEX\|KEY} [FOR JOIN\|ORDER BY\|GROUP BY] (i1, …)` |
| 옵티마이저 힌트 | `SELECT /*+ BKA(t) NO_ICP(t) INDEX(t i) JOIN_ORDER(a,b) MAX_EXECUTION_TIME(1000) SET_VAR(…) */` (주석처럼 보이나 의미 있음) |
| 잠금 읽기 | `FOR UPDATE` · `FOR SHARE`(8.0 · 옛 `LOCK IN SHARE MODE`) · `OF t` · `NOWAIT` · `SKIP LOCKED` (8.0) |
| ROLLUP | `GROUP BY a, b WITH ROLLUP` · `GROUPING()` (8.0.1) · `ROLLUP()` 함수형 구문 없음(**추정**) · `CUBE` 미지원(예약만) |
| 8.4 | `QUALIFY`·`TABLESAMPLE` 예약됨 — 기능 제공 여부는 HeatWave 의존(**추정**) |
| 실행 계획 | `EXPLAIN [FORMAT=TRADITIONAL\|JSON\|TREE]` · `EXPLAIN ANALYZE`(8.0.18) |

출처: https://dev.mysql.com/doc/refman/8.0/en/select.html · https://dev.mysql.com/doc/refman/8.0/en/with.html · https://dev.mysql.com/doc/refman/8.0/en/window-functions-usage.html · https://dev.mysql.com/doc/refman/8.0/en/set-operations.html · https://dev.mysql.com/doc/refman/8.0/en/join.html · https://dev.mysql.com/doc/refman/8.0/en/lateral-derived-tables.html · https://dev.mysql.com/doc/refman/8.0/en/index-hints.html · https://dev.mysql.com/doc/refman/8.0/en/optimizer-hints.html · https://dev.mysql.com/doc/refman/8.0/en/innodb-locking-reads.html · https://dev.mysql.com/doc/refman/8.0/en/group-by-modifiers.html

### T-8. DML

| 문 | 형태 · 메모 |
|---|---|
| INSERT | `INSERT [LOW_PRIORITY\|HIGH_PRIORITY] [IGNORE] INTO t [(cols)] VALUES (…),(…) \| SELECT … \| SET c=v` |
| 업서트 | `… ON DUPLICATE KEY UPDATE c = VALUES(c)` → 8.0.20+ 권장 `INSERT … VALUES (…) AS new ON DUPLICATE KEY UPDATE c = new.c` · 영향 행 수 1(삽입)/2(갱신)/0(변화 없음) |
| INSERT IGNORE | 키 충돌·변환 오류를 경고로 강등 |
| REPLACE | 충돌 행 DELETE 후 INSERT(트리거·FK 주의) |
| UPDATE | 단일 = `ORDER BY … LIMIT n` 가능 · 다중 테이블 `UPDATE a JOIN b ON … SET a.x=b.y` |
| DELETE | 단일 = `ORDER BY … LIMIT` · 다중 `DELETE a, b FROM a JOIN b …` / `DELETE FROM a USING a JOIN b` |
| RETURNING | **없음**(MariaDB만) · `LAST_INSERT_ID()` · `ROW_COUNT()` |
| MERGE | **없음** |
| TRUNCATE | `TRUNCATE [TABLE] t` = DDL(암묵 커밋 · 롤백 불가 · AUTO_INCREMENT 초기화) |
| LOAD DATA | `LOAD DATA [LOCAL] INFILE 'f' [REPLACE\|IGNORE] INTO TABLE t FIELDS TERMINATED BY … LINES …` · `local_infile` 서버/클라이언트 양쪽 허용 필요 · `SELECT … INTO OUTFILE`(서버 파일) |
| 기타 | `INSERT … SELECT` · `DO expr` · `HANDLER` · `IMPORT TABLE` · 안전 모드 `sql_safe_updates`(키 없는 UPDATE/DELETE 거부) |

출처: https://dev.mysql.com/doc/refman/8.0/en/insert.html · https://dev.mysql.com/doc/refman/8.0/en/insert-on-duplicate.html · https://dev.mysql.com/doc/refman/8.0/en/replace.html · https://dev.mysql.com/doc/refman/8.0/en/update.html · https://dev.mysql.com/doc/refman/8.0/en/delete.html · https://dev.mysql.com/doc/refman/8.0/en/truncate-table.html · https://dev.mysql.com/doc/refman/8.0/en/load-data.html

### T-9. 객체 종류

| 객체 | CREATE / ALTER / DROP | OR REPLACE · IF [NOT] EXISTS | 이름 | 카탈로그 · 정의 조회 | 메모 |
|---|---|---|---|---|---|
| DATABASE = SCHEMA | ✓/✓/✓ | IF NOT EXISTS · IF EXISTS | `db` | `information_schema.SCHEMATA` · `SHOW CREATE DATABASE` | 카탈로그 층 없음(`def` 고정) · 스키마 = DB |
| TABLE | ✓/✓/✓ (`RENAME TABLE`) | IF NOT EXISTS · IF EXISTS | `db.t` | `TABLES` · `COLUMNS` · `SHOW CREATE TABLE` · `SHOW [FULL] COLUMNS` | `TEMPORARY`(세션 · 카탈로그에 안 보임) · `PARTITION BY RANGE/LIST/HASH/KEY` · 엔진 `InnoDB`(기본)/`MyISAM`/`MEMORY`/`CSV`/`ARCHIVE`/`FEDERATED` · `CREATE TABLE … LIKE`/`AS SELECT` |
| VIEW | ✓/✓/✓ | **OR REPLACE** · DROP IF EXISTS | `db.v` | `VIEWS` · `SHOW CREATE VIEW` | `ALGORITHM` · `DEFINER` · `SQL SECURITY` · `WITH CHECK OPTION` · 물리화 뷰 없음 |
| INDEX | ✓/(ALTER TABLE)/✓ | IF 없음(**추정** · 8.0 미지원) | 테이블 소속 | `STATISTICS` · `SHOW INDEX` | `BTREE`·`HASH`(MEMORY) · `FULLTEXT`(ngram 파서 = 한중일) · `SPATIAL` · 함수형 `((expr))` = 8.0.13 · `INVISIBLE` = 8.0 · 내림차순 인덱스 8.0 · 접두 길이 `col(10)` |
| 제약 | ALTER TABLE ADD/DROP | — | 테이블 소속 | `TABLE_CONSTRAINTS` · `KEY_COLUMN_USAGE` · `REFERENTIAL_CONSTRAINTS` · `CHECK_CONSTRAINTS` | PK 이름 항상 `PRIMARY` · UNIQUE · FK(InnoDB만) · `CHECK` = **8.0.16**(이전은 무시) · `[NOT] ENFORCED` |
| TRIGGER | ✓/✗/✓ | IF NOT EXISTS(8.0.29) · IF EXISTS | `db.trg` | `TRIGGERS` · `SHOW CREATE TRIGGER` | `BEFORE\|AFTER INSERT\|UPDATE\|DELETE … FOR EACH ROW` · 문장 단위·INSTEAD OF·DDL 트리거 없음 · `FOLLOWS/PRECEDES` 순서 · `NEW.`/`OLD.` |
| PROCEDURE | ✓/✓(특성만)/✓ | IF NOT EXISTS(8.0.29) · IF EXISTS | `db.p` | `ROUTINES` · `PARAMETERS` · `SHOW CREATE PROCEDURE` · `SHOW PROCEDURE STATUS` | 본문 변경 = DROP + CREATE · 오버로드 **불가** |
| FUNCTION | ✓/✓/✓ | 동일 | `db.f` | 동일 | 스칼라만(테이블 함수 없음) · `RETURNS 타입` · 바이너리 로그 시 `DETERMINISTIC`/`READS SQL DATA` 필요(`log_bin_trust_function_creators`) · UDF/로더블 함수 = `CREATE FUNCTION … SONAME` |
| EVENT | ✓/✓/✓ | IF NOT EXISTS · IF EXISTS | `db.e` | `EVENTS` · `SHOW CREATE EVENT` | `ON SCHEDULE AT\|EVERY` · `event_scheduler` |
| SEQUENCE | **없음** | — | — | — | `AUTO_INCREMENT` 컬럼(테이블당 1) · `AUTO_INCREMENT` 값 = `TABLES.AUTO_INCREMENT` |
| SYNONYM | **없음** | — | — | — | |
| PACKAGE · TYPE | **없음** | — | — | — | |
| USER | ✓/✓/✓ · `RENAME USER` | IF NOT EXISTS · IF EXISTS | `'u'@'host'` | `mysql.user` · `SHOW CREATE USER` · `SHOW GRANTS` | 계정 = 이름 + 호스트 |
| ROLE | ✓/✗/✓ | IF NOT EXISTS · IF EXISTS | `'r'@'%'` | `mysql.role_edges` · `information_schema.APPLICABLE_ROLES`(8.0.19) | 8.0 |
| SERVER | ✓/✓/✓ | — | 이름 | `mysql.servers` | FEDERATED 엔진용 |
| TABLESPACE · LOGFILE GROUP · SPATIAL REFERENCE SYSTEM · RESOURCE GROUP | ✓/✓/✓ | 일부 | 전역 | `FILES` · `ST_SPATIAL_REFERENCE_SYSTEMS` · `RESOURCE_GROUPS` | 관리 객체 |

공통: 오버로드 불가 · 이름 2부(`db.obj`) · `information_schema` 컬럼 `TABLE_CATALOG` = `def` · 메타 조회 = `information_schema` 우선(8.0 = 데이터 사전 기반 · 빠름) · `SHOW CREATE …` = DDL 원문의 단일 원천.

출처: https://dev.mysql.com/doc/refman/8.0/en/sql-data-definition-statements.html · https://dev.mysql.com/doc/refman/8.0/en/create-table.html · https://dev.mysql.com/doc/refman/8.0/en/create-index.html · https://dev.mysql.com/doc/refman/8.0/en/create-table-check-constraints.html · https://dev.mysql.com/doc/refman/8.0/en/create-view.html · https://dev.mysql.com/doc/refman/8.0/en/create-trigger.html · https://dev.mysql.com/doc/refman/8.0/en/create-procedure.html · https://dev.mysql.com/doc/refman/8.0/en/create-event.html · https://dev.mysql.com/doc/refman/8.0/en/create-server.html · https://dev.mysql.com/doc/refman/8.0/en/information-schema.html · https://dev.mysql.com/doc/refman/8.0/en/invisible-indexes.html

### T-10. 루틴

| 항목 | 내용 |
|---|---|
| 매개변수 | 프로시저 `IN`(기본)/`OUT`/`INOUT` · 함수 = IN만 |
| 기본값 · 이름 인자 | **둘 다 없음**(모든 인자 위치로 전달) |
| 호출 | `CALL p(1, @out)` · 괄호 생략 가능(인자 없을 때) · OUT 값 = 사용자 변수로 받고 `SELECT @out` · 프로시저 안 `SELECT` = 결과 집합(여러 개 가능) |
| 본문 | `BEGIN … END` · `DECLARE` 변수/조건/커서/핸들러(`CONTINUE\|EXIT HANDLER FOR SQLEXCEPTION`) · `IF/CASE/LOOP/WHILE/REPEAT/LEAVE/ITERATE` · `SIGNAL/RESIGNAL SQLSTATE '45000'` · 라벨 |
| 특성 | `[NOT] DETERMINISTIC` · `CONTAINS SQL\|NO SQL\|READS SQL DATA\|MODIFIES SQL DATA` · `SQL SECURITY DEFINER\|INVOKER` · `COMMENT` · `LANGUAGE SQL` · `DEFINER = user` |
| 구분자 | 클라이언트에서 `DELIMITER //` 필요(API로 보낼 땐 본문 통째로 한 문장 · DELIMITER 불필요) |
| 동적 SQL | `PREPARE s FROM @sql; EXECUTE s USING @a; DEALLOCATE PREPARE s;` |
| 오류 정보 | `GET DIAGNOSTICS` |

출처: https://dev.mysql.com/doc/refman/8.0/en/create-procedure.html · https://dev.mysql.com/doc/refman/8.0/en/call.html · https://dev.mysql.com/doc/refman/8.0/en/sql-compound-statements.html · https://dev.mysql.com/doc/refman/8.0/en/stored-programs-defining.html · https://dev.mysql.com/doc/refman/8.0/en/sql-prepared-statements.html

### T-11. 트랜잭션 · 세션

| 항목 | 내용 |
|---|---|
| 자동 커밋 | `autocommit = 1` 기본 · `SET autocommit = 0` |
| 시작 | `START TRANSACTION [READ ONLY\|READ WRITE\|WITH CONSISTENT SNAPSHOT]` · `BEGIN [WORK]`(루틴 안에선 블록 시작이라 쓸 수 없음) |
| 종료 | `COMMIT [AND [NO] CHAIN] [[NO] RELEASE]` · `ROLLBACK` |
| 세이브포인트 | `SAVEPOINT s` · `ROLLBACK TO [SAVEPOINT] s` · `RELEASE SAVEPOINT s` |
| 암묵 커밋 | 거의 모든 DDL(CREATE/ALTER/DROP/TRUNCATE/RENAME) · 계정 관리(GRANT 등) · `LOCK TABLES` · `LOAD DATA`(일부) — 8.0 DDL = 원자적(atomic DDL)이나 롤백 불가 |
| 격리 | 기본 **REPEATABLE READ**(InnoDB · 갭 락) · `SET [SESSION\|GLOBAL] TRANSACTION ISOLATION LEVEL …` · `transaction_isolation` 변수 |
| 현재 DB | `USE db` · `SELECT DATABASE()` |
| 세션 설정 | `SET [SESSION] var = v` · `SET @@sql_mode` · `SET NAMES utf8mb4` · `SET time_zone` · `SET PERSIST`(8.0) |
| 잠금 대기 | `innodb_lock_wait_timeout`(50초) · `lock_wait_timeout` · `max_execution_time`(SELECT만 · ms) |
| 열린 트랜잭션 조회 | `information_schema.INNODB_TRX` · `performance_schema.data_locks` |

출처: https://dev.mysql.com/doc/refman/8.0/en/commit.html · https://dev.mysql.com/doc/refman/8.0/en/savepoint.html · https://dev.mysql.com/doc/refman/8.0/en/implicit-commit.html · https://dev.mysql.com/doc/refman/8.0/en/innodb-transaction-isolation-levels.html · https://dev.mysql.com/doc/refman/8.0/en/set-transaction.html · https://dev.mysql.com/doc/refman/8.0/en/atomic-ddl.html

### T-12. DCL

| 항목 | 내용 |
|---|---|
| GRANT | `GRANT priv[(cols)], … ON {*.*\|db.*\|db.t\|PROCEDURE db.p\|FUNCTION db.f} TO user [WITH GRANT OPTION]` · 역할 부여 `GRANT r TO u [WITH ADMIN OPTION]` |
| REVOKE | `REVOKE … FROM user` · `REVOKE ALL PRIVILEGES, GRANT OPTION FROM u` |
| 권한 | 정적(`SELECT INSERT UPDATE DELETE CREATE DROP ALTER INDEX EXECUTE CREATE VIEW SHOW VIEW TRIGGER EVENT REFERENCES PROCESS SUPER FILE RELOAD …`) + 동적(8.0 · `SYSTEM_VARIABLES_ADMIN CONNECTION_ADMIN BACKUP_ADMIN ROLE_ADMIN …`) |
| 역할 | 8.0 · `CREATE ROLE` · `SET ROLE` · `SET DEFAULT ROLE` · `mandatory_roles` · `activate_all_roles_on_login` |
| 부분 취소 | `partial_revokes`(8.0.16) |
| 조회 | `SHOW GRANTS [FOR u [USING r]]` · `information_schema.USER_PRIVILEGES/SCHEMA_PRIVILEGES/TABLE_PRIVILEGES/COLUMN_PRIVILEGES` |
| 없음 | `GRANT … TO PUBLIC` · 행 단위 보안 · DENY |

출처: https://dev.mysql.com/doc/refman/8.0/en/grant.html · https://dev.mysql.com/doc/refman/8.0/en/revoke.html · https://dev.mysql.com/doc/refman/8.0/en/roles.html · https://dev.mysql.com/doc/refman/8.0/en/privileges-provided.html · https://dev.mysql.com/doc/refman/8.0/en/partial-revokes.html

### T-13. 오류

| 항목 | 내용 |
|---|---|
| 형태 | `ERROR 1064 (42000): You have an error in your SQL syntax; check the manual … near 'xxx' at line 3` = 번호 + SQLSTATE + 메시지 |
| 번호 대역 | 1000~1999·3000~5000(서버) · 2000~2999(클라이언트 `CR_*` · 예 2013 연결 끊김) · 10000~(서버 로그 전용) |
| 위치 | 구문 오류 = `near '…' at line n`(열 정보 없음 → 클라이언트가 `near` 조각 검색으로 위치 추정) |
| 대표 | 1045 접근 거부 · 1049 DB 없음 · 1062 키 중복(23000) · 1146 테이블 없음(42S02) · 1054 컬럼 없음(42S22) · 1205 잠금 대기 초과 · 1213 교착 · 1451/1452 FK · 3819 CHECK 위반 · 2006 서버 사라짐 · 1317 질의 중단(KILL) |
| 경고 | `SHOW WARNINGS [LIMIT]` · `SHOW ERRORS` · `@@warning_count` · OK 패킷의 경고 수 · `GET DIAGNOSTICS CONDITION 1 @n = MYSQL_ERRNO, @m = MESSAGE_TEXT` |
| 사용자 오류 | `SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = '…'`(번호 1644) |

출처: https://dev.mysql.com/doc/mysql-errors/8.0/en/ · https://dev.mysql.com/doc/mysql-errors/8.0/en/server-error-reference.html · https://dev.mysql.com/doc/mysql-errors/8.0/en/client-error-reference.html · https://dev.mysql.com/doc/refman/8.0/en/show-warnings.html · https://dev.mysql.com/doc/refman/8.0/en/get-diagnostics.html · https://dev.mysql.com/doc/refman/8.0/en/signal.html

### T-14. 클라이언트 · 프로토콜

| 항목 | 내용 |
|---|---|
| 프로토콜 | MySQL Client/Server Protocol(TCP 3306 · 소켓 · 명명된 파이프) · TLS · 인증 플러그인 8.0 기본 `caching_sha2_password`(TLS 없으면 RSA 공개키 교환) · `mysql_native_password` = 8.4 기본 비활성 · X Protocol(33060)은 별개 |
| 텍스트/이진 | `COM_QUERY` = 텍스트 결과(모든 값 글자) · `COM_STMT_PREPARE/EXECUTE` = 이진 결과(서버 측 준비 문) · 커서 페치 `COM_STMT_FETCH`(읽기 전용 커서) |
| 다중 결과 | `CLIENT_MULTI_STATEMENTS`(`;`로 여러 문 한 번에) · `CLIENT_MULTI_RESULTS`(CALL 결과 여럿 + 마지막 OK) · `SERVER_MORE_RESULTS_EXISTS` 플래그 |
| 대량 적재 | `LOAD DATA LOCAL INFILE`(클라이언트가 파일 바이트 전송 · 양쪽 `local_infile` 허용 · 보안상 기본 끔) · 다중 행 `INSERT VALUES (…),(…)`(크기 = `max_allowed_packet` 64MB) |
| 취소 | 별도 연결로 `KILL QUERY <connection_id>`(`CONNECTION_ID()` 먼저 보관) · 프로토콜 수준 취소 없음 · `KILL <id>` = 연결 종료 |
| 스트리밍 | 결과 = 서버가 밀어냄 · 다 읽기 전 같은 연결에 다른 명령 불가 |
| 타임아웃 | `wait_timeout`(28800초 유휴 끊김) · `net_read_timeout`/`net_write_timeout` |
| 마지막 정보 | OK 패킷 = 영향 행 수 · `last_insert_id` · 경고 수 · 상태 플래그(트랜잭션 중 `SERVER_STATUS_IN_TRANS` = 열린 트랜잭션 판정에 사용 가능) |
| Rust crate | `mysql`(동기 · 순수 Rust) · `mysql_async`(tokio) · `sqlx`(mysql feature) — 셋 다 MIT/Apache · **nexa-sql 현재 미사용**(워크스페이스에 MySQL 드라이버 크레이트 없음 · 2026-10-10 확인) · crate별 `LOAD DATA LOCAL` 처리기·KILL 지원 정도는 **추정**(미확인) |

출처: https://dev.mysql.com/doc/dev/mysql-server/latest/PAGE_PROTOCOL.html · https://dev.mysql.com/doc/refman/8.0/en/c-api-multiple-queries.html · https://dev.mysql.com/doc/refman/8.0/en/kill.html · https://dev.mysql.com/doc/refman/8.0/en/load-data-local-security.html · https://dev.mysql.com/doc/refman/8.0/en/caching-sha2-pluggable-authentication.html · https://docs.rs/mysql/latest/mysql/ · https://docs.rs/mysql_async/latest/mysql_async/

---

### T-15 nexa-sql 지원 현황

방언 열 **MY**(MySQL) — 항목별 ✅/부분/❌와 코드 자리는 §11 표의 `MY` 열 · 한 줄 판정은 §11 끝 "요약" · 결함은 §9.

## §7. ODBC(일반)

> 관점 = **백엔드를 모르는 채 ODBC 드라이버로 붙는 경우**. ODBC가 보장하는 것은 API·이스케이프·카탈로그 함수·SQLSTATE뿐이고 SQL 본문은 드라이버를 거쳐 그대로 백엔드로 간다. 표의 "백엔드 의존" = ODBC가 정하지 않음 → `SQLGetInfo`/`SQLGetTypeInfo`로 런타임에 물어야 함.

### T-1. 버전 · 문서

| 항목 | 내용 |
|---|---|
| 규격 | ODBC 3.8(Windows 7 · 비동기 연결·스트리밍 출력 매개변수·드라이버 C 타입 확장) · 3.x = SQL-92·X/Open CLI·ISO/IEC 9075-3(SQL/CLI) 정렬 |
| 드라이버 관리자 | Windows = `odbc32.dll` · Linux/macOS = unixODBC 또는 iODBC |
| 적합성 | API 적합 = Core/Level 1/Level 2(`SQL_ODBC_INTERFACE_CONFORMANCE`) · SQL 적합 = `SQL_SQL_CONFORMANCE`(SQL-92 Entry/FIPS Transitional/Intermediate/Full) |
| 버전 조회 | `SQLGetInfo(SQL_DBMS_NAME/SQL_DBMS_VER/SQL_DRIVER_NAME/SQL_DRIVER_ODBC_VER)` |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/odbc-programmer-s-reference · https://learn.microsoft.com/en-us/sql/odbc/reference/what-s-new-in-odbc-3-8 · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetinfo-function · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/interface-conformance-levels

### T-2. 어휘 · 이스케이프 시퀀스

| 항목 | 형태 | 판정·메모 |
|---|---|---|
| 날짜 | `{d 'yyyy-mm-dd'}` | 드라이버가 백엔드 리터럴로 바꿈 |
| 시간 | `{t 'hh:mm:ss'}` | |
| 타임스탬프 | `{ts 'yyyy-mm-dd hh:mm:ss[.f…]'}` | |
| 간격 | `{INTERVAL [+\|-]'…' 필드}` | 지원 여부 = `SQL_DATETIME_LITERALS` |
| 스칼라 함수 | `{fn 함수(인자)}` | 목록 = T-6 |
| 프로시저 호출 | `{[?=]call proc[(?, ?)]}` | 반환값 `?=` · 이름 인자 `@p = ?`는 ODBC 3.8 이전 일부 드라이버만(**추정**) |
| 외부 조인 | `{oj a LEFT OUTER JOIN b ON …}` | `SQL_OJ_CAPABILITIES` |
| LIKE 이스케이프 | `LIKE 'a\_%' {escape '\'}` | `SQL_LIKE_ESCAPE_CLAUSE` |
| 매개변수 | `?`(위치) — 이름 붙은 매개변수 없음 | `SQLDescribeParam` 지원 여부 드라이버 의존 |
| 식별자 인용 | `SQLGetInfo(SQL_IDENTIFIER_QUOTE_CHAR)` → `"`/`` ` ``/`[`(공백 = 미지원) | 하드코딩 금지 |
| 대소문자 | `SQL_IDENTIFIER_CASE` = UPPER/LOWER/SENSITIVE/MIXED · 인용 시 `SQL_QUOTED_IDENTIFIER_CASE` | |
| 이름 규칙 | 최대 길이 `SQL_MAX_TABLE_NAME_LEN`·`SQL_MAX_COLUMN_NAME_LEN`… · 특수 문자 `SQL_SPECIAL_CHARACTERS` · 카탈로그 구분자 `SQL_CATALOG_NAME_SEPARATOR` · 위치 `SQL_CATALOG_LOCATION` | |
| 이스케이프 해석 끄기 | `SQL_ATTR_NOSCAN = SQL_NOSCAN_ON` | |
| 네이티브 변환 | `SQLNativeSql` = 드라이버가 실제 보낼 SQL 확인 | |
| 문자열·주석·종결 | 백엔드 의존(`SQL_ACCESSIBLE_*` 아님 · ODBC는 단일 문장 단위 · 배치 지원 = `SQL_BATCH_SUPPORT`) | |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/escape-sequences-in-odbc · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/date-time-and-timestamp-literals · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/procedure-call-escape-sequence · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/outer-joins · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/like-escape-sequence · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/quoted-identifiers · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlnativesql-function

### T-3. ODBC 예약 키워드(부록 C · `SQL_ODBC_KEYWORDS`)

- 아래 목록 = **235개**(공식 표의 낱말 수 집계). "코어 SQL 문법 드라이버와 호환을 위해 피하라"는 권고이며 백엔드의 진짜 예약어는 `SQLGetInfo(SQL_KEYWORDS)`(드라이버 고유 · 쉼표 구분)로 받아 합친다.

```
ABSOLUTE ACTION ADA ADD ALL ALLOCATE ALTER AND ANY ARE AS ASC ASSERTION AT AUTHORIZATION AVG
BEGIN BETWEEN BIT BIT_LENGTH BOTH BY CASCADE CASCADED CASE CAST CATALOG CHAR CHAR_LENGTH
CHARACTER CHARACTER_LENGTH CHECK CLOSE COALESCE COLLATE COLLATION COLUMN COMMIT CONNECT
CONNECTION CONSTRAINT CONSTRAINTS CONTINUE CONVERT CORRESPONDING COUNT CREATE CROSS CURRENT
CURRENT_DATE CURRENT_TIME CURRENT_TIMESTAMP CURRENT_USER CURSOR DATE DAY DEALLOCATE DEC DECIMAL
DECLARE DEFAULT DEFERRABLE DEFERRED DELETE DESC DESCRIBE DESCRIPTOR DIAGNOSTICS DISCONNECT
DISTINCT DOMAIN DOUBLE DROP ELSE END END-EXEC ESCAPE EXCEPT EXCEPTION EXEC EXECUTE EXISTS
EXTERNAL EXTRACT FALSE FETCH FIRST FLOAT FOR FOREIGN FORTRAN FOUND FROM FULL GET GLOBAL GO GOTO
GRANT GROUP HAVING HOUR IDENTITY IMMEDIATE IN INCLUDE INDEX INDICATOR INITIALLY INNER INPUT
INSENSITIVE INSERT INT INTEGER INTERSECT INTERVAL INTO IS ISOLATION JOIN KEY LANGUAGE LAST
LEADING LEFT LEVEL LIKE LOCAL LOWER MATCH MAX MIN MINUTE MODULE MONTH NAMES NATIONAL NATURAL
NCHAR NEXT NO NONE NOT NULL NULLIF NUMERIC OCTET_LENGTH OF ON ONLY OPEN OPTION OR ORDER OUTER
OUTPUT OVERLAPS PAD PARTIAL PASCAL POSITION PRECISION PREPARE PRESERVE PRIMARY PRIOR PRIVILEGES
PROCEDURE PUBLIC READ REAL REFERENCES RELATIVE RESTRICT REVOKE RIGHT ROLLBACK ROWS SCHEMA SCROLL
SECOND SECTION SELECT SESSION SESSION_USER SET SIZE SMALLINT SOME SPACE SQL SQLCA SQLCODE
SQLERROR SQLSTATE SQLWARNING SUBSTRING SUM SYSTEM_USER TABLE TEMPORARY THEN TIME TIMESTAMP
TIMEZONE_HOUR TIMEZONE_MINUTE TO TRAILING TRANSACTION TRANSLATE TRANSLATION TRIM TRUE UNION
UNIQUE UNKNOWN UPDATE UPPER USAGE USER USING VALUE VALUES VARCHAR VARYING VIEW WHEN WHENEVER
WHERE WITH WORK WRITE YEAR ZONE
```

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/reserved-keywords · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetinfo-function

### T-4. 데이터 타입(SQL 타입 식별자)

| SQL 타입 식별자 | 대표 SQL 타입 | 비고 |
|---|---|---|
| `SQL_CHAR` / `SQL_VARCHAR` / `SQL_LONGVARCHAR` | CHAR(n) / VARCHAR(n) / LONG VARCHAR | |
| `SQL_WCHAR` / `SQL_WVARCHAR` / `SQL_WLONGVARCHAR` | 유니코드 문자 | 3.5+ |
| `SQL_DECIMAL` / `SQL_NUMERIC` | DECIMAL(p,s) / NUMERIC(p,s) | |
| `SQL_TINYINT` `SQL_SMALLINT` `SQL_INTEGER` `SQL_BIGINT` | 정수 | 부호 여부 = `UNSIGNED_ATTRIBUTE` |
| `SQL_REAL` / `SQL_FLOAT` / `SQL_DOUBLE` | REAL / FLOAT(p) / DOUBLE PRECISION | |
| `SQL_BIT` | BIT(1비트) | SQL-92 BIT과 다름 |
| `SQL_BINARY` / `SQL_VARBINARY` / `SQL_LONGVARBINARY` | 이진 | |
| `SQL_TYPE_DATE` / `SQL_TYPE_TIME` / `SQL_TYPE_TIMESTAMP` | DATE / TIME(p) / TIMESTAMP(p) | 2.x = `SQL_DATE/TIME/TIMESTAMP` |
| `SQL_TYPE_UTCDATETIME` / `SQL_TYPE_UTCTIME` | UTC 시각 | 드물게 지원 |
| `SQL_INTERVAL_*` 13종 | YEAR · MONTH · YEAR_TO_MONTH · DAY · HOUR · MINUTE · SECOND · DAY_TO_HOUR · DAY_TO_MINUTE · DAY_TO_SECOND · HOUR_TO_MINUTE · HOUR_TO_SECOND · MINUTE_TO_SECOND | |
| `SQL_GUID` | GUID | |
| 드라이버 고유 | 음수/고유 번호(예: SQL Server `SQL_SS_XML`·`SQL_SS_TIME2`) | `SQLGetTypeInfo`에만 나타남 |

`SQLGetTypeInfo(SQL_ALL_TYPES)` 결과 열(19) = `TYPE_NAME DATA_TYPE COLUMN_SIZE LITERAL_PREFIX LITERAL_SUFFIX CREATE_PARAMS NULLABLE CASE_SENSITIVE SEARCHABLE UNSIGNED_ATTRIBUTE FIXED_PREC_SCALE AUTO_UNIQUE_VALUE LOCAL_TYPE_NAME MINIMUM_SCALE MAXIMUM_SCALE SQL_DATA_TYPE SQL_DATETIME_SUB NUM_PREC_RADIX INTERVAL_PRECISION` → **백엔드 타입 이름·리터럴 접두·생성 매개변수의 유일한 이식 원천**(완성·DDL 생성에 사용). 실제 타입 집합 = 백엔드 의존.

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/sql-data-types · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgettypeinfo-function · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/interval-data-types · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/c-data-types

### T-5. 연산자 · NULL

| 항목 | 내용 |
|---|---|
| 연산자·우선순위 | 백엔드 의존(부록 C 최소 문법 = `+ - * /` · 비교 · `AND OR NOT` · `LIKE` `IN` `BETWEEN` `IS NULL` `EXISTS`) |
| 연결 | 이식 형태 = `{fn CONCAT(a,b)}`(NULL 처리도 백엔드 의존이라고 명시) · `\|\|`/`+` = 백엔드 의존 · `SQL_CONCAT_NULL_BEHAVIOR` |
| NULL 정렬 | `SQL_NULL_COLLATION` = HIGH/LOW/START/END |
| 콜레이션 | `SQL_COLLATION_SEQ` · 나머지 백엔드 의존 |
| 상관 이름 | `SQL_CORRELATION_NAME`(별칭 지원) · `SQL_GROUP_BY` · `SQL_ORDER_BY_COLUMNS_IN_SELECT` |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/sql-minimum-grammar · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetinfo-function

### T-6. 스칼라 함수 이스케이프 `{fn …}`

| 분류(지원 비트마스크) | 함수 |
|---|---|
| 문자열 (`SQL_STRING_FUNCTIONS`) | `ASCII BIT_LENGTH CHAR CHAR_LENGTH CHARACTER_LENGTH CONCAT DIFFERENCE INSERT LCASE LEFT LENGTH LOCATE OCTET_LENGTH POSITION REPEAT REPLACE RIGHT RTRIM LTRIM SOUNDEX SPACE SUBSTRING UCASE` (1-기반) |
| 수치 (`SQL_NUMERIC_FUNCTIONS`) | `ABS ACOS ASIN ATAN ATAN2 CEILING COS COT DEGREES EXP FLOOR LOG LOG10 MOD PI POWER RADIANS RAND ROUND SIGN SIN SQRT TAN TRUNCATE` |
| 날짜·시간·간격 (`SQL_TIMEDATE_FUNCTIONS`) | `CURRENT_DATE CURRENT_TIME CURRENT_TIMESTAMP CURDATE CURTIME DAYNAME DAYOFMONTH DAYOFWEEK DAYOFYEAR EXTRACT HOUR MINUTE MONTH MONTHNAME NOW QUARTER SECOND TIMESTAMPADD TIMESTAMPDIFF WEEK YEAR` · 간격 키워드 `SQL_TSI_FRAC_SECOND SECOND MINUTE HOUR DAY WEEK MONTH QUARTER YEAR`(`SQL_TIMEDATE_ADD_INTERVALS`/`_DIFF_INTERVALS`) |
| 시스템 (`SQL_SYSTEM_FUNCTIONS`) | `DATABASE() IFNULL(exp, value) USER()` |
| 변환 (`SQL_CONVERT_FUNCTIONS` · `SQL_CONVERT_*` 표) | `CONVERT(value_exp, SQL_타입)` — 대상 = `SQL_BIGINT SQL_BINARY SQL_BIT SQL_CHAR SQL_DATE SQL_DECIMAL SQL_DOUBLE SQL_FLOAT SQL_INTEGER SQL_INTERVAL_* SQL_LONGVARBINARY SQL_LONGVARCHAR SQL_NUMERIC SQL_REAL SQL_SMALLINT SQL_TIME SQL_TIMESTAMP SQL_TINYINT SQL_VARBINARY SQL_VARCHAR SQL_WCHAR SQL_WLONGVARCHAR SQL_WVARCHAR SQL_GUID` |
| 집계·윈도우·JSON 등 | 이스케이프 없음 → 백엔드 의존(집계 5종 `COUNT SUM AVG MIN MAX`만 최소 문법 · `SQL_AGGREGATE_FUNCTIONS`) |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/scalar-functions · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/string-functions · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/numeric-functions · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/time-date-and-interval-functions · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/system-functions · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/explicit-data-type-conversion-function

### T-7. 질의

| 항목 | 내용 |
|---|---|
| 기본 | 최소 문법 = `SELECT [ALL\|DISTINCT] … FROM … [WHERE] [GROUP BY] [HAVING] [UNION] [ORDER BY]` · 외부 조인 = `{oj}` |
| 행 제한 | 이식 문법 없음 → `SQL_ATTR_MAX_ROWS`(문장 속성 · 드라이버가 무시 가능) 또는 페치 중단 |
| 커서 | `SQL_ATTR_CURSOR_TYPE`(FORWARD_ONLY/STATIC/KEYSET/DYNAMIC) · `SQLFetchScroll` · 블록 페치 `SQL_ATTR_ROW_ARRAY_SIZE` |
| 위치 갱신 | `SELECT … FOR UPDATE` + `SQLSetPos`/`WHERE CURRENT OF` · `SQL_POSITIONED_STATEMENTS` |
| CTE·윈도우·LIMIT·힌트·잠금 | 백엔드 의존 · 지원 질의 = `SQL_SQL92_*`(`SQL_SQL92_RELATIONAL_JOIN_OPERATORS` 등) |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/sql-minimum-grammar · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlsetstmtattr-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlfetchscroll-function

### T-8. DML

| 항목 | 내용 |
|---|---|
| 기본 | `INSERT INTO t [(cols)] VALUES (…)` · 검색 UPDATE/DELETE · 위치 UPDATE/DELETE(`WHERE CURRENT OF cursor`) |
| 영향 행 수 | `SQLRowCount`(배열 실행 시 `SQL_PARAM_ARRAY_ROW_COUNTS` 개별/합산) |
| 커서 기반 쓰기 | `SQLSetPos(SQL_UPDATE/SQL_DELETE)` · `SQLBulkOperations(SQL_ADD)` |
| 업서트·MERGE·RETURNING·TRUNCATE·벌크 파일 | 백엔드 의존 |
| 자동 생성 키 | 표준 API 없음 → 백엔드 의존(**추정** · 드라이버 고유 속성 일부) |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/sql-minimum-grammar · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlrowcount-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlbulkoperations-function

### T-9. 객체 · 카탈로그 함수(유일한 이식 메타)

| 함수 | 주는 것 | 주요 결과 열 |
|---|---|---|
| `SQLTables` | 카탈로그·스키마·테이블·뷰 등 목록(`%` 패턴 · 특수 호출로 카탈로그/스키마/타입 목록) | `TABLE_CAT TABLE_SCHEM TABLE_NAME TABLE_TYPE REMARKS` · TABLE_TYPE = `TABLE VIEW SYSTEM TABLE GLOBAL TEMPORARY LOCAL TEMPORARY ALIAS SYNONYM` + 드라이버 고유 |
| `SQLColumns` | 컬럼 | `… COLUMN_NAME DATA_TYPE TYPE_NAME COLUMN_SIZE BUFFER_LENGTH DECIMAL_DIGITS NUM_PREC_RADIX NULLABLE REMARKS COLUMN_DEF SQL_DATA_TYPE SQL_DATETIME_SUB CHAR_OCTET_LENGTH ORDINAL_POSITION IS_NULLABLE` |
| `SQLPrimaryKeys` | PK 컬럼 | `COLUMN_NAME KEY_SEQ PK_NAME` |
| `SQLForeignKeys` | FK(참조·피참조 양방향) | `PKTABLE_* PKCOLUMN_NAME FKTABLE_* FKCOLUMN_NAME KEY_SEQ UPDATE_RULE DELETE_RULE FK_NAME PK_NAME DEFERRABILITY` |
| `SQLStatistics` | 인덱스·테이블 통계 | `NON_UNIQUE INDEX_QUALIFIER INDEX_NAME TYPE ORDINAL_POSITION COLUMN_NAME ASC_OR_DESC CARDINALITY PAGES FILTER_CONDITION` |
| `SQLSpecialColumns` | 행 식별 최적 컬럼(`SQL_BEST_ROWID`) · 자동 갱신 컬럼(`SQL_ROWVER`) | `SCOPE COLUMN_NAME DATA_TYPE … PSEUDO_COLUMN` — 그리드 편집 행 식별에 사용 가능 |
| `SQLProcedures` | 프로시저·함수 | `PROCEDURE_CAT PROCEDURE_SCHEM PROCEDURE_NAME NUM_INPUT_PARAMS NUM_OUTPUT_PARAMS NUM_RESULT_SETS REMARKS PROCEDURE_TYPE`(UNKNOWN/PROCEDURE/FUNCTION) |
| `SQLProcedureColumns` | 루틴 매개변수·결과 열 | `COLUMN_NAME COLUMN_TYPE`(IN/INOUT/OUT/RETURN_VALUE/RESULT_COL) `DATA_TYPE TYPE_NAME … COLUMN_DEF ORDINAL_POSITION` |
| `SQLTablePrivileges` / `SQLColumnPrivileges` | 권한 | `GRANTOR GRANTEE PRIVILEGE IS_GRANTABLE` |
| `SQLGetTypeInfo` | 타입 | T-4 |

| 객체 | ODBC 관점 |
|---|---|
| TABLE · VIEW · SYNONYM | `SQLTables`로 목록 · DDL 원문 = 백엔드 의존(없음) |
| INDEX · 제약 | `SQLStatistics` · `SQLPrimaryKeys` · `SQLForeignKeys` · CHECK/UNIQUE 제약 이름 = 이식 원천 없음 |
| PROCEDURE · FUNCTION | `SQLProcedures`/`SQLProcedureColumns` · 본문 = 백엔드 의존 |
| TRIGGER · SEQUENCE · PACKAGE · EVENT · USER · ROLE | **이식 원천 없음** → 백엔드 의존 |
| 이름 계층 | 카탈로그(`SQL_CATALOG_USAGE`·`SQL_CATALOG_TERM`) · 스키마(`SQL_SCHEMA_USAGE`·`SQL_SCHEMA_TERM`) · 지원 안 하면 열 = NULL · 최대 3부 |
| DDL 지원 여부 | `SQL_CREATE_TABLE` `SQL_CREATE_VIEW` `SQL_DROP_TABLE` `SQL_ALTER_TABLE` 비트마스크(문법 자체는 백엔드 의존) |
| 오버로드 | 백엔드 의존 |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/catalog-functions-in-odbc · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqltables-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlcolumns-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlprimarykeys-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlforeignkeys-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlstatistics-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlspecialcolumns-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlprocedures-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlprocedurecolumns-function

### T-10. 루틴

| 항목 | 내용 |
|---|---|
| 호출 | `{call p(?, ?)}` · `{? = call f(?)}` · 방향 = `SQLBindParameter(InputOutputType = SQL_PARAM_INPUT/OUTPUT/INPUT_OUTPUT · 3.8 `SQL_PARAM_OUTPUT_STREAM`)` |
| 출력 값 | 모든 결과 집합을 `SQLMoreResults`로 다 읽은 **뒤** 출력 매개변수 버퍼가 채워짐(드라이버 의존 · `SQL_PARAM_ARRAY_*`) |
| 지원 여부 | `SQL_PROCEDURES` = "Y"/"N" · `SQL_MAX_PROCEDURE_NAME_LEN` |
| 정의 문법(CREATE PROCEDURE 본문)·기본값·이름 인자 | 백엔드 의존 |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/procedure-calls · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlbindparameter-function · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/procedure-parameters

### T-11. 트랜잭션

| 항목 | 내용 |
|---|---|
| 자동 커밋 | `SQL_ATTR_AUTOCOMMIT`(기본 ON) — 끄면 첫 문장에서 암묵 시작 |
| 종료 | `SQLEndTran(SQL_HANDLE_DBC, hdbc, SQL_COMMIT\|SQL_ROLLBACK)` — SQL 문 `COMMIT` 대신 이 API를 권장 |
| 격리 | `SQL_ATTR_TXN_ISOLATION`(READ_UNCOMMITTED/READ_COMMITTED/REPEATABLE_READ/SERIALIZABLE) · 기본값 = `SQL_DEFAULT_TXN_ISOLATION` · 지원 = `SQL_TXN_ISOLATION_OPTION` |
| DDL 동작 | `SQL_TXN_CAPABLE` = NONE/DML/DDL_COMMIT/DDL_IGNORE/ALL |
| 커서 보존 | `SQL_CURSOR_COMMIT_BEHAVIOR`/`SQL_CURSOR_ROLLBACK_BEHAVIOR` |
| 세이브포인트 · 현재 DB 전환 | 세이브포인트 = 백엔드 의존 · 카탈로그 = `SQL_ATTR_CURRENT_CATALOG` |
| 읽기 전용 | `SQL_ATTR_ACCESS_MODE` |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/commit-mode · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlendtran-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlsetconnectattr-function · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/transaction-isolation-levels

### T-12. DCL

| 항목 | 내용 |
|---|---|
| 문법 | 최소 문법 밖 → 백엔드 의존 · 지원 비트 `SQL_SQL92_GRANT`/`SQL_SQL92_REVOKE` |
| 조회 | `SQLTablePrivileges` · `SQLColumnPrivileges`만 이식 |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqltableprivileges-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetinfo-function

### T-13. 오류 · 진단

| 항목 | 내용 |
|---|---|
| 반환 코드 | `SQL_SUCCESS` · `SQL_SUCCESS_WITH_INFO`(경고 · 진단 레코드 있음) · `SQL_ERROR` · `SQL_NO_DATA` · `SQL_NEED_DATA` · `SQL_STILL_EXECUTING` · `SQL_INVALID_HANDLE` |
| 진단 | `SQLGetDiagRec(핸들종류, 핸들, i, SqlState[6], &NativeError, Msg, …)` 1..n 반복 · `SQLGetDiagField`(`SQL_DIAG_ROW_NUMBER`·`SQL_DIAG_COLUMN_NUMBER`·`SQL_DIAG_NUMBER`) |
| SQLSTATE | 5자 = 클래스 2 + 하위 3(ODBC 3.x = SQL-92 정렬 · 2.x와 다름 → `SQL_ATTR_ODBC_VERSION`) · 예 `08S01` 통신 끊김 · `23000` 제약 위반 · `42000` 구문/접근 · `42S02` 테이블 없음 · `HY008` 취소됨 · `HYT00` 시간 초과 · `01000` 경고 |
| 메시지 접두 | `[벤더][ODBC 구성 요소][데이터 원본] 메시지` — 어느 층의 오류인지 판별 |
| 위치(줄·열) | 백엔드 의존(메시지 본문 파싱) · NativeError = 백엔드 오류 번호 |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdiagrec-function · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/diagnostic-messages · https://learn.microsoft.com/en-us/sql/odbc/reference/appendixes/appendix-a-odbc-error-codes · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/return-codes-odbc

### T-14. 클라이언트 · 실행

| 항목 | 내용 |
|---|---|
| 실행 | `SQLExecDirect` · `SQLPrepare`+`SQLExecute` · 결과 열 정보 `SQLNumResultCols`/`SQLDescribeCol`/`SQLColAttribute` |
| 페치 | `SQLFetch`/`SQLFetchScroll` · 블록 `SQL_ATTR_ROW_ARRAY_SIZE` · 큰 값 `SQLGetData` 조각 읽기(`SQL_GD_ANY_COLUMN` 등 `SQL_GETDATA_EXTENSIONS`) |
| 다중 결과 | `SQLMoreResults` 반복(행 수만 있는 결과 포함) |
| 배열 바인딩(대량) | `SQL_ATTR_PARAMSET_SIZE` = N · `SQL_ATTR_PARAM_BIND_TYPE`(열/행 방향) · `SQL_ATTR_PARAM_STATUS_PTR` · `SQL_ATTR_PARAMS_PROCESSED_PTR` · 지원 = `SQL_PARAM_ARRAY_ROW_COUNTS`/`SQL_PARAM_ARRAY_SELECTS` · 진짜 벌크 경로는 드라이버 고유(예: SQL Server BCP 확장) |
| 큰 값 쓰기 | 실행 시 데이터 `SQL_DATA_AT_EXEC` + `SQLParamData`/`SQLPutData` |
| 취소 | `SQLCancel(hstmt)`(다른 스레드에서 호출 가능 · 결과 `HY008`) · 3.8 = `SQLCancelHandle`(연결 핸들 · 비동기 연결) · 시간 상한 `SQL_ATTR_QUERY_TIMEOUT`(초) · `SQL_ATTR_CONNECTION_TIMEOUT`/`SQL_ATTR_LOGIN_TIMEOUT` |
| 비동기 | `SQL_ATTR_ASYNC_ENABLE`(문장) · 3.8 비동기 연결·이벤트 알림(Windows) |
| 생존 확인 | `SQL_ATTR_CONNECTION_DEAD`(3.5+ · 드라이버가 마지막 상태로 판정 · 왕복 없음) |
| 접속 문자열 | `SQLDriverConnect("DRIVER={…};SERVER=…;UID=…;PWD=…")` · DSN · 키 이름 = 드라이버 의존 |
| Rust crate | `odbc-api`(MIT · 안전 래퍼 · 열 방향 버퍼 `ColumnarBuffer` · 배열 삽입 지원) · `odbc-sys`(FFI) — **nexa-sql 현재 미사용**(2026-10-10 확인) · 플랫폼별 드라이버 관리자 링크 방식(동적 적재 여부)은 **추정**(미확인) |

출처: https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlmoreresults-function · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/arrays-of-parameter-values · https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/binding-arrays-of-parameters · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlcancel-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlcancelhandle-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdata-function · https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqldriverconnect-function · https://docs.rs/odbc-api/latest/odbc_api/

---

### 미확인(추정) 목록

| 절 | 항목 |
|---|---|
| §6 T-1 | 8.0 지원 종료 시점(2026-04) |
| §6 T-3 | 8.4 예약어 총수 260(증감 계산값) |
| §6 T-6 | `WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS` 8.4 제거 여부 |
| §6 T-7 | `ROLLUP()` 함수형 구문 미지원 · 8.4 `QUALIFY`/`TABLESAMPLE` 기능 범위 |
| §6 T-9 | `CREATE/DROP INDEX IF [NOT] EXISTS` 8.0 미지원 |
| §6 T-14 | Rust crate별 LOCAL INFILE·KILL 지원 정도 |
| §7 T-2 | `{call}` 안 이름 인자 |
| §7 T-8 | 자동 생성 키 API 부재 |
| §7 T-14 | `odbc-api` 드라이버 관리자 링크 방식 |

### T-15 nexa-sql 지원 현황

방언 열 **AN**(ANSI/ODBC(일반)) — 항목별 ✅/부분/❌와 코드 자리는 §11 표의 `AN` 열 · 한 줄 판정은 §11 끝 "요약" · 결함은 §9.

## §8. 교차 비교표 — 차이가 코드 분기인 항목

§2~§7의 사실 중 **코드가 방언마다 달리 처리해야 하는 것**만 모았다. 칸 = 그 DBMS의 사실 · 맨 오른쪽 = nexa-sql이 지금 가르는 자리(없으면 결함 후보 → §9). 근거와 출처는 각 장의 같은 T 번호 표에 있다.

### §8-1 어휘(T-2)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL | ANSI(109 §1) | nexa-sql 분기 자리 |
|---|---|---|---|---|---|---|---|
| 인용 식별자 | `"…"` | `[…]` · `"…"`(QUOTED_IDENTIFIER ON) | `"…"` · `U&"…"` | `"…"` · `[…]` · `` `…` `` | `` `…` `` · `"…"`(ANSI_QUOTES) | `"…"` · `U&"…"` | `lexer.rs`(방언 무관 1벌 · 백틱 없음 = §9 #3) · `identq.rs` 인용 문자 |
| 비인용 이름 접힘 | 대문자 | 보존(비교 = 콜레이션) | 소문자 | 보존(ASCII 대소문자 무시) | 보존(테이블 이름 = `lower_case_table_names` · OS) | 대문자 | `identq.rs` needed 판정(ORA 소문자 · PG 대문자 위반) |
| 식별자 최대 | 128바이트(12.2+) | 128자(로컬 임시 표 116) | 63바이트 | 제한 없음(추정) | 64자 | 구현 정의 | 없음(검사 안 함) |
| 문자열 이스케이프 | `''` · `q'[…]'` | `''` | `''` · `E'\…'` · `$$…$$`/`$tag$` | `''` | `''` · **`\'`**(NO_BACKSLASH_ESCAPES 아니면) · `"…"` 문자열 | `''` | `lexer.rs` — `E''`·MySQL `\` 미처리 = §9 #1 |
| 국가 문자열 | `N'…'` | `N'…'` | (`N'…'` = text로 읽음) | 없음 | `N'…'` · `_charset'…'` | `N'…'` | `lexer.rs` `N'` |
| 줄 주석 | `--` | `--` | `--` | `--` | `-- `(공백 필수) · **`#`** | `--` | `#` 미인식 = §9 #2 |
| 블록 주석 중첩 | 안 됨 | **됨** | **됨** | 안 됨 | 안 됨(`/*! */` 버전 주석) | 됨(bracketed comment) | 중첩 미처리 = §9 #5 |
| 문장/배치 구분 | `;` · PL/SQL 블록 뒤 `/` | `;` · 배치 `GO [n]` | `;`(본문은 `$$`) | `;` | `;` · 클라이언트 `DELIMITER` | `;` | `split.rs` — `GO n`·`DELIMITER` 없음 = §9 #4·#12 |
| 바인드 | `:name` · `:1` | `@name`(드라이버 `@P1`) | `$1` | `?` `?N` `:a` `@a` `$a` | `?` | `?` · `:name` | `bind.rs` |
| 대체 변수 | `&v` `&&v`(SQL*Plus) | `$(v)`(sqlcmd) | `:v`(psql) | 없음 | 없음 | 없음 | `engine.rs` `&` = 방언 무관 기본 켬 = §9 #19 |
| 빈 문자열 | **NULL과 같음** | 값 | 값 | 값 | 값 | 값 | 그리드·필터 NULL 판정 |
| 불리언 리터럴 | SQL에서 없음(PL/SQL만 · 23ai SQL) | 없음(`bit`) | `TRUE`/`FALSE` | `TRUE`/`FALSE`(3.23+ · 정수) | `TRUE`/`FALSE`(= 1/0) | `TRUE`/`FALSE`/`UNKNOWN` | — |

### §8-2 예약어·자료형·연산(T-3~T-5)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL | ANSI | nexa-sql |
|---|---|---|---|---|---|---|---|
| 예약어 수(조사 판) | SQL 110 · PL/SQL 85 | 185 | 예약 78 + 함수·형 가능 23 + 비예약(함수·형 불가) 55 | 키워드 147(대부분 문맥 예약) | 예약 260(8.0) | 예약 398 / 비예약 297(2016) · 409/298(2023) | 강조 196 · 인용 판정 117(포맷터 표 재사용) = §9 #7·#8 |
| 예약/비예약 구분 | 있음(V$RESERVED_WORDS) | 예약만 공표 | 4분류 | fallback 규칙 | R 표시 | 2분류 | **없음** |
| 문자열 연결 | `\|\|` | `+` · `CONCAT` · `\|\|`(2025) | `\|\|` | `\|\|` | `CONCAT`(`\|\|` = OR · PIPES_AS_CONCAT) | `\|\|` | 포맷터·완성 무관 |
| 오름차순 NULL 위치 | NULLS LAST | NULLS FIRST | NULLS LAST | NULLS FIRST | NULLS FIRST | 구현 정의 | 그리드 클라이언트 정렬 = 자체 규칙 |
| 정수 나눗셈 | 실수 | 정수 | 정수 | 정수 | 실수(`DIV` = 정수) | 구현 정의 | — |
| 대표 날짜형 | `DATE`(시각 포함) · `TIMESTAMP` | `datetime2` · `date` | `timestamp[tz]` · `date` | 없음(TEXT/REAL/INTEGER) | `DATETIME` · `TIMESTAMP` | `DATE`/`TIME`/`TIMESTAMP` | `builtins::types` 방언 표 |

### §8-3 질의·DML(T-7·T-8)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL | ANSI | nexa-sql(`.sqlg`) |
|---|---|---|---|---|---|---|---|
| 행 제한 | `FETCH FIRST`(12c) · `ROWNUM` | `TOP (n)` · `OFFSET … FETCH` | `LIMIT/OFFSET` · `FETCH FIRST` | `LIMIT/OFFSET` | `LIMIT [off,] n` | `OFFSET … FETCH FIRST` | ansi 상속 → ORA·MS에 `LIMIT` 후보 = §9 #9 · 페치 페이지 = `Caps` |
| 계층 질의 | `CONNECT BY` · 재귀 WITH | 재귀 CTE | `WITH RECURSIVE` | `WITH RECURSIVE` | `WITH RECURSIVE`(8.0) | `WITH RECURSIVE` + SEARCH/CYCLE | oracle.sqlg |
| 집합 차 | `MINUS`(21c `EXCEPT`) | `EXCEPT` | `EXCEPT` | `EXCEPT` | `EXCEPT`(8.0.31) | `EXCEPT` | — |
| 피벗 | `PIVOT/UNPIVOT` | `PIVOT/UNPIVOT` | 없음(crosstab 확장) | 없음 | 없음 | 없음 | — |
| 측면 조인 | `LATERAL` · `CROSS/OUTER APPLY` | `CROSS/OUTER APPLY` | `LATERAL` | 없음 | `LATERAL`(8.0.14) | `LATERAL` | 미정의 = §9 #13 |
| 갱신 결과 받기 | `RETURNING … INTO` | `OUTPUT` | `RETURNING` | `RETURNING`(3.35) | 없음 | 없음(데이터 변경 델타 표 = 추정) | `Caps.returning` 후보 |
| 업서트 | `MERGE` | `MERGE` | `ON CONFLICT` · `MERGE`(15) | `ON CONFLICT`(3.24) · `INSERT OR REPLACE` | `ON DUPLICATE KEY UPDATE` · `REPLACE` | `MERGE` | gen.rs MY = `VALUES(c)` 폐기 예고 꼴 = §9 #26 |
| 조인 갱신 | 갱신 가능 조인 뷰 · 상관 하위 질의 | `UPDATE … FROM` | `UPDATE … FROM` · `DELETE … USING` | `UPDATE … FROM`(3.33) | 다중 표 `UPDATE`/`DELETE` | 없음 | — |
| 다중 행 VALUES | `INSERT ALL`(23ai 전) | 됨 | 됨 | 됨 | 됨 | 됨 | oracle.sqlg `INSERT ALL` 없음 = §9 #11 |
| 행 잠금 | `FOR UPDATE [OF] [NOWAIT\|WAIT n\|SKIP LOCKED]` | 힌트 `UPDLOCK` | `FOR UPDATE/NO KEY UPDATE/SHARE/KEY SHARE` | 없음 | `FOR UPDATE/SHARE` | 커서 `FOR UPDATE` | — |

### §8-4 객체·루틴(T-9·T-10)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL | ANSI | nexa-sql |
|---|---|---|---|---|---|---|---|
| 이름 계층 | 스키마.객체[@링크] | 서버.DB.스키마.객체 | DB.스키마.객체(DB 넘나듦 불가) | 붙인 DB.객체 | DB.객체(DB = 스키마) | 카탈로그.스키마.객체 | `split_db_key` · 3부 이름 완성 |
| `OR REPLACE` | 뷰·루틴·패키지·트리거·타입·시노님 | `CREATE OR ALTER`(2016 SP1) | 뷰·함수·프로시저·트리거·규칙 | 없음 | 뷰만 | 없음 | gen.rs 방언 분기 |
| `IF [NOT] EXISTS` | 23ai(19.28 백포트 = 추정) | `DROP … IF EXISTS`(2016) | 대부분 | 대부분 | 대부분 | 없음 | 삭제 문장 생성 |
| **오버로드** | 패키지 멤버 · 식별 = `ALL_ARGUMENTS.OVERLOAD` + `SUBPROGRAM_ID` | 없음(번호 프로시저 `;n` = 폐기 예고) | 함수·프로시저·집계·연산자 · 식별 = oid / 인자 형 서명 | 없음 | 없음 | 있음(특정 이름 SPECIFIC) | ORA `PackageMember{overload}`(10-10) · PG oid · 실행 시 선택 = 첫 묶음 = §9 #17 |
| 이름 지정 인자 | `p => v` | `@p = v` | `p => v` · `p := v` | 해당 없음 | 없음 | `p => v` | 완성 `ArgState`(T-316/317) |
| 블록·본문 끝 | `/` | 배치 끝(`GO`) · CREATE는 배치 첫 문장 | `$$…$$` · `BEGIN ATOMIC`(14) | 트리거 `END;` | `DELIMITER` 필요 | `BEGIN ATOMIC … END` | `split.rs` |
| 호출 | `EXEC`/`CALL`/익명 블록 · `SELECT f() FROM DUAL` | `EXEC` · `SELECT dbo.f()` | `CALL` · `SELECT f()` | 해당 없음 | `CALL` · `SELECT f()` | `CALL` | gen.rs `member_call`/`call_one` |
| 소스 보기 | `DBMS_METADATA.GET_DDL` · `ALL_SOURCE` | `OBJECT_DEFINITION` · `sys.sql_modules` | `pg_get_functiondef`/`viewdef`/`indexdef`/`triggerdef` | `sqlite_schema.sql` | `SHOW CREATE …` | 정보 스키마 | nsql-catalog 소스 열기 |
| 상태 | VALID/INVALID · ENABLED | `is_disabled` | 없음(트리거 enabled) | 없음 | 없음 | 없음 | 유효성 배지 |

### §8-5 트랜잭션·오류·클라이언트(T-11·T-13·T-14)

| 항목 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL | nexa-sql |
|---|---|---|---|---|---|---|
| 서버 자동 커밋 | 없음(클라이언트가 결정) | 켬 | 켬(클라이언트) | 켬 | 켬(`autocommit=1`) | `manual_begin_sql` |
| DDL 트랜잭션 | 암시 커밋 | 트랜잭션 안 | 트랜잭션 안 | 트랜잭션 안 | 암시 커밋 | 수동 커밋 경고 |
| 스키마/DB 전환 | `ALTER SESSION SET CURRENT_SCHEMA` | `USE db` | `SET search_path` | `ATTACH` | `USE db` | `context_switch_name` · PG `SET SESSION/LOCAL`·`SET SCHEMA` 미인식 = §9 #24 |
| 오류 형식 | `ORA-nnnnn` · `PLS-` · 줄/열 | 번호·심각도·상태·줄 | SQLSTATE · 문자 위치 · 힌트 | 결과 코드 · 오프셋(3.38 API) | 번호 + SQLSTATE · "near … at line n" | MS·SQLite 위치 미전달 = §9 #23 |
| 실행 취소 | `OCIBreak` | TDS Attention | CancelRequest | `sqlite3_interrupt` | `KILL QUERY` | `CancelHandle` |
| 대량 적재 | 배열 DML | TDS bulk · `BULK INSERT` | `COPY` | 트랜잭션 묶음 | `LOAD DATA LOCAL` | `Caps.bulk_load` · MY 드라이버 없음 |

## §9. 조사로 드러난 결함·누락 → TODO

§11(코드 감사 · 정적 분석)의 결함 후보 27건을 원인별로 묶어 TODO에 올린다. 번호 `#n` = §11 "결함·누락 후보" 표의 번호. **정적 분석이라 고치기 전에 재현을 먼저 한다**(특히 #15 · #14).

| TODO | 묶음 | 포함(#) | 우선 | 첫 재현 방법 |
|---|---|---|---|---|
| **T-324** | 어휘 층 방언화 — 백슬래시 이스케이프 · `#` 주석 · 백틱 · 중첩 블록 주석 · PG `[]` 첨자 · `DELIMITER` · `GO n` · 포맷터 `$$`/`#` | 1 · 2 · 3 · 4 · 5 · 6 · 12(`GO n`) · 27 | P1 | `nsql plan` 방언별 예문(§2~§7 T-2 표 예문) — 문장 수·문자열 경계 비교 |
| **T-325** | 예약어·인용 판정·강조 = DBMS 사실 표 기반(예약/비예약 구분) | 7 · 8 · 20 · 21 | P1 | 열 머리 DnD로 `LEVEL`/`KEY`/`COLUMN` 열 → 조건 바 실행 = 구문 오류 재현 |
| **T-326** | `.sqlg` 문법 빼기 연산 + 방언별 누락 절(ORA `INSERT ALL`·`MATCH_RECOGNIZE`·flashback · MS `FOR XML/JSON`·`WAITFOR` · PG `CALL`·`DISTINCT ON`·`COMMENT ON` · ANSI `CALL`·`START TRANSACTION`·`LATERAL`·`GROUPING SETS`) | 9 · 10 · 11 · 12 · 13 | P2 | 완성 기동 명령 `intel.probe`로 ORA `SELECT … ` 뒤 `LIMIT` 후보 존재 확인 |
| **T-327** | 실행·카탈로그 방언 결함 — PG 루틴 Arguments 0건 의심 · 서버 `SHOW` 가로챔 · `&` 치환 기본값 · 오버로드 선택 · ORA 2조각 이름 · PG 문맥 전환 · 오류 위치 · MY `VALUES()` · 낡은 주석 | 14 · 15 · 16 · 17 · 18 · 19 · 23 · 24 · 25 · 26 | P1(#15·#14·#19) / P2 | #15 = Repository(PG) 함수 펼침 → Arguments 수 · #14 = PG `SHOW search_path` 실행 · #19 = MS 탭 `SELECT 'R&D'` |
| (기록만) | MySQL·ODBC 드라이버 없음 | 22 | — | M4 범위(드라이버 결정 뒤) |

## §10. 새 DBMS 추가 절차(체크리스트)

새 DBMS를 지원 목록에 넣을 때는 **코드보다 이 문서가 먼저**다.

1. **§1 템플릿 복사** → 새 장 `§N. <DBMS> <기준 버전>`을 T-1~T-15 그대로 채운다(공식 문서 원천 · 출처 URL · "추정" 표시). T-15는 처음에 전부 ❌로 시작한다.
2. **§8 교차 비교표**에 새 열을 더한다 — 기존 DBMS와 다른 항목(인용 문자 · 대소문자 접힘 · 구분자 · 바인드 · 행 제한 · 오버로드 키 · 자동 커밋 · 스키마 전환)이 곧 코드 분기다.
3. **어휘·실행 층** — `crates/nsql-script/src/dialect.rs`(방언 열거 · 능력표 `Caps`) → `lexer.rs`(인용·리터럴·주석) → `split.rs`(문장 구분자·블록) → `bind.rs`(바인드 표기) · 시험 = 그 DBMS의 T-2 예문 전부.
4. **드라이버** — `crates/nsql-driver-<dbms>`(DR-3 = 외부 crate는 어댑터 안에 격리 · 원장 기록) · T-14 표(바인드 타입 · LOB · 취소 · 대량 적재) · `Session` 포트 구현.
5. **카탈로그·탐색기** — `crates/nsql-catalog`(`tree.rs` 종류 표 · `detail.rs` · `gen.rs` 소스 생성 = [100 §5](100-object-source-run-and-output-tab.md) 보수적 생성 원칙) · T-9 표의 카탈로그 뷰 · 오버로드 식별 키.
6. **완성** — `crates/nsql-script/grammar/<dbms>.sqlg`(T-7·T-8·T-9 절 순서 · [82](82-grammar-driven-completion.md)) · `builtins.rs`(T-3 예약어 · T-4 자료형 · T-6 함수 시그니처 · 시스템 패키지).
7. **하이라이터·포맷터** — 키워드 집합(T-3) · 블록 끝(T-10) · `crates/nsql-format` 방언 분기.
8. **기능 점검** — T-15 열을 점검표로 써서 실서버(또는 컨테이너) E2E를 만든다 · 결과를 T-15에 ✅로 바꾼다 · 남은 ❌ = TODO.
9. **문서** — 이 문서의 새 장 · §8 · §9 · [109 §15](109-ansi-sql-syntax-survey.md) 대조 열 · CLAUDE.md 지원 DBMS 목록.

## §11. nexa-sql 지원 현황(T-15 · 전 방언) — 코드 감사

> 2026-10-10 · 읽기 전용 정적 분석(grep/read · cargo 실행 없음) · 기준 = HEAD 3e739d7 + 10-10 미커밋 변경. 줄 번호는 그 시점 기준이다. 방언 약칭 = ORA · MS · PG · LT(SQLite) · MY · AN(ANSI/ODBC).


> 범위 = `nexa-sql` 작업 트리(커밋 3e739d7 + 미커밋 변경 포함)와 `../nexa-ui` 를 **grep/read로만** 본 결과. 빌드·시험은 돌리지 않았다(정적 판정).
> 표기: ✅ 지원 · 부분 = 일부만/방언 무관 공통 처리 · ❌ 없음. 경로는 저장소 루트 기준(`crates/…`), nexa-ui는 `nexa-ui/crates/…`.
> 열 머리 약어: **ORA** Oracle · **MS** SQL Server · **PG** PostgreSQL · **LT** SQLite · **MY** MySQL · **AN** ODBC/ANSI(일반).

### 0. 한눈에 — 어디서 방언을 가르는가

| 층 | 방언 분기 여부 | 원천 |
|---|---|---|
| 어휘 분석(문자열·주석·인용) | **방언 무관 1벌** | `crates/nsql-script/src/lexer.rs:22-80` |
| 문장 분리 | 방언 일부 분기(블록 끝 규칙만) | `crates/nsql-script/src/split.rs:80-88, 120, 288, 403-432` |
| 바인드 표기 | 능력표 `Caps.marker` | `crates/nsql-core/src/caps.rs:15-24, 102-186` |
| 구문 강조 키워드 | **방언 무관 1벌**(196 낱말) | `nexa-ui/crates/nexa-ctl/src/highlight.rs:430-448` |
| 식별자 인용 판단의 예약어 | **방언 무관 1벌**(117 낱말 · 포맷터 표 재사용) | `crates/nexa-sql/src/identq.rs:41-62` → `crates/nsql-format/src/lib.rs:437, 562` |
| 완성 키워드 | 공통 94 + 방언 표 | `crates/nsql-script/src/intel.rs:674, 772-884` |
| 완성 문법(절 → 다음 낱말) | `.sqlg` 6개(ansi 위에 덧입힘 · **빼기 연산 없음**) | `crates/nsql-script/grammar/*.sqlg` · `grammar.rs:101-146, 288-297` |
| 내장 함수·자료형·정렬·사전 객체 | 방언 표 | `crates/nsql-script/src/builtins.rs` |
| 카탈로그·트리·소스·생성 | 방언 `match` | `crates/nsql-catalog/src/{lib,tree,gen}.rs` |
| 드라이버 | ORA·MS·PG·LT 내장 / **MY·ODBC 없음** | `crates/nsql-drivers/src/lib.rs:44-66, 178-192` |

---

### T-2 어휘

#### T-2-1 인용·식별자

| 항목 | ORA | MS | PG | LT | MY | AN | 근거 |
|---|---|---|---|---|---|---|---|
| `"…"` 인용 식별자 인식(분석기) | ✅ | ✅ | ✅ | ✅ | 부분(ANSI_QUOTES일 때만 의미) | ✅ | `lexer.rs:63-71` |
| `[…]` 인용 식별자 | (해당 없음) | ✅ | ⚠ 배열 첨자 `a[1]`도 Ident로 분류 | ✅(SQLite도 허용) | (해당 없음) | — | `lexer.rs:72-80`(방언 무관) |
| `` `…` `` 백틱 인식(분석기) | — | — | — | ❌ | **❌** | — | `lexer.rs` 에 분기 없음(포맷터만 `nsql-format/src/lexer.rs:146`) |
| 인용 생성(`quote_ident`) | `"` | `[ ]` | `"` | `"` | `` ` `` | `"` | `nsql-catalog/src/lib.rs:721-728` |
| 대소문자 접힘 가정(`Caps.ident_fold`) | Upper ✅ | Keep ✅ | Lower ✅ | Keep(부분 · 실제는 대소문자 무시) | Keep(부분 · `lower_case_table_names` 의존) | Keep | `caps.rs:115,132,151,155-186` |
| 인용 필요 판정(접힘 위반) | 소문자 포함 → 인용 ✅ | 없음 | 대문자 포함 → 인용 ✅ | 없음 | 없음 | 없음 | `identq.rs:55-61` |
| 식별자 문자 `$` `#` | ✅ | `#temp` ✅ | `$` 부분 | — | `$` 부분 | — | `lexer.rs:211-213`(`is_ident_char` 방언 무관) |

#### T-2-2 문자열 리터럴

| 형식 | ORA | MS | PG | LT | MY | 근거 |
|---|---|---|---|---|---|---|
| `'…'` + `''` 이스케이프 | ✅ | ✅ | ✅ | ✅ | ✅ | `lexer.rs:44-48, 96-111` |
| Oracle `q'[…]'` `q'{…}'` `q'(…)'` `q'<…>'` `q'X…X'` | ✅ | — | — | — | — | `lexer.rs:49-53, 114-133` (`nq'…'` 는 n 뒤 q 라 `prev_is_ident` 로 **미인식** → 부분) |
| `N'…'` | ✅(N=코드, 이어 문자열 — 결과적으로 정상) | ✅ | — | — | ✅ | 별도 분기 없음(자연 처리) |
| PG `E'…\'…'` 백슬래시 이스케이프 | — | — | **❌**(`\'` 에서 문자열 종료로 오판) | — | — | `scan_single_quoted` 에 `\` 처리 없음 `lexer.rs:96-111` |
| MySQL 기본 백슬래시 이스케이프 `'it\'s'` | — | — | — | — | **❌** | 같음 |
| PG 달러 인용 `$$…$$` `$tag$…$tag$` | — | — | ✅ | — | — | `lexer.rs:55-62, 189-206` · 본문 안 완성 `dollar_body_at` `lexer.rs:142-185` |
| `B'…'` `X'…'` | 자연 처리 | 자연 처리 | 자연 처리 | 자연 처리 | 자연 처리 | — |

#### T-2-3 주석

| 형식 | ORA | MS | PG | LT | MY | 근거 |
|---|---|---|---|---|---|---|
| `-- …` | ✅ | ✅ | ✅ | ✅ | ✅(MySQL은 `-- ` 공백 필수 · 구분 안 함) | `lexer.rs:30-34` |
| `/* … */` | ✅ | ✅ | ✅ | ✅ | ✅ | `lexer.rs:35-43` |
| 중첩 `/* /* */ */` (PG·MS 허용) | — | **❌** | **❌** | — | — | 첫 `*/` 에서 끝남 `lexer.rs:36-40` |
| `#` 줄 주석 (MySQL) | — | — | — | — | **❌** | 분기 없음 · 강조기도 `--` 만(`highlight.rs:434`) |
| MySQL `/*! … */` 조건 주석 | — | — | — | — | 부분(주석으로만 봄 · 실행 의미 무시) | — |
| SQL*Plus `REM` | ✅(명령) | — | — | — | — | `command.rs:329` |

#### T-2-4 문장 분리

| 규칙 | ORA | MS | PG | LT | MY | AN | 근거 |
|---|---|---|---|---|---|---|---|
| 코드 영역 `;` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | `split.rs:282-300, 364-` |
| 단독 `/` 줄 = PL/SQL 블록 끝 | ✅ | (무해 · 인식) | (무해) | (무해) | (무해) | ✅ | `split.rs:120, 387-394` |
| 블록 시작 판정(`BEGIN`·`DECLARE`·`CREATE [OR REPLACE|OR ALTER] PROCEDURE|PROC|FUNCTION|PACKAGE|TRIGGER|TYPE|LIBRARY`) | ✅ | ✅(`PROC`·`OR ALTER`) | 해당 없음(`$$`) | 해당 없음 | 해당 없음 | ✅ | `split.rs:403-432` |
| `BEGIN … END;` 짝 세기(`/` 없는 방언) | — | — | ✅ | ✅ | ✅ | — | `split.rs:80-88, 288` |
| `BEGIN;` / `BEGIN TRANSACTION…` = 트랜잭션 문장 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | `split.rs:80-82` |
| 단독 `GO` 줄 = 배치 끝 | (방언 무관 인식) | ✅ | — | — | — | — | `split.rs:120, 394` |
| `GO n`(반복 횟수) | — | **❌**(명령 `GO`로 넘어감) | — | — | — | — | `split.rs:120` 은 `rest == "GO"` 만 |
| MySQL `DELIMITER //` | — | — | — | — | **❌**(분리기에 없음 · 완성 키워드에만 `intel.rs:861`) | — | `split.rs` 전체에 `DELIMITER` 없음 |
| T-SQL `;` 없는 연속 문장 | — | 부분(다음 `;`/`GO`까지 한 배치로 전송 — 서버는 실행 가능, 문장 단위 결과·오류 위치는 뭉침) | — | — | — | — | `split.rs:282-300` |
| 줄 머리 SQL*Plus/sqlcmd 명령(`VAR` `PRINT` `EXEC` `SET` `DEFINE` `@file` `:setvar` `:r` `:connect` …) | ✅ | 부분(sqlcmd `:setvar` `:r` `:connect` `GO`) | 부분 | 부분 | 부분 | — | `command.rs:145-195` |

#### T-2-5 바인드·치환

| 항목 | ORA | MS | PG | LT | MY | AN | 근거 |
|---|---|---|---|---|---|---|---|
| 작성 표기 `:NAME`(엔진 공통) → 전송 표기 | `:NAME` ✅ | `@NAME` ✅(+`DECLARE` 프리펜드) | `$1..` ✅ | `?` ✅ | `?` ✅(드라이버 없음) | `?` ✅ | `caps.rs:15-24` · `dialect.rs:98-188` |
| `::` 캐스트·`:=` 대입 제외 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | `bind.rs:1-2, 36-45` |
| 배열 조각 `a[1:3]` 의 `:` 제외 | — | — | ✅ | — | — | — | `bind.rs:25-34`(실제로는 `[ ]` 가 Ident 로 분류돼 미도달) |
| `&name` 치환(SQL*Plus DEFINE) | ✅ | ⚠ 방언 무관 기본 켬 | ⚠ | ⚠ | ⚠ | ⚠ | `engine.rs:127, 754-794`(문자열 안도 치환 · 미정의 → 입력 요청) |
| `${이름[:형식]}` · `${env:…}` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | `engine.rs:797-` |
| T-SQL `@P1` 위치 매개변수 충돌 회피 | — | ✅ | — | — | — | — | `nsql-driver-mssql/src/lib.rs:685-691` |

---

### T-3 예약어

| 원천 | 크기 | 방언 구분 | 예약/비예약 구분 | 근거 |
|---|---|---|---|---|
| 구문 강조(SQL 하나) | 197 줄 낱말(고유 196) — 키워드 + 함수 + 자료형 혼합 | **없음**(ORA·MS·PG 공통 1벌) | ❌ | `nexa-ui/crates/nexa-ctl/src/highlight.rs:430-448` · nexa-sql은 `SyntaxSpec::sql()` 하나를 등록 `crates/nexa-sql/src/syntax.rs:20` |
| 식별자 인용 판정(`needs_quote`) | 119 줄(고유 117) — 포맷터 절 키워드 | **없음** | ❌ | `identq.rs:52` → `nsql-format/src/lib.rs:437-565` |
| 완성 키워드 공통 | 94 | — | ❌ | `intel.rs:674` |
| 완성 키워드 ORA / MS / PG / MY / LT | 14 / 15 / 12 / 50 / 9 | ✅(공통 뒤에 덧붙임) | ❌ | `intel.rs:772, 788, 805, 820, 873` · 합성 `keywords_for` `intel.rs:885-899` |
| 조건 바 연산 낱말 | 소수 | 없음 | — | `crates/nexa-sql/src/condbar.rs:20` |
| 출처 | 수작업 표(공식 예약어 목록·`V$RESERVED_WORDS`·`pg_get_keywords()`·SQLite `sqlite3_keyword_*` 미사용) | | | |

판정: **ORA 부분 · MS 부분 · PG 부분 · LT 부분 · MY 부분 · AN 부분** — 서버 사전에서 예약어를 읽거나 방언별 예약어 표를 두는 길이 없다.

---

### T-4 자료형(`builtins::types` · 방언 표가 앞, ANSI 공통 15개 중 방언에 없는 이름만 뒤에 · 중복 제거 `builtins.rs:1153-1176`)

| | ORA | MS | PG | LT | MY | AN |
|---|---|---|---|---|---|---|
| 방언 표 항목 수 | 37(`BOOLEAN` 2회 — 합성 때 첫 것만 남음) | 37 | 59 | 8 | 32 | 0(공통 15만) |
| 위치 | `builtins.rs:961` | `:1065` | `:1003` | `:1140` | `:1105` | `:943` |
| 인자 꼴(`sig`) | `NUMBER(p, s)` · `VARCHAR2(n [CHAR\|BYTE])` · `TIMESTAMP[(n)] WITH [LOCAL] TIME ZONE` · `INTERVAL … TO …` ✅ | `varchar(n \| max)` · `nvarchar(n \| max)` ✅ | `varchar(n)` · `timestamp[(p)] [without time zone]` ✅ | 친화 표시(`VARCHAR(n) → TEXT`) ✅ | `TIMESTAMP[(fsp)]` ✅ | 기본 |
| 사용자 정의 타입 | 메타 `ObjectKind::Type` 와 합침 ✅ | ✅ | ✅ | — | — | — |
| 빠진 것(예) | `VECTOR`(23ai) · `MLSLABEL` · `VARRAY`/`TABLE OF` 꼴 · `%TYPE`/`%ROWTYPE`는 `Want::TypeAttr`로 따로 ✅ | `json`/`vector`(2025) · `DOUBLE PRECISION` 는 공통으로 들어옴 | 배열 `int[]` · `bit`/`varbit` · `jsonpath` · `ltree`(확장) | `ANY`(STRICT) | `MULTIPOINT`/`LINESTRING`/`POLYGON` · `SERIAL` · `NATIONAL` 꼴 | — |

판정: ORA ✅(주요) · MS ✅ · PG ✅ · LT ✅ · MY ✅(드라이버 없음) · AN 부분.

---

### T-6 함수

| | ORA | MS | PG | LT | MY | AN |
|---|---|---|---|---|---|---|
| 방언 함수 항목(`b!`) | 43 | 40 | 38 | 25 | 27 | 0 |
| + ANSI 공통 | 39 | 39 | 39 | 39 | 39 | 39 |
| 시그니처(`sig`) | ✅ 전 항목 | ✅ | ✅ | ✅ | ✅ | ✅ |
| 시스템 패키지 | **22 패키지 · 멤버 81**(`DBMS_OUTPUT` … `DBMS_SPACE`) ✅ | — | — | — | — | — |
| 시스템 프로시저 | — | 이름 32(`MSSQL_SYSTEM_PROCS`) + **@파라미터 표 30**(`MSSQL_SYSTEM_PROC_PARAMS`) ✅ | — | — | — | — |
| 사전 객체(관계 자리 후보) | 49 | 26 | 26 | 7 | 11 | 0 |
| 정렬(COLLATE) | 11 | 24 | 13 | 3 | 14 | 0 |
| 근거 | `builtins.rs:73, 286, 520` | `:178, 601, 754, 816, 1204` | `:131, 572, 1267` | `:257, 644, 1323` | `:224, 630, 1283` | `:31, 663, 1160` |
| 오버로드된 내장 함수 | 시그니처 1줄(대표 꼴) — 부분 | 부분 | 부분 | 부분 | 부분 | — |
| 사용자 함수 시그니처 | 서버 `ALL_ARGUMENTS`(오버로드 번호) ✅ | `sys.parameters` ✅ | `pg_proc`(oid) ✅ | ❌ | ❌(`routine_args` 빈 목록 `lib.rs:3285-3287`) | ❌ |

표 안의 범주 혼선: ORA 표에 의사열 `ROWNUM`·`ROWID`, 패키지 멤버 `DBMS_LOB.GETLENGTH`, 프로시저 `RAISE_APPLICATION_ERROR` 가 함수로 섞여 있다 · MS 표에 문장 `RAISERROR`·`THROW` 가 함수로 들어 있다(`builtins.rs:73-130, 178-223`).

---

### T-7 / T-8 질의·DML — `.sqlg` 문법 범위

`ansi.sqlg`(문장 5 · 절 40)가 바탕이고 방언 파일은 `+=`(덧붙임)·`=`(대체)만 쓴다. **빼는 연산이 없어** ansi의 비표준·타 방언 항목이 모든 방언에 상속된다(`grammar.rs:101-146`). ODBC = ansi 그대로(`grammar.rs:288-297`).

| 구문 | ORA | MS | PG | LT | MY | AN(ansi.sqlg) |
|---|---|---|---|---|---|---|
| SELECT 절(WITH·FROM·JOIN·WHERE·GROUP BY·HAVING·ORDER BY·집합) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ `ansi.sqlg:11-12, 20-79` |
| `MINUS` | ✅ `oracle.sqlg:9, 37` | — | — | — | — | — |
| `CONNECT BY` · `START WITH` · `PRIOR` · `NOCYCLE` · `ORDER SIBLINGS BY` | ✅ `oracle.sqlg:9-35` | — | — | — | — | — |
| `PIVOT` / `UNPIVOT` | ✅ `oracle.sqlg:9, 14` | ✅ `mssql.sqlg:14` | — | — | — | ❌ |
| `MODEL` | 부분(문장 절 이름만 · `[clause MODEL]` 없음) `oracle.sqlg:9` | — | — | — | — | — |
| `MATCH_RECOGNIZE` · `FLASHBACK`/`AS OF` · `INSERT ALL/FIRST` · `WITH FUNCTION` | ❌ | — | — | — | — | — |
| `TOP` · `CROSS/OUTER APPLY` · `OPTION` · `WITH (NOLOCK)` · `TABLESAMPLE` | — | ✅ `mssql.sqlg:9-25` | — | — | — | — |
| `FOR XML` / `FOR JSON` · `OPENJSON`(함수만) · `WAITFOR` | — | ❌ | — | — | — | — |
| `LATERAL` | ✅(FROM next) | — | ✅ `postgres.sqlg:14` | — | — | ❌(표준인데 없음) |
| `ILIKE` · `SIMILAR TO` · `IS DISTINCT FROM` · `FILTER` · `WINDOW` | — | — | ✅ `postgres.sqlg:9-31` | `WINDOW` ✅ `sqlite.sqlg:9` | — | `WINDOW` ❌ |
| `DISTINCT ON` | — | — | **❌** | — | — | — |
| `GROUPING SETS` / `ROLLUP` / `CUBE` | ✅ `oracle.sqlg:18` | ❌ | ✅ `postgres.sqlg:18` | — | `WITH ROLLUP` ❌ | ❌(표준인데 없음) |
| `FETCH FIRST … ROWS ONLY` / `OFFSET` | ✅(ansi) | `OFFSET … FETCH NEXT` ✅ | ✅ | ⚠ 상속(SQLite 미지원) | ⚠ 상속(MySQL 미지원) | ✅ |
| `LIMIT` | ⚠ 상속(Oracle 미지원) | ⚠ 상속(T-SQL 미지원) | ✅ | ✅ | ✅ | ⚠ 비표준이 ansi에 `ansi.sqlg:12, 75` |
| `FOR UPDATE` / `SKIP LOCKED` / `NOWAIT` | ✅ `oracle.sqlg:16` | — | `FOR UPDATE/SHARE` ✅ | — | `FOR UPDATE` · `LOCK IN SHARE MODE` ✅ | ❌ |
| INSERT / UPDATE / DELETE | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ `ansi.sqlg:13-18, 79-103` |
| `RETURNING` | ✅ `RETURNING INTO`·`BULK COLLECT INTO` `oracle.sqlg:39-41` | ⚠ 상속(T-SQL 없음 · 대신 `OUTPUT`) | ✅ | ✅ | ⚠ 상속(MySQL 없음) | ⚠ 비표준이 ansi에 `ansi.sqlg:14-18` |
| `OUTPUT` (T-SQL) | — | ✅ `mssql.sqlg:29-35` | — | — | — | — |
| `ON CONFLICT … DO NOTHING/UPDATE` | — | — | ✅ `postgres.sqlg:39-41` | ✅ `sqlite.sqlg:38-40` | — | — |
| `ON DUPLICATE KEY UPDATE` · `REPLACE INTO` · `INSERT IGNORE` | — | — | — | `REPLACE`/`INSERT OR …` ✅ | ✅ `mysql.sqlg:6, 24-28` | — |
| `MERGE` | ✅(ansi) | 부분(`WHEN NOT MATCHED BY SOURCE/TARGET` 없음) | ✅(PG15+) | ⚠ 상속(SQLite 미지원) | ⚠ 상속(MySQL 미지원) | ✅ `ansi.sqlg:19, 104-111` |
| DDL `CREATE/ALTER/DROP/TRUNCATE` 다음 낱말 | ✅(+`PACKAGE [BODY]`·`MATERIALIZED VIEW`·`SYNONYM`…) | ✅(+`OR ALTER`·`CLUSTERED`…) | ✅(+`EXTENSION`·`DOMAIN`…) | ✅(+`IF [NOT] EXISTS`·`VIRTUAL TABLE`) | ✅ | ✅ `ansi.sqlg:113-133` |
| 문장 시작에 오는 미지원 문장 | — | — | — | ⚠ `MERGE INTO`·`GRANT`·`REVOKE`·`TRUNCATE TABLE`·`CREATE PROCEDURE/FUNCTION/SEQUENCE` 상속 | ⚠ `MERGE INTO` 상속 | — |
| `CALL` | ❌(시작 낱말 없음 · `EXEC`만) | — | **❌**(PG11+ `CALL` 있음) | — | ✅ `mysql.sqlg:6` | ❌(표준인데 없음) |
| `COMMENT ON` | ✅ `oracle.sqlg:6` | — | ❌ | — | — | — |

---

### T-9 객체

#### T-9-1 객체 종류(`ObjectKind` 38종 · `nsql-catalog/src/lib.rs:14-82, 85`)와 방언별 탐색기 폴더(`kinds_for` `lib.rs:376-430`)

| 방언 | 폴더 종류 | 수 |
|---|---|---|
| ORA | Table · View · MaterializedView · Index · Sequence · Queue · Type · Package · Procedure · Function · Synonym · SchemaTrigger · Trigger · DbLink · JavaClass · Job · SchedulerJob · SchedulerProgram | 18 |
| MS | Table · ExternalTable · View · Index · Procedure · Function · Sequence · Synonym · Trigger · Type (+ SSMS 층: Database · DbUser · DbRole · LinkedServer · Schema · Login · ServerRole · Credential · CryptoProvider · ServerAudit · AuditSpec · BackupDevice · Endpoint · ServerTrigger) | 10 (+14) |
| PG | Table · ForeignTable · View · MaterializedView · Index · Procedure · Function · Aggregate · Sequence · Type (+ DB 수준 Extension · EventTrigger `lib.rs:432-438`) | 10 (+2) |
| MY | Table · View · Procedure · Function · Trigger | 5 (Event·Index 없음) |
| LT | Table · View · Index · Trigger | 4 |
| AN(ODBC) | Table · View | 2 |

#### T-9-2 하위 폴더(`tree.rs` `sub_kinds` · `SubKind` 18종)

| | ORA | MS | PG | LT/MY | AN |
|---|---|---|---|---|---|
| 테이블 | Columns·Constraints·FK·References·Triggers·Indexes·Partitions·Dependencies | Columns·UniqueKeys·Check·FK·Indexes·References·Triggers·ExtendedProperties | Columns·Constraints·FK·Indexes·Dependencies·References·Partitions·Triggers·Rules·Policies | Columns·Constraints·FK·Indexes·Triggers | Columns |
| 루틴 | Arguments·Dependencies | Arguments | Arguments | MY Arguments(⚠ 늘 빈 목록) · LT 없음 | — |
| 패키지 | Procedures·Functions·Dependencies + 멤버별 Arguments | — | — | — | — |

#### T-9-3 오버로드

| | ORA | MS | PG | LT | MY |
|---|---|---|---|---|---|
| 패키지 멤버 오버로드 구별 | ✅ `ALL_PROCEDURES.subprogram_id`+`overload` (`lib.rs:2222-2260`) · 멤버별 인자 필터 `member_overload` (`tree.rs:380-389, 441-460`) — 10-10 미커밋 변경 | — | — | — | — |
| 루틴 인자 조회 오버로드 열쇠 | `ALL_ARGUMENTS.overload` ✅ `lib.rs:3278-3340` | object_id(사실상 단일) | `pg_proc.oid` ✅ `lib.rs:3399-3443` | — | ❌ |
| 실행 시 오버로드 고르기(OUT 타입·캡처) | 부분 — "넘긴 인자 수 ≤ 형식 인자 수 + 이름 표기 일치"인 **첫 묶음**(타입·기본값 미고려) `nsql-run/src/lib.rs:1011-1026, 3029-3050` | 해당 적음 | 부분(같음) | — | — |
| PG 서명 포함 식별(`f(int, text)`) | — | — | 탐색기 `extra` = `pg_get_function_identity_arguments` ✅ `lib.rs:2061` · 소스 `::regprocedure` ✅ `lib.rs:3096-3108` · 이름만이면 `ORDER BY oid LIMIT 1` = **첫 오버로드** 부분 | — | — |

#### T-9-4 소스 열기 · Generate SQL

| | ORA | MS | PG | LT | MY | AN |
|---|---|---|---|---|---|---|
| 소스 원천 | `ALL_SOURCE`(PROC·FUNC·PKG·PKG BODY·TYPE) · 트리거·뷰 `GET_DDL` · 그 밖 `DBMS_METADATA.GET_DDL` ✅ `lib.rs:2840-3002` | 테이블 = 컬럼으로 조립 · 모듈 = `sys.sql_modules` · 연결된 서버 · 서버 트리거 ✅ (Sequence·Synonym·Type·Index 는 모듈이 아니라 부분) `lib.rs:3003-3078` | 테이블 조립 · 뷰 `pg_get_viewdef` · 루틴 `pg_get_functiondef` · 트리거 `pg_get_triggerdef` ✅ (Sequence·Type·Index 등 → `no_source`) `lib.rs:3079-3150` | `sqlite_master.sql` ✅(스키마 무시 · 첨부 DB 구별 없음) `lib.rs:3185-3197` | `SHOW CREATE …` ✅(드라이버 없음) `lib.rs:3151-3184` | ❌ `lib.rs:3198` |
| Generate SQL 종류(`gen_whats`) | SELECT·INSERT·UPDATE·DELETE·MERGE·CALL·DDL | 같음 | 같음 | 같음(MERGE = `ON CONFLICT`) | 같음(MERGE = `ON DUPLICATE KEY UPDATE … VALUES(col)`) | MERGE 제외 | `gen.rs:19-38, 278-315, 640-680` |
| 패키지 멤버 생성 | CALL만 ✅ `gen.rs:282-287` | — | — | — | — | — |
| 보수적 생성(스키마 한정 · 트리거 ON 한정) | ✅ | ✅ | ✅ | — | ✅(`qualify_mysql_header`) | — |

---

### T-10 루틴 호출

| | ORA | MS | PG | LT | MY | AN | 근거 |
|---|---|---|---|---|---|---|---|
| `EXEC 본문` 감싸기(`ExecForm`) | `BEGIN …; END;` | `EXEC …` / `SET @V =` / `SELECT @A =` | `CALL …` / `SELECT 식 AS "V"` | 같음(Call) | 같음 | 같음 | `caps.rs:47-55` · `dialect.rs:561-615` |
| 생성되는 호출문 | `EXEC name(p => :p, …)` · 함수 `EXEC :rc := f(…)` / `SELECT f(…) FROM DUAL` | `EXEC name @p = :p [OUTPUT]` · 테이블 함수 `SELECT * FROM f(…)` · 스칼라 `SELECT f(…)` | `CALL p(…)` · `SELECT * FROM f(…)`(위치 인자만 · `=>` 미사용) | `CALL`/`SELECT` | `CALL`/`SELECT` | — | `gen.rs:737-860` |
| 이름 표기 인자 파싱 | `=>` ✅ | `@x =` ✅ | `=>` ✅ | — | — | — | `call.rs:1-40, 98` |
| 이름 표기 인자 완성(T-316/317) | `이름 => ` ✅ | `@이름 = ` ✅(시스템 프로시저 30 + 사용자) | `이름 => ` ✅ | — | — | — | `intel.rs:39-59` · `builtins.rs:754-814` |
| OUT 처리(`OutValues`) | 프로토콜 OUT ✅ | 프로토콜 + `OUTPUT` 표기 보충 ✅ | 1행 결과 캡처 ✅ · refcursor 이름 → FETCH ✅ | 1행 결과 | 1행 결과 | 1행 결과 | `caps.rs:27-77` |
| 블록 종결자 | `/` ✅ | `GO` ✅ | `$$` 본문 + `;` ✅ | `BEGIN…END;` ✅ | `BEGIN…END;` 부분(`DELIMITER` ❌) | `/` | T-2-4 표 |

---

### T-11 트랜잭션·세션 문맥

| | ORA | MS | PG | LT | MY | AN | 근거 |
|---|---|---|---|---|---|---|---|
| 수동 커밋 시작문(`Caps.tx_begin`) | 없음(암묵) ✅ | `BEGIN TRANSACTION` ✅ | `BEGIN` ✅ | `BEGIN` ✅ | `START TRANSACTION` ✅ | 없음(드라이버 속성) | `caps.rs:108, 125, 145, 174, 180, 186` |
| 서버 자동 커밋(`server_autocommit`) | false | true | true | true | true | false | 같음 |
| DDL 트랜잭션성 / 암묵 커밋 | 암묵 커밋 ✅ | 트랜잭션 ✅ | ✅ | ✅ | 암묵 커밋 ✅ | (ORA·MY 외 = 트랜잭션 가정) | `nsql-core/src/lib.rs:795-816` |
| 쓰기 여부 탐침(`strict_probe`) | `dbms_transaction` ✅ | `dm_tran_*` ✅ | `pg_current_xact_id_if_assigned` ✅ | ❌ | ❌ | ❌ | `caps.rs:111-130, 146` |
| 문맥 전환 인식(`context_switch_name`) | `ALTER SESSION SET CURRENT_SCHEMA = x` ✅ | `USE x` ✅ | `SET search_path TO x` 부분(`SET SESSION/LOCAL search_path`·`SET SCHEMA` 미인식 · `"$user"` 이 첫 값이면 그대로 이름으로) | ❌(`ATTACH` 없음) | `USE x` ✅ | ❌ | `nsql-catalog/src/lib.rs:940-1003` |
| 현재 스키마 조회 | `SYS_CONTEXT` ✅ | `SCHEMA_NAME()` ✅ | `current_schema()` ✅ | `main` 고정 | `DATABASE()` | 빈 값 | `lib.rs:1006-1018` |
| 서버 `SHOW …` 문 | — | — | **❌** `SHOW search_path` 등이 클라이언트 명령으로 가로채여 "지원 안 함" | — | **❌** `SHOW DATABASES`/`SHOW CREATE TABLE`/`SHOW STATUS` 가로챔 · `SHOW VARIABLES` 는 **클라이언트 변수 표**로 바뀜 | — | `command.rs:315-317` · `nsql-run/src/lib.rs:2120-2185` |
| 서버 `SET …` 문 통과 | `SET TRANSACTION/ROLE` ✅ | `SET NOCOUNT` 등 ✅ | `SET search_path` ✅ | — | ✅ | — | `command.rs:282-285` |

---

### T-13 오류

| | ORA | MS | PG | LT | MY | AN | 근거 |
|---|---|---|---|---|---|---|---|
| 코드 표기 정규화(`native_code`) | `ORA-nnnnn` ✅ | `Msg n` ✅ | `SQLSTATE xxxxx` ✅ | `SQLITE n` ✅ | `#n` ✅ | 숫자 | `nsql-core/src/dberr.rs:143-180` |
| 분류(`classify` → 없는 객체·권한·제약·연결…) | ✅ | ✅ | ✅ | 부분(메시지 문자열) | ✅ | 부분 | `dberr.rs:182-340` |
| 오류 위치(`DbError.position`) | ✅ `offset()` `nsql-driver-oracle/src/lib.rs:297-309` | ❌ 줄 번호를 **메시지 글**에만(`(줄 n)`) `nsql-driver-mssql/src/lib.rs:540-553` | ✅ `db.position()` `nsql-driver-pg/src/lib.rs:161-183` | ❌ `nsql-driver-sqlite/src/lib.rs:77-123` | — | — | |
| 저장 코드 컴파일 오류(`SHOW ERRORS`) | ✅ `ALL_ERRORS` `caps.rs:96, 114` · `lib.rs:3445` | ❌(실행 오류가 곧 컴파일 오류) | ❌ | ❌ | ❌ | ❌ | |

---

### T-14 드라이버·대량 적재·취소

| | ORA | MS | PG | LT | MY | AN |
|---|---|---|---|---|---|---|
| 드라이버 크레이트 | `nsql-driver-oracle`(`oracle` 0.6 · Instant Client) ✅ | `nsql-driver-mssql`(`tiberius` 0.12 · rustls) ✅ | `nsql-driver-pg`(`postgres` 0.19) ✅ | `nsql-driver-sqlite`(`rusqlite` 0.32 bundled) ✅ | **❌ 없음** — `open()` 이 "드라이버는 이 빌드에 없습니다(M4)" 오류 `nsql-drivers/src/lib.rs:60-64` · `available()` 목록에도 없음 `:178-192` · 확장 드라이버(cdylib) 로더 코드 없음 | **❌ 없음**(같음) |
| `Caps.bulk_load` | `ArrayDml` ✅ `caps.rs:116` | `TdsBulk` ✅(`bulk_begin` 구현 `nsql-driver-mssql/src/lib.rs:1060` · `bulk.rs:18` 주석 "MultiRow 폴백"은 낡음) | `CopyIn` ✅ `caps.rs:152` | `MultiRow{500, 32000}`(드라이버 싱크 없음) 부분 | MultiRow(드라이버 없음) | MultiRow |
| 실행 취소(`CancelHandle`) | `break_execution` ✅ `oracle/src/lib.rs:532` | Attention/소켓 종료 ✅ `mssql/src/lib.rs:979` | `CancelToken` ✅ `pg/src/lib.rs:707` | `InterruptHandle` ✅ `sqlite/src/lib.rs:367` | — | — |
| 오류 위치 | ✅ | 메시지만 | ✅ | ❌ | — | — |

---

### §15 대조 (ANSI) — `ansi.sqlg` 와 표준 문장 목록

`ansi.sqlg [start] next`(`ansi.sqlg:8-9`) = SELECT · WITH · INSERT INTO · UPDATE · DELETE FROM · MERGE INTO · CREATE · ALTER · DROP · TRUNCATE TABLE · GRANT · REVOKE · COMMIT · ROLLBACK · SAVEPOINT · EXPLAIN · DESCRIBE · BEGIN · DECLARE.

| 표준 문장(SQL:2016 Part 2 기준) | ansi.sqlg | 비고 |
|---|---|---|
| query / `WITH [RECURSIVE]` | ✅ | `ansi.sqlg:20-23` |
| `INSERT` · `UPDATE` · `DELETE` | ✅ | |
| `MERGE` | ✅ 부분 | `WHEN MATCHED [AND] THEN UPDATE/DELETE` · `WHEN NOT MATCHED THEN INSERT` 만 |
| `TRUNCATE TABLE` | ✅ | |
| `CREATE SCHEMA/TABLE/VIEW/SEQUENCE/TRIGGER/PROCEDURE/FUNCTION/TYPE` · `CREATE INDEX`(비표준) | ✅ | `ansi.sqlg:113-114` |
| `CREATE DOMAIN` · `ASSERTION` · `ROLE` · `CHARACTER SET` · `COLLATION` · `TRANSLATION` · `CAST` · `ORDERING` · `TRANSFORM` · `METHOD` | ❌ | |
| `CREATE GLOBAL/LOCAL TEMPORARY TABLE` · `RECURSIVE VIEW` | ❌ | |
| `ALTER TABLE` ADD / DROP COLUMN / DROP CONSTRAINT / RENAME | ✅ | `ansi.sqlg:118-120` |
| `ALTER TABLE … ALTER COLUMN` · `SET/DROP DEFAULT` · `ADD COLUMN` | ❌(MS·PG 방언 파일만) | |
| `ALTER DOMAIN/TYPE/ROUTINE/SEQUENCE` | 부분(SEQUENCE만) | |
| `DROP …` + `CASCADE/RESTRICT` | 부분(`IF EXISTS`는 있음 · CASCADE/RESTRICT 없음) | `ansi.sqlg:121-123` |
| `GRANT` / `REVOKE` | ✅(다음 낱말 없음 — 시작 낱말만) | |
| `START TRANSACTION` · `SET TRANSACTION` · `SET CONSTRAINTS` · `RELEASE SAVEPOINT` | ❌ | |
| `COMMIT` · `ROLLBACK` · `SAVEPOINT` | ✅(시작 낱말만) | |
| `CALL` · `RETURN` | ❌ | |
| `DECLARE CURSOR` · `OPEN` · `FETCH` · `CLOSE` | 부분(`DECLARE`만) | |
| `SET SCHEMA/CATALOG/ROLE/SESSION AUTHORIZATION/TIME ZONE/PATH` | ❌ | |
| `CONNECT` · `DISCONNECT` · `SET CONNECTION` | (클라이언트 명령으로 처리) | `command.rs` |
| `PREPARE` · `EXECUTE` · `DESCRIBE`(동적) · `GET DIAGNOSTICS` | 부분(`DESCRIBE`만) | |
| `VALUES` 문 · `TABLE t` 문 | ❌ | |
| 질의 절: `WINDOW` · `LATERAL` · `TABLESAMPLE` · `GROUPING SETS/ROLLUP/CUBE` · `FILTER` · `FETCH NEXT` · `LEFT/RIGHT/FULL OUTER JOIN` 절 이름 | ❌ / 부분(OUTER JOIN `same=` 별칭은 있으나 FROM next에 `RIGHT OUTER`·`FULL OUTER` 없음) | `ansi.sqlg:28-50` |
| ansi에 섞인 **비표준** | `LIMIT` · `RETURNING` · `EXPLAIN` · `DESCRIBE` · `CREATE INDEX` · `BEGIN` | 모든 방언으로 상속 |

**강조기 키워드 집합**(`highlight.rs:439-448` · 196 고유): DML/DDL/TCL 기본 · PL/SQL 일부(`ELSIF` `LOOP` `PRAGMA` `AUTONOMOUS_TRANSACTION` `ROWTYPE` `EXCEPTION` `RAISE`) · T-SQL 일부(`TOP` `GO` `ISNULL` `GETDATE` `DATEADD`) · 자료형 27 · 함수 ~40.
빠진 것(표본): `CONNECT` `PRIOR` `START` `LEVEL` `NOCYCLE` `SIBLINGS` · `PIVOT` `UNPIVOT` · `LATERAL` `APPLY` `OUTPUT` · `CONFLICT` `NOTHING` `DO` `ILIKE` `SIMILAR` · `ROLLUP` `CUBE` `GROUPING` `SETS` `FILTER` `WITHIN` `NULLS` `LAST` `PRECEDING` `FOLLOWING` `UNBOUNDED` `CURRENT` `RANGE` `ROW` · `INTERVAL` `COLLATE` `ESCAPE` `ANY` `SOME` · `CALL` `EXPLAIN` `DESCRIBE` `USE` `LOCK` · `PRINT` `RAISERROR` `THROW` `TRY` `CATCH` `WAITFOR` `PROC` `IDENTITY` `NOLOCK` · `MATERIALIZED` `EXTENSION` `DOMAIN` `COMMENT` `GLOBAL` · `BULK` `COLLECT` `FORALL` · 자료형 `DATETIME` `DATETIME2` `TINYINT` `MONEY` `UNIQUEIDENTIFIER` `NCLOB` `RAW` `LONG` `JSONB` `UUID` `BYTEA` `TIMESTAMPTZ` `BIGSERIAL`.
강조기 어휘: 줄 주석 `--` 만 · 블록 주석 비중첩 · 문자열 = `'` **와 `"`**(인용 식별자가 문자열 색) · `escape_backslash = false` · `$$`·`q'…'`·백틱·`[ ]` 규칙 없음(`highlight.rs:434-438`).

---

### 결함·누락 후보 (완성·강조 오류로 이어질 구체 항목)

| # | 대상 | 증상 | 위치 |
|---|---|---|---|
| 1 | MY·PG | `E'…\'…'`·MySQL `'…\'…'` 백슬래시 이스케이프 미처리 → 문자열이 일찍 끝나 **문장 분리·바인드·완성 문맥이 전부 어긋남** | `crates/nsql-script/src/lexer.rs:96-111` |
| 2 | MY | `#` 줄 주석 미인식 → 주석 안 `;`·`'` 가 분리/문자열을 오염 | `lexer.rs:28-80` · 강조 `highlight.rs:434` |
| 3 | MY | 백틱 `` `name` `` 미인식(분석기·강조기) → `` `db`.`t`. `` 뒤 완성·Ctrl 링크 불가 · 백틱 안 `;` 분리 | `lexer.rs:63-80` · `outline.rs:121-205` · `highlight.rs:436` |
| 4 | MY | `DELIMITER //` 미지원(완성 키워드에는 제시) → 프로시저 본문이 `;` 에서 잘림 | `split.rs`(부재) · `intel.rs:861` |
| 5 | PG·MS | 중첩 블록 주석 `/* /* */ */` → 첫 `*/` 에서 끝남 | `lexer.rs:35-43` |
| 6 | PG | `[ … ]` 를 늘 인용 식별자로 분류 → 배열 첨자 `a[1]` · `ARRAY[1,2]` 안이 식별자 취급(완성·바인드 문맥 손실) | `lexer.rs:72-80` |
| 7 | 전체 | 구문 강조가 방언 무관 1벌(196) · 위 "빠진 것" 목록의 핵심어(`CONNECT BY`·`PIVOT`·`LATERAL`·`ON CONFLICT`·`OUTPUT`·`APPLY`·`ROLLUP`·`INTERVAL`·`COLLATE`·`CALL` …)와 MS/PG 자료형이 강조되지 않음 · `"…"` 가 문자열 색 | `nexa-ui/crates/nexa-ctl/src/highlight.rs:430-448` · `crates/nexa-sql/src/syntax.rs:20` |
| 8 | 전체 | 인용 판정 예약어 = 포맷터 절 키워드 117(방언 무관) — 실제 예약어인 ORA `LEVEL` `SIZE` `COMMENT` `UID` `USER` `ROWID` `ROWNUM` `SYSDATE` `NUMBER` `DATE` `FILE` `ACCESS` `RESOURCE` `SESSION` `MODE` `LOCK` `RAW` `LONG` `OPTION`, MS `KEY` `PRIMARY` `PROC` `IDENTITY` `OPEN` `CLOSE` `PLAN` `FILE`, PG `COLUMN` `CONSTRAINT` `CHECK` `REFERENCES` `UNIQUE` `DO` `ARRAY` `COLLATE`, MY `KEY` `KEYS` `RANGE` `RANK` 등이 빠짐 → 열 머리 DnD·셀 조건·`SELECT *` 템플릿이 **인용 없이** 넣어 구문 오류 | `crates/nexa-sql/src/identq.rs:52` → `crates/nsql-format/src/lib.rs:437-565` |
| 9 | 전체 | `.sqlg` 에 빼기 연산이 없어 ansi의 비표준·타 방언 항목이 상속 — ORA·MS 에 `LIMIT`, MS·MY 에 `RETURNING`, LT·MY 에 `MERGE INTO`·`FETCH FIRST`, LT 에 `GRANT`/`REVOKE`/`TRUNCATE TABLE`/`CREATE PROCEDURE` 후보가 뜸 | `grammar.rs:101-146` · `ansi.sqlg:8-18, 75` |
| 10 | PG | `CALL` 시작 낱말 없음 · `DISTINCT ON` 없음 · `COMMENT ON` 없음 | `postgres.sqlg:7-9` |
| 11 | ORA | `INSERT ALL/FIRST` · `MATCH_RECOGNIZE` · `AS OF`(flashback) · `WITH FUNCTION` · `MODEL` 절 정의 없음 | `oracle.sqlg` |
| 12 | MS | `FOR XML/JSON` · `WAITFOR` · `MERGE … WHEN NOT MATCHED BY SOURCE` · `GROUPING SETS/ROLLUP/CUBE` · `GO n` 없음 | `mssql.sqlg` · `split.rs:120` |
| 13 | ansi | 표준 문장 `CALL` · `START TRANSACTION` · `SET TRANSACTION` · `RELEASE SAVEPOINT` · `VALUES` · `ALTER COLUMN` · `CASCADE/RESTRICT` · `WINDOW` · `LATERAL` · `GROUPING SETS` 없음 | `ansi.sqlg:8-133` |
| 14 | PG·MY | 서버 `SHOW …` 가 클라이언트 명령으로 가로채여 "지원 안 함" · **MySQL `SHOW VARIABLES` 는 클라이언트 변수 표로 바뀜** · mysql.sqlg는 `SHOW DATABASES`/`SHOW CREATE TABLE`를 후보로 냄 | `crates/nsql-script/src/command.rs:315-317` · `crates/nsql-run/src/lib.rs:2120-2185` · `mysql.sqlg:6` |
| 15 | PG | 탐색기 루틴 Arguments 호출명이 `schema.f(a integer, b text)` 꼴인데 `pg_routine_args_sql` 은 `(` 를 벗기지 않고 `.` 로만 나눔 → `proname = 'f(a integer, …)'` 로 **인자 0건** 의심 · 인자 타입에 `.` 이 있으면 3조각 → `None` (정적 분석 · 실서버 확인 필요) | `crates/nsql-catalog/src/tree.rs:371-379` · `crates/nsql-catalog/src/lib.rs:3399-3410` · 같은 꼴 `gen.rs:919-921` · **✅ 10-10 확인·수정**(재현 = 정적 분석대로 · 서명 분리 + `pg_get_function_identity_arguments` 일치 · 실서버 V1 2/1/0행) · **#15-b**(같은 경로) = 이름 없는 인자가 `!name.is_empty()` 필터로 버려짐 → `$n` 표시 · 반환값 행(position 0)만 거름 ✅ 10-10 |
| 16 | PG | 서명 없이 소스 열기 = `ORDER BY oid LIMIT 1` 첫 오버로드 | `lib.rs:3104-3108` |
| 17 | ORA·PG | 실행 시 오버로드 선택 = 인자 수·이름만 보고 **첫 묶음**(타입·기본값 미고려) → 잘못된 OUT 타입/캡처 가능 | `crates/nsql-run/src/lib.rs:1011-1026, 3029-3050` |
| 18 | ORA | `routine_args` 의 2조각 이름 `A.B` 가 (현재 스키마, 패키지 A, B)와 (소유자 A, B) 두 후보를 OR 로 합쳐 같은 `overload='0'` 묶음에 섞일 수 있음 | `lib.rs:3297-3330` |
| 19 | 전체 | **✅ 10-10 D-272**(Oracle만 묻기 · `script.define` · CLI 전 경로) · 종전: `&name` 치환이 방언과 무관하게 기본 켬 · 문자열 안도 치환 · 미정의면 입력 요청 → MS/PG/MY 의 `'R&D'`·`a & b` 에서 뜻밖의 입력 창 | `crates/nsql-script/src/engine.rs:127, 754-794` |
| 20 | 내장 표 | ORA 함수 표에 의사열·패키지 멤버·프로시저 혼입(`ROWNUM` `ROWID` `DBMS_LOB.GETLENGTH` `RAISE_APPLICATION_ERROR`) · MS 함수 표에 문장 `RAISERROR` `THROW` · ORA 자료형 `BOOLEAN` 중복(뒤 것 23ai 설명이 버려짐) | `builtins.rs:73-130, 178-223, 994, 1000` |
| 21 | 자료형 | ORA `VECTOR` · MS `json`/`vector` · PG 배열 `T[]`·`bit`·`varbit`·`jsonpath` · MY 공간 자료형 일부 · `SERIAL` 없음 | `builtins.rs:961-1150` |
| 22 | MY·AN | 드라이버 없음 — 문법·키워드·카탈로그 질의는 있으나 실행 불가 · MY 루틴 Arguments 폴더는 늘 빈 목록 | `crates/nsql-drivers/src/lib.rs:60-64` · `lib.rs:3285-3287` · `tree.rs` `sub_kinds` |
| 23 | MS·LT | 오류 위치 미전달(MS = 메시지 글에 줄만) → 편집기 오류 위치 표시 없음 | `nsql-driver-mssql/src/lib.rs:540-553` · `nsql-driver-sqlite/src/lib.rs:77-123` |
| 24 | PG | 문맥 전환 = `SET search_path TO …` 만 · `SET SESSION/LOCAL search_path` · `SET SCHEMA 'x'` 미인식 · `"$user"` 첫 값이면 그대로 이름 | `crates/nsql-catalog/src/lib.rs:985-1001` |
| 25 | 문서 | `BulkLoad::TdsBulk` 주석 "지금은 MultiRow 폴백"은 낡음(실제 `bulk_begin` 구현됨) | `crates/nsql-core/src/bulk.rs:18` |
| 26 | MY 생성 | `ON DUPLICATE KEY UPDATE c = VALUES(c)` = MySQL 8.0.20부터 폐기 예고 꼴 | `crates/nsql-catalog/src/gen.rs:666-672` |
| 27 | 포맷터 | `$$…$$` 본문을 문자열로 보지 않음(PG 함수 본문 서식 위험) · `#` 주석 없음 | `crates/nsql-format/src/lexer.rs:146-260` |

---

### 요약(T-15 열로 넣을 한 줄 판정)

| 템플릿 | ORA | MS | PG | LT | MY | AN |
|---|---|---|---|---|---|---|
| T-2 어휘 | ✅ | 부분(중첩 주석·`GO n`) | 부분(`E''`·중첩 주석·`[]`) | ✅ | 부분(`#`·백틱·`\'`·`DELIMITER` ❌) | 부분 |
| T-3 예약어 | 부분 | 부분 | 부분 | 부분 | 부분 | 부분 |
| T-4 자료형 | ✅ | ✅ | ✅ | ✅ | ✅ | 부분 |
| T-6 함수 | ✅(+패키지 22) | ✅(+시스템 프로시저 32/30) | ✅ | ✅ | ✅ | 부분(공통 39) |
| T-7/8 질의·DML | ✅(INSERT ALL·MATCH_RECOGNIZE ❌) | 부분 | 부분(CALL·DISTINCT ON ❌) | 부분(상속 오염) | 부분(상속 오염) | 부분 |
| T-9 객체 | ✅ | ✅ | ✅(#15·#15-b 10-10 수정) | 부분 | 부분(드라이버 없음) | 부분 |
| T-10 루틴 | ✅ | ✅ | ✅ | 부분 | 부분 | 부분 |
| T-11 트랜잭션·문맥 | ✅ | ✅ | 부분(`SHOW` 가로챔) | 부분 | 부분(`SHOW` 가로챔) | 부분 |
| T-13 오류 | ✅ | 부분(위치 없음) | ✅ | 부분 | 부분 | 부분 |
| T-14 드라이버·적재·취소 | ✅ | ✅ | ✅ | 부분(적재 싱크 없음) | ❌ | ❌ |
