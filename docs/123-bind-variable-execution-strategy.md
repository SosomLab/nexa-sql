# 123 · 바인드 변수 실행 전략 — `:VAR`는 바인드로 가는가, 리터럴로 가는가 (138차 Linux · 2026-10-24 · 검토)

> 요구(사용자 10-24): "Oracle 기준 `EXEC :VAR1 := 'ABC'` 뒤 `SELECT * FROM TB_ITEM WHERE ITEM_CD = :VAR1`이 현재 값으로 치환돼 실행되는지 바인드 변수로 실행되는지 확인 · 각 DBMS가 바인드를 지원하면 되도록 바인드로 실행되게 하는 것이 좋다 · 현재 상태와 향후 방향을 정리". 원천 = DR-8(세션 변수는 클라이언트에 산다) · [63](63-variable-management.md)(변수 관리) · [05 §7](05-mssql-tools.md)(SQL Server 세션 변수 3가지 설계) · [44 §9](44-transaction-log.md)(바인드 3계층 로그) · `nsql-script::dialect::prepare_with` · `nsql-core::caps::Marker`.

## 0. 한 줄 결론

**지금도 바인드가 기본이다.** 내장 7방언 전부 `:VAR1`을 서버 바인드 변수로 보낸다(Oracle `:VAR1` 이름 바인드 · SQL Server `@VAR1` + `sp_executesql` 매개변수 · PostgreSQL `$1` · MySQL/SQLite/ODBC/DuckDB `?`). 리터럴 치환은 **바인드가 불가능한 자리에서만** 쓴다 = ① SQL Server에서 배치 첫 문장이어야 하는 DDL·`USE`·`SET` 옵션(`DECLARE @v = 리터럴` 프리펜드 · 05 §7 (b) 폴백) ② `&이름` 치환 변수(SQL*Plus 의미 그대로 = 글자 치환이 목적) ③ `EXEC :A := 50`처럼 **우변이 리터럴인 대입**(서버에 보내지 않고 로컬 대입 · DR-8) ④ 새 ClickHouse 드라이버(HTTP 인터페이스에 위치 바인드가 없어 1차는 리터럴 치환 · 2차 = 서버 매개변수). 방향 = 이 원칙을 **D-356 규칙으로 고정**하고, 남은 리터럴 자리를 줄이며(ClickHouse 2차 · 실행 계획의 바인드), 어느 쪽으로 갔는지를 **로그·계획에서 보이게** 한다.

## 1. 사용자 예제가 실제로 가는 길(Oracle · 10-24 코드 확인)

| 단계 | 문장 | 엔진이 하는 일 | 서버로 가는 것 |
|---|---|---|---|
| 1 | `EXEC :VAR1 := 'ABC';` | 우변이 리터럴(`parse_literal` 성공) → **로컬 대입**(`Action::LocalAssign` · 변수 표 `VAR1 = 'ABC'` · 타입 자동 = VARCHAR2(3)) | **아무것도 안 감**(왕복 0 · DR-8) |
| 2 | `SELECT * FROM TB_ITEM WHERE ITEM_CD = :VAR1;` | `extract_binds` → `VAR1` · 능력표 `Marker::Named`(Oracle) → 문장은 **그대로** · `BindParam{ name: VAR1, value: 'ABC', ty: VARCHAR2(3), direction: In }` | 문장 원문 + 바인드 1(OCI `OCIBindByName` / Thin 바인드) — 서버는 `:VAR1`을 바인드 변수로 파싱 · 플랜 캐시 재사용 · 값은 데이터로만 |
| 2′ | 같은 문장 · 우변이 식일 때(`EXEC :B := :A + 10`) | 식은 서버가 계산해야 하므로 `BEGIN :B := :A + 10; END;`(InOut 바인드 · OUT 값 흡수) | PL/SQL 블록 + 바인드 2 |

확인 방법(사용자 실기 가능) = 실행 뒤 **트랜잭션 로그 창**의 문장 행을 펼치면 3계층(원문 · 재작성 · 드라이버 바인드 매핑 · 44 §9)이 보인다 · Oracle 서버 쪽은 `V$SQL`에서 `SQL_TEXT LIKE '%:VAR1%'`가 그대로 남아 있으면 바인드(리터럴이면 `'ABC'`가 박힌다).

## 2. 방언별 현재 상태(코드 기준 · `prepare_with`)

| 방언 | Marker | 보내는 꼴 | 리터럴로 가는 경우 | 타입 |
|---|---|---|---|---|
| Oracle(OCI·Thin) | `Named` | `:VAR1` 그대로 + 이름 바인드(같은 이름 1회) | 없음 | 변수 타입(VARCHAR2(n)·NUMBER·DATE·REF CURSOR…) · 암묵 변수 = NULL |
| SQL Server | `AtName` | `@VAR1` + `sp_executesql @stmt, @params`(RPC · OUTPUT = 돌아오는 값) · 바인드 하나뿐인 SELECT 항목엔 `AS [:VAR1]` 열 이름 | **DeclarePrepend** = 배치 첫 문장이어야 하는 `CREATE/ALTER PROCEDURE·FUNCTION·VIEW·TRIGGER·SCHEMA…` · `USE` · `SET <옵션>`(`SET @x`는 제외) → 머리에 `DECLARE @v <type> = <리터럴>;`(줄 오프셋 보정 · 05 §7 (b)) | 선언 안 한 글자 변수는 InOut일 때 NVARCHAR(4000)으로 넓힘(09-21 잘림 결함 이후) · `sql_variant` + `[V$type]`로 서버 타입 복원(T-162 ⑤) |
| PostgreSQL | `DollarN` | `$1 $2 …`(같은 이름 = 같은 번호) · 바인드만 있는 열은 `AS ":VAR1"` | 없음 | 변수 타입 → PG 타입(`datetime_bind_type_by_dialect`) |
| MySQL/MariaDB · SQLite · ODBC · DuckDB | `Question` | `?`(등장 순서 · 같은 이름이 두 번이면 두 번) | 없음 | 드라이버 매핑(ODBC = `SQLBindParameter`) |
| ClickHouse(138차 · D-354) | `Question` + 드라이버 치환 | 드라이버가 `?`를 **리터럴로 치환**(문자열 이스케이프 · 이진 = `unhex`) | **전부**(1차) — HTTP 인터페이스는 `{name:Type}` 서버 매개변수뿐 | 없음(리터럴) |
| 공통 | — | `&이름`/`${이름}` 치환 변수(DEFINE · 내장 변수 · 환경 변수) | **전부**(설계상 글자 치환 = SQL*Plus) | — |
| 공통 | — | `EXEC :V := <리터럴>` | 로컬 대입(왕복 0) · `EXEC :V := <식>`은 서버 | — |
| 그리드 편집(87) | 방언 Marker | 자동 UPDATE/DELETE/INSERT 값 = **바인드**(`gridedit_sql::Sink` · 미리보기 글만 리터럴 · 날짜는 `TO_DATE`·`DATE ''` 표기) | 없음 | 셀 타입 |
| 벌크 적재(89) · SQL 내보내기 | — | 다중 행 INSERT 폴백·내보내기 파일 = 리터럴(`to_sql_literal`) · 드라이버 적재 경로(`BulkLoad`)는 바인드/네이티브 | 파일 생성물은 본질상 리터럴 | — |

## 3. 왜 바인드가 맞나 — 기술 평가

| 관점 | 바인드 | 리터럴 치환 |
|---|---|---|
| 서버 플랜 캐시 | 같은 문장 = 같은 커서 재사용(Oracle 하드 파싱 0 · SQL Server 플랜 재사용 · PG prepared) | 값마다 다른 문장 = 하드 파싱·플랜 캐시 오염(Oracle `cursor_sharing=FORCE`로 완화해도 DBA가 싫어하는 패턴) |
| 타입 충실도 | 값이 타입 그대로(NUMBER · DATE · TIMESTAMP · NVARCHAR · BLOB) | 직렬화 규칙에 의존(날짜 형식 · NLS · 소수 자릿수 · 이진 hex · 긴 글 4000 초과) |
| 안전 | SQL 삽입 불가(값은 데이터 채널) | 이스케이프 실수 = 삽입 · `'` · 백슬래시 · 주석 |
| 오류 위치 | 원문 좌표 유지(T-371 `map_error_position`) | 치환으로 길이가 변해 좌표 보정 필요(DeclarePrepend = 줄 오프셋) |
| 실행 계획 | 옵티마이저가 **바인드 피킹**으로 첫 값 기준 계획 · `EXPLAIN PLAN`(Oracle)은 값·타입을 모른다 → 실제 계획은 `--analyze`(DISPLAY_CURSOR · 122) | 값이 보이니 `EXPLAIN`이 정확한 대신 운영과 다른 플랜 |
| 세션 변수 의미 | SQL*Plus `VARIABLE`/`:x`와 같은 모델(OUT 흡수 · REF CURSOR) | SQLCMD `:setvar`/DBeaver `${x}` 모델(글자) |
| 비용 | 드라이버 바인드 1회 · 클라이언트 재작성(`replace_refs` 선형) | 문자열 조립 |

결론 = **바인드 가능한 자리는 전부 바인드**(지금 상태) · 리터럴은 "바인드가 문법상 불가능한 자리"로 한정 · 그 자리는 로그에 표시.

## 4. 향후 방향(권장 · 번호 = 결정 후보)

| # | 내용 | 비용 | 우선 |
|---|---|---|---|
| D-356 | **규칙 고정**: `:이름`은 바인드 · 리터럴 치환은 (가) 방언/드라이버에 바인드가 없을 때 (나) 배치 구조상 불가능할 때(SQL Server DeclarePrepend)만 · `&이름`은 글자 치환(의도) · 로컬 대입은 우변 리터럴만 — 61 §1 불변식에 한 줄 | 문서 | P1 |
| T-416 | **가시성** = 실행 로그·트랜잭션 로그 문장 행에 전달 모드 꼬리표(`bind N` · `declare-prepend` · `inline`) + `nsql plan`에 같은 표시 → 사용자가 "치환인지 바인드인지"를 묻지 않아도 보인다(44 §9 3계층에 한 칸) | 소 | P1 |
| T-417 | **ClickHouse 2차 = 서버 매개변수** `{name:Type}` + URL `param_name=` — 변수 타입 → ClickHouse 타입(VARCHAR2→String · NUMBER→Float64/Int64/Decimal · DATE→Date · TIMESTAMP→DateTime) · 바인드 추출은 지금 `?` 치환 자리와 같음 · 리터럴 치환은 타입을 모르는 값만 | 중 | P2 |
| T-418 | **실행 계획과 바인드**(122 보강) — Oracle `EXPLAIN PLAN`은 바인드 타입을 VARCHAR2로 가정 → `--analyze`(실행 뒤 DISPLAY_CURSOR) 안내 · SQL Server SHOWPLAN은 `DECLARE` 프리펜드로 가므로 "값 기준 계획" 표시 · 계획 결과 탭 머리에 "바인드 기준/리터럴 기준" 한 줄 | 소 | P2 |
| T-419 | SQL Server **DeclarePrepend 최소화** — `USE`·`SET 옵션` 단독 문장은 변수 참조가 없으면 프리펜드 생략(이미 params 0이면 머리 비움 = 확인) · `CREATE PROCEDURE` 본문의 `:x`는 서버 코드이므로 바인드 대상 아님(`is_stored_code_ddl` = 이미) → 남는 것 = "배치 첫 문장 DDL + 바인드 참조" 조합뿐 → 경고 토스트("이 문장의 :x는 리터럴로 들어감") | 소 | P3 |
| T-420 | **암묵 변수 타입** — 선언 없이 처음 쓰는 `:x`(NULL · 타입 Auto)는 서버가 타입을 정한다(Oracle VARCHAR2 · SQL Server sql_variant) → `VARIABLE x NUMBER` 선언 권고 진단(`Diagnostic::ImplicitVariable` = 이미) + 변수 창에서 타입 고정 버튼 | 소 | P3 |
| — | 하지 않을 것 = "전부 리터럴" 옵션(플랜 캐시·안전 후퇴) · 세션 변수의 서버 저장(05 §7 (c) SESSION_CONTEXT)은 명시 옵션으로만(이미 결정) | — | — |

## 5. 검증 자산(현재 있음)

- `nsql-script` 단위 = `prepare_with` 방언별(Named/AtName/DollarN/Question) · `needs_declare_prepend` · `lone_select_items` · `bind_type` 넓힘.
- 통합 = `integration.rs` 각 방언 `*_bind_*`(Oracle·MSSQL·PG·MySQL·SQLite·ODBC) · 실서버 E2E `win-vars-e2e.sh`(26) · `define-e2e.sh`(변수·DEFINE 왕복).
- 실기 확인 = 트랜잭션 로그 3계층 · Oracle `V$SQL`/`V$SQL_BIND_CAPTURE` · SQL Server Profiler/XE `rpc_completed`(sp_executesql 매개변수) · PG `log_statement = 'all'`($1 표기).
