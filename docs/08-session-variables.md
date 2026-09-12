# 08 · 세션 변수 · 스크립트 엔진 설계 (`nsql-script`) — ★ 핵심 차별점

> 사용자 요구(09-12): *"Oracle 기준 sqlplus의 기능(변수 선언·세션 내 재사용·refcursor)을 지원하고, SQL Server에서는 직접 안 되는 이 기능을 간접으로라도 다수의 쿼리에서 동일한 변수를 함께 쓰게 하고 싶다."* + *"`connect user/pass@host:port/db;`로 에디터 안에서 세션 전환."*
> 근거 조사: [04 §8](04-oracle-tools.md)(SQL*Plus 의미론) · [05 §7](05-mssql-tools.md)(T-SQL 배치 제약과 우회 3안).
> **구현 상태(09-12)**: `crates/nsql-script` 33 테스트 green · `nsql plan` dry-run으로 사용자 예시 스크립트가 Oracle·MSSQL 양쪽으로 계획됨([examples/golden-session-vars.sql](../examples/golden-session-vars.sql)).

## 1. 한 문장 원리

**SQL*Plus의 바인드 변수는 서버가 아니라 클라이언트 메모리에 산다**([04 §8.2](04-oracle-tools.md)). 매 실행마다 문장을 스캔해 `:NAME`을 찾아 버퍼를 바인딩하고, 실행 후 OUT 값이 다음 문장의 IN 값이 된다. 우리는 이것을 **DBMS 무관 엔진**으로 복제하고, DBMS 차이는 **방언 재작성 계층** 하나로 흡수한다. 그래서 단일 앱으로 가능하다(D-1 답).

```text
편집기/CLI 텍스트
   │ split_script            항목: Command | Sql(Query/Dml/Ddl/Block) — 원문 span·줄번호 보존
   ▼
Engine::plan(item)  ──▶  Action
   │   ├ LocalAssign      EXEC :V := '리터럴'      → DB 왕복 0 (모든 방언 동일)
   │   ├ Execute(Prepared) :NAME → 방언 플레이스홀더 + 파라미터(값·타입·방향)
   │   ├ Print / Connect / Describe / Show / Spool / RunScript / NeedInput(&치환)
   ▼
호스트(GUI·CLI)가 드라이버 실행  ──▶  ExecResult{out_params}  ──▶  Engine::absorb  (저장소 갱신)
```

## 2. 문장 분리 규칙 (`split.rs`)

| 입력 | 항목 | 근거 |
|---|---|---|
| 줄 첫머리 `VAR` `PRINT` `EXEC` `CONN` `SET` `DEF` `DESC` `SHOW` `SPOOL` `PROMPT` `@` `@@` `REM` `GO` `WHENEVER` `:setvar` `:connect` `:r` | 한 줄 명령(`-` 줄 연속 지원) | SQL*Plus + sqlcmd |
| `EXEC` 홀로 → 다음 줄부터 `;`·`/`·빈 줄·다음 명령까지 | **블록 EXEC** | ★ Golden 관용(사용자 예시) |
| 문자열·주석 밖 `;` | SQL 종료(꼬리 주석은 잘라냄) | SQL*Plus |
| `BEGIN`·`DECLARE`·`CREATE [OR REPLACE] PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE` | 블록 — `;`가 아니라 **단독 `/`** 로 종료 | SQL*Plus |
| 단독 `GO` | 배치 종료 | sqlcmd(방언 무관 인식) |
| `q'[…]'` · `''` · `"…"` · `[…]` · `--` · `/* */` | 스캐너가 보호(`lexer.rs`) | Oracle·T-SQL |

## 3. 변수 저장소 (`vars.rs`)

- 이름은 대소문자 무관(대문자 정규화). `VARIABLE name TYPE [= 값]`으로 선언하거나, ★ **선언 없이 대입하면 `Auto` 타입으로 암묵 생성**(3-2) — Golden이 그렇게 동작하는 것으로 보이고(사용자 예시가 선언 없이 `EXEC :V := …` 후 사용), 사용성이 훨씬 낫다. 엔진은 `Diagnostic::ImplicitVariable`로 알린다(호스트가 경고 표시 · 설정으로 엄격 모드 가능).
- 타입: SQL*Plus 부분집합 `NUMBER · VARCHAR2(n) · CHAR(n) · CLOB · REFCURSOR · BINARY_FLOAT/DOUBLE · DATE · TIMESTAMP` + `Auto`(값에서 추론). BLOB/BOOLEAN은 19c SQL*Plus에 없으므로 제외.
- 값: `Null · Int · Float · Decimal(문자열 보존) · Str · Bool · Bytes · Cursor(id)`. **`Decimal`은 문자열로 보존** — Oracle `NUMBER(38)`을 f64로 깎지 않는다.
- ★ **세션(연결)과 변수 테이블을 분리**한다: `CONNECT`로 연결을 갈아끼워도 변수는 워크시트에 남는다(SQL*Plus 동일). 단 `Cursor` 값은 연결에 묶이므로 재접속 시 무효화.

## 4. 방언 재작성 (`dialect.rs`) — 단일 앱을 가능하게 하는 층

| 방언 | `:NAME` → | 모드 | EXEC 래핑 | SELECT … INTO :X |
|---|---|---|---|---|
| **Oracle** | `:NAME` 그대로 | 드라이버 이름 바인드(IN/InOut) | `BEGIN 본문; END;` | PL/SQL 그대로(OUT 바인드) |
| **SQL Server** | `@NAME` | 1차 `sp_executesql` RPC 파라미터(`OUTPUT`) · 2차 배치 첫 문장 DDL·`USE`·`SET`이면 `DECLARE @v 타입 = 리터럴;` 프리펜드(줄 오프셋 기록) | `:V := e` → `SET @V = e` · 그 외 `EXEC 본문` | ★ `SELECT @X = a, @Y = b FROM …` 로 변환 |
| **PostgreSQL** | `$n` | 위치 바인드 | `:V := e` → `SELECT e AS "V"` · 그 외 `CALL` | (후속) |
| MySQL · SQLite · ODBC | `?` | 위치(중복 등장 = 중복 전송) | `CALL` | (후속) |

- ★ **리터럴 대입은 로컬**: `EXEC :V_PRG_NM := 'SP_…'`은 어느 방언에서도 DB를 부르지 않는다. 이것이 사용자가 원한 "SQL Server에서도 다수의 쿼리가 같은 변수를 쓰는" 기능의 절반이고, 나머지 절반은 `sp_executesql` OUTPUT 회수다.
- 타입 매핑(T-SQL): `NUMBER→DECIMAL(38,10)`, `VARCHAR2(n)→NVARCHAR(n|MAX)`, `CLOB→NVARCHAR(MAX)`, `Auto→SQL_VARIANT`. 커서는 `sp_executesql`로 못 넘긴다([05 §7](05-mssql-tools.md)) → MSSQL에서 REFCURSOR는 "결과 집합 탭"으로 대체(호스트 정책).
- (c) `SESSION_CONTEXT` 방식은 **명시 옵션**으로 후속(서버 측 코드가 변수를 봐야 할 때).

## 5. `CONNECT` (`connect.rs`)

`user/pass@host:1521/svc` · `user/pass@tns` · `user@host`(비밀번호 프롬프트) · `/ as sysdba` · `mssql://user:pass@host:1433/db` · `…?dialect=pg`. 엔진은 `Action::Connect(spec)`만 내고, 호스트가 트랜잭션 커밋 → 재접속 → 방언 교체를 수행한다(SQL*Plus: CONNECT는 커밋 후 재접속).

## 6. 치환 변수 (`engine.rs::substitute`)

`&NAME`·`&&NAME`·`DEFINE`·`:setvar`·스크립트 인자 `&1..&n`. 미정의면 `Action::NeedInput` → 호스트가 프롬프트 후 `define()` → 같은 항목 재계획. `SET DEFINE OFF` 지원. **주석 안은 치환하지 않는다**(SQL*Plus와 다른 의도적 완화 — 사고 방지).

## 7. 열린 것 (TODO)

- `SET SERVEROUTPUT` 폴링(`DBMS_OUTPUT.GET_LINES`) · `SET AUTOTRACE`(2차 세션) · `SPOOL` 실체 · `@`/`@@` 파일 로드(호스트) — 엔진은 Action만 낸다.
- 오류 위치 역매핑: `Prepared::line_offset` + `Item::span`으로 편집기 줄 복원.
- Oracle 스크립트를 MSSQL에 그대로 돌릴 때 `EXEC SP(a, b)` 괄호 호출은 T-SQL 문법이 아니다 — 자동 변환하지 않는다(경고만). 스크립트 호환은 **변수 공유**까지이지 프로시저 호출 문법까지가 아니다.
