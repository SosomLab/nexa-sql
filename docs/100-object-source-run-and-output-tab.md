# 100. 객체 소스 탭의 F5 = 객체 단위 실행 · Output 탭 · 소스 열기 vs DDL 생성(09-30 · 108차 win)

> 사용자 요구(09-30 저녁): ① 소스 열기와 DDL 생성의 차이 분석 ② 소스 열기 뒤 F5가 프로시저를 CREATE하지 않고 줄 단위로 도는 것 →
> 객체(뷰·패키지·함수·프로시저 …)에서 다르게 동작 ③ 프로시저 편집에서는 결과 그리드보다 "컴파일이 잘 되었는지" 같은 **메시지**가 더
> 뜻이 있다 → 일반 편집 탭에서도 프로시저 실행·PRINT 메시지를 보는 **Output 탭**(처음엔 없음 · 보기 설정 · 메시지가 오면 자동 · 다른
> 클라이언트의 기본 기능) ④ APPDB·APPDB_SB의 `SP_DMP_MP_VERSION_CREATE_BSY`를 DDL/소스 열기로 열어 실제 내용과 일치 확인 ⑤ 객체 선언·
> 테이블·인덱스·PK/UK·제약 등 **전 유형 DDL 전수 테스트**(appdb 자동 접속 · Debug 직접 실행).

## 1. 소스 열기(Open source) vs Generate SQL ▸ DDL — 차이 분석

| | 소스 열기 | Generate SQL ▸ DDL |
|---|---|---|
| 진입 | 탐색기 더블클릭/Enter · 우클릭 ▸ 소스 열기(패키지는 Body 항목 따로) | 우클릭 ▸ SQL 생성 ▸ DDL |
| 원천 함수 | `nsql_catalog::source` | `nsql_catalog::generate(GenWhat::Ddl)` → `object_ddl` |
| Oracle PL/SQL(프로시저·함수·패키지·본문·트리거·타입) | `ALL_SOURCE` 줄을 이어 붙이고 앞에 `CREATE OR REPLACE `, 끝에 `/` 한 줄 | `DBMS_METADATA.GET_DDL` → `plsql_terminate`(블록마다 `/` · 빈 줄) |
| 이름 표기 | ~~사용자가 쓴 그대로(스키마 없음 · `PROCEDURE          SP_X`)~~ → **09-30 저녁: `PROCEDURE APPDB_SB.SP_X`**(소유 스키마 한정 · 공백 1칸 · `explorer.source_schema` 기본 켬 · `normalize_plsql_header`) | Oracle 표기 `"APPDB"."SP_X"` + `NONEDITIONABLE` 등 수식어 |
| 패키지 | 스펙만 · 본문은 별도 탭(`NAME.body.sql`) | 스펙 + 본문을 한 본문에(블록 2개 · `/` 2개) |
| Oracle 뷰 | `CREATE OR REPLACE VIEW "OWNER"."NAME" AS` + `ALL_VIEWS.TEXT`(종결자 없음) | `GET_DDL('VIEW')` + `;` |
| Oracle 그 밖(테이블·인덱스·시퀀스 …) | `GET_DDL` 그대로 | `GET_DDL` + `;` · 테이블은 옵션(`qualified` · `compact` · `full` · FK 분리) + 머리 주석 `-- schema.name definition` |
| SQL Server | `OBJECT_DEFINITION` → `CREATE OR ALTER` + `GO` | 루틴·뷰·트리거 = **소스 열기와 같은 본문**(`source()` 폴백) · 테이블 = `table_ddl` |
| PostgreSQL | `pg_get_functiondef`/`pg_get_viewdef`/`pg_get_triggerdef` + `;` | 위와 같음(폴백) |
| 결과 자리 | **새 편집기 탭**(캐럿 BOF · 편집 가능) | **SQL Preview 모달**(옵션 · 바인드 주석) → "편집기에서 열기" 버튼이 새 탭 |
| 세션(09-30 전) | 활성 공유 세션(다른 서버일 수 있었다) | 카탈로그의 메타 세션 |
| 세션(09-30 후) | ★ **그 객체의 서버 세션에 묶는다**(`bind_tab_to_server` · 없으면 전용 하나) | 그대로 |

요약: **소스 열기 = "지금 저장된 소스를 그대로 다시 컴파일할 수 있는 최소 본문"**(ALL_SOURCE 그대로 · 사용자 표기 유지) ·
**DDL 생성 = "DBMS가 말하는 정식 정의"**(수식어·따옴표·스키마 · 여러 객체를 한 본문으로 · 테이블 옵션). 둘 다 본문(2행~)은 `ALL_SOURCE`와
글자 단위로 같다(§4 실측).

## 2. 객체 소스 탭의 F5 = 객체 단위 실행

### 2-1. 원인 분석("줄 단위 실행")
- CLI `nsql plan -d oracle`로 같은 본문을 나누면 **1항목(Block)** 이다 — 분할기(`split_script_in`)는 첫 줄이 `CREATE [OR REPLACE] PROCEDURE …`면
  블록으로 보고 다음 `/` 줄까지 한 항목으로 잡는다. Oracle 방언에서는 재현되지 않는다.
- 재현 가능한 길 둘: ① 소스 열기 탭이 **활성 공유 세션**(다른 서버 = PG/MySQL/SQLite 방언)에 붙어 있으면 `;`마다 끊긴다(선언부 `V_X NUMBER;` ·
  `END IF;` …) ② 머리 줄이 한 줄이 아니면(`CREATE OR REPLACE` 줄 바꿈 뒤 `PROCEDURE`) 블록으로 못 본다. T-SQL `CREATE PROC` 약어도 종전엔
  블록이 아니었다(이번에 추가).
- 어느 쪽이든 **객체 소스 탭은 "한 객체 = 한 실행"이 사용자의 뜻**이므로 분할 자체를 하지 않는 길을 만든다.

### 2-2. 구현
- 탐색기 → 탭 출처: `ExplorerAction::OpenSql { origin: Option<ObjectOrigin> }`(`schema` · `name` · `kind` = 연 종류(패키지 본문 = `PackageBody`) ·
  `server` = 카탈로그 스펙 · `ExplorerSet`이 채움). SELECT 템플릿은 `None`.
- 앱: `App::object_tabs: HashMap<편집기 탭 id, ObjectOrigin>`(탭이 닫히면 정리) · 탭을 **그 서버의 세션에 묶는다**(`bind_tab_to_server` =
  `new_tab_on`에서 부품화 · 공유 → 전용 → 없으면 그대로).
- F5(`run.all`): 객체 탭이면 `run_text_whole(src)` → `Cmd::Run { whole: true }` → `Runner::run_whole`: 분할이 정확히 1항목이면 그대로 ·
  아니면 `whole_item(src)`(끝의 `/`·`GO` 줄과 마지막 `;` 정리 · 블록은 `END;` 유지 · 종류 = 첫 단어)로 **한 항목**을 `run_item`. 컴파일 검사
  (`report_compile_errors`)·탐색기 갱신(`ddl_target`)은 보통 길과 같다. Ctrl+Enter(현재 문장)는 종전대로(부분 실행은 그대로 가능).
- 객체 유형별 동작 = 종류로 갈린다: 프로시저·함수·패키지(스펙/본문)·트리거·타입 = `Block` 한 번 + 컴파일 검사 → Output에 `PROCEDURE X compiled ·
  no errors` 또는 `Warning: … created with compilation errors` + 줄/열 목록 · 뷰·MV = `Ddl` 한 번(`;` 제거) → 완료 줄 · 그 밖 = `Ddl`.
- Output 첫 줄 `▶ PROCEDURE APPDB.NSQLT_P2 — 소스 전체를 한 단위로 실행(F5)` · 상태줄 `… 한 단위로 실행 중`.
- ★ 컴파일 **성공**도 메시지가 된다(`RunEvent::Message` = `Msg::OutCompiledOk` · 종전엔 오류만) — SQL*Plus "Procedure created."에 해당.

## 3. Output 탭

### 3-1. 다른 클라이언트
| 클라이언트 | 자리 | 내용 |
|---|---|---|
| SSMS | Results 옆 **Messages** 탭 | `(N rows affected)` · PRINT · RAISERROR · `Command(s) completed successfully.` · 오류(줄 번호) |
| DBeaver | 결과 아래 **Output** 탭(Ctrl+Shift+O) + 별도 Execution Log | DBMS_OUTPUT · 서버 출력(켜기 버튼) |
| PL/SQL Developer | SQL 창의 **Output** 탭 · 프로그램 창의 컴파일 오류 목록 | DBMS_OUTPUT · 컴파일 결과 |
| SQL Developer | Script Output · **Dbms Output** 창(세션마다 켬) | 스크립트 결과 · 서버 출력 |
| TablePlus | 하단 **Messages/Log** | 실행 로그 · 오류 |

공통 기본 기능 = 시간순 누적 · 문장 완료/영향 행 · 서버 출력(PRINT/DBMS_OUTPUT) · 오류(줄) · 지우기 · 복사 · 상한.

### 3-2. 구현(부품 재사용 · 새 컨트롤 0)
- `crates/nexa-sql/src/output.rs` `OutputView` = 읽기 전용 고정폭 `TextBox`(뷰 탭 `ext_view`와 같은 부품) + 머리 줄(`Output · N` · **지우기** · **전체 복사**) ·
  줄마다 `[HH:MM:SS]` + 표식(`⚠` 경고 · `✖` 오류) · 여러 줄 메시지는 시각 폭만큼 들여쓰기 · `output.max_lines` 초과 = 오래된 줄부터 · 새 줄 = 끝으로 스크롤 ·
  드래그 선택 · ⌘/Ctrl+C · 우클릭 복사 · 전체 선택.
- 결과 패널: `ResultPanel::output: Option<OutputView>`(편집기 탭마다 하나 · 탭을 닫아도 본문은 남는다) + 자리표시 탭 `ResultTab::is_output`(그리드는 빈 것 ·
  고정 = 자동 회수 제외 · 이름 있음 = 번호 제외). 활성 탭이 Output이면 그리기·입력은 그리드 대신 Output으로(`paint.rs` · `input.rs` · 그리드 코드 무변경).
- 이벤트 → 줄(`app/output.rs::output_on_event` · 실행한 편집기 탭 `sess.run_editor`): `ResultSet`(`[n] N행 가져옴 · ms`) · `Done`(`[n] 완료 · N행 영향 · ms`) ·
  `Print`(`X = 7`) · `VarList` · `Message`(DBMS_OUTPUT · T-SQL PRINT · RAISE NOTICE · EXEC 결과 `:X = 7` · compiled · Committed) · `Warning`(⚠) · `Error`(✖ `N행: …`).
  로그 창과는 별개(로그 = 전부 · Output = 사람이 읽는 메시지만).
- 표시 정책(설정 · 카테고리 **Output** · 일반 그룹):

| 키 | 값 | 기본 | 뜻 |
|---|---|---|---|
| `output.show` | off / auto / errors / always | **auto** | 탭이 나타나는 때 — off = View ▸ Output으로만 · auto = 첫 메시지 · errors = 경고·오류만 · always = 처음부터 |
| `output.activate` | never / no_results / always | **no_results** | 메시지가 오면 Output 탭으로 전환 — 결과 셋이 없는 실행(컴파일 · DDL · PRINT)만 · 오류는 늘 · 포커스는 안 뺏는다 |
| `output.done_lines` | on/off | on | 문장 완료 줄(SSMS Messages식) |
| `output.timestamps` | on/off | on | `[HH:MM:SS]` |
| `output.max_lines` | 100~100000 | 5000 | 편집기 탭마다 보관 줄 |
| `output.serveroutput` | on/off | **on** | Oracle 새 세션마다 `SET SERVEROUTPUT ON`(D-239 · 실행마다 GET_LINES 1회 · 부하원 39 §3) |

- View ▸ **Output**(Ctrl+Shift+O · ⌘⇧O · DBeaver와 같은 키) = 활성 편집기 탭의 Output 탭 토글(닫아도 본문 유지) · 팔레트 "View: Show/Hide Output" · 탭 × 도 같다.
- 자체 시험: 기동 명령 `output.dump:<파일>`(첫 줄 `보임|줄수`) · `editor.dump:<파일>`(첫 줄 `제목|종류|읽기전용`) · 단위 테스트 `output::tests` 3 · `whole_item` 1.

### 3-2-a. 결과 탭과의 전환(10-01 ㉘ · 사용자)

- **결과 셋이 오면 결과 탭으로**: Output을 보던 중이라도 `RunEvent::ResultSet`이 오면 그 결과 탭이 활성(`results::switch_to_result` · `output.activate = always`만 예외 · 포커스 유지).
- **실행 시작 때 Output이 활성이면** 결과는 Output 자리표시가 아니라 **가장 최근 결과 탭**(없으면 새 결과 탭)으로(`run_target_tab` · ㉘-b "2행 가져옴인데 결과 탭이 비어 있다").
- 자체 시험 `result.dump:<파일>` · E2E `scripts/win-output-result-e2e.sh`.

### 3-3. 남은 것(T-266 · T-267)
- Output 줄 색(오류 = danger · 경고 = amber · 텍스트 색만) · 탭 라벨 미읽음 배지 · 줄 더블클릭 = 오류 줄로 이동 · 필터/찾기 · SQL Server `RAISERROR` 심각도 분류 ·
  PG `RAISE NOTICE` 레벨 표식 · Output만 있는 실행에서 "결과1" 빈 탭 안 만들기(`grid.result_tabs`와 조율) · 객체 탭 = 탭 줄 색/아이콘(`TabKind::Object`) ·
  객체 탭 F5 뒤 탐색기 상태(VALID/INVALID) 즉시 갱신 확인 · 위키 캡처.
- ✅ **10-30(181차 7차 · T-266 · D-417)**:
  ① **메시지 등급** = nsql-run `MessageLevel{Info, Warning, Error}` + `message_level(text)` — 글 머리 접두로 판정(`WARNING`/`WARN` → Warning · `EXCEPTION`/`ERROR`/`FATAL`/`PANIC` → Error · `NOTICE`/`INFO`/`DEBUG`/`LOG`/그 밖 → Info · 접두 뒤 `:` 또는 공백 필요) · GUI `RunEvent::Message` → 등급별 `OutKind` · PostgreSQL = notice 콜백이 `parsed_severity`(프로토콜 V 필드 · 서버 로케일과 무관한 영어 · 없으면 서버 글자)로 `"WARNING: …"` 접두를 보장 · **한계 = SQL Server `RAISERROR` 심각도 ≤ 10(정보 메시지)은 tiberius가 message만 넘겨 Info로 분류**(심각도 11+는 오류 경로 = 종전대로 오류) · 드라이버 변경은 별도.
  ② **줄 단위 색** = nexa-ctl `TokenKind::Error`(danger) · `Warn`(warn) 추가(SQL 강조 규격은 내지 않음) · nexa-sql `OutHighlighter` = 줄 머리 `[hh:mm:ss] `를 건너뛰고 `✖` = 오류 · `⚠` = 경고 · 들여쓴 이어진 줄은 앞 줄 종류 유지.
  ③ **Output 안 찾기** = Output 탭에 포커스가 있을 때 Ctrl+F → 같은 찾기 막대가 **Output 본문**을 대상으로(표식 · 다음/이전 · 범위 그대로 · 읽기 전용이라 바꾸기는 무변경) · Output 탭이 바뀌면 편집기 대상으로 돌아감.
  ④ ✅ **SQL Server 메시지 등급(11-01 216차 · T-266 종결)** = `nsql_run::message_level_in(dialect, text)` — SQL Server는 TDS Info 토큰(심각도 ≤ 10 = PRINT · RAISERROR ≤ 10)만 메시지로 오고 ≥ 11은 Error 토큰 = 오류 경로이므로 **글 접두로 가리지 않고 늘 Info**(tiberius가 class를 주지 않아 토큰 의미로 판정 · `PRINT 'ERROR: x'`가 빨갛게 보이던 흠) · 다른 방언 = ①의 접두 판정 그대로.

## 4. 실기(09-30 · Debug · appdb 자동 접속 · 키 주입 0)

- **`SP_DMP_MP_VERSION_CREATE_BSY` APPDB(69줄) · APPDB_SB(495줄)**: CLI `cat source`/`cat gen ddl` = GUI 소스 열기/DDL 미리보기 덤프와 **글자 단위 일치** ·
  소스 열기 = `CREATE OR REPLACE ` + `ALL_SOURCE`(68/494줄) + `/` 일치 · DDL 본문(2행~) = `ALL_SOURCE`(2행~) 일치(머리 줄만 `NONEDITIONABLE "APPDB"."…"`) ·
  스키마별로 다른 판이 맞게 들어온다(둘 다 서버 상태 INVALID = 탐색기 `object!`).
- **전 유형 DDL E2E `scripts/oracle-ddl-e2e.sh -o <폴더> -n target/debug/nsql.exe -p appdb`** = **67/67 통과**(임시 객체 `NSQLT_*` · 끝에 0개 남김): 테이블 ×2 · PK · UK · CHECK ·
  NOT NULL · FK(ON DELETE CASCADE) · 인덱스 ×2(복합) · 코멘트 ×2 · 시퀀스 · ALTER(ADD/MODIFY/RENAME/DROP COLUMN · DISABLE/ENABLE) · 뷰 · 함수 · 프로시저 · 패키지 스펙+본문(한 스크립트) ·
  트리거 · 타입 · 시노님 · MV · 상태 VALID 9 · **소스 왕복 = 원문 일치 6종** · DDL 생성(테이블·프로시저·패키지 2블록) · 컴파일 오류 보고(`Warning … compilation errors` + 줄/열 · `cat errors` · INVALID) ·
  소스 열기 본문 재실행(1항목) · 스펙+본문 재실행(2항목) · DDL 본문 재실행 · DML(트리거 ID 채움 · 함수 42 · 패키지 8) · DBMS_OUTPUT 2 · PRINT · 제약 위반 3(ORA-02290/00001/02291) · 캐스케이드 · DROP 전부.
  스크립트 1차의 실패 7건은 전부 기대값 쪽(종류 이름 `body` · 뷰 따옴표 · `PLS-`가 아닌 `ORA-` · SQL에서 패키지 상수 참조 불가)이었다.
- **GUI(Debug · 격리 홈 + 프로필 복사 · 기동 명령)**: 일반 탭 `SET SERVEROUTPUT ON` + 블록 + `EXEC` + `PRINT` + `SELECT` → Output 6줄(`hello from DBMS_OUTPUT` ·
  `NSQLT_P2: 42` · `[2] 완료` · `:X = 7` · `X = 7` · `[6] 1행 가져옴`) · 객체 탭 F5(`NSQLT_P2`) → 3줄(`▶ …` · `[1] 완료 · 60 ms` · `compiled · no errors`) ·
  틀린 `NSQLT_BAD2` F5 → `✖ 3행: Warning … compilation errors` + `3/16 ERROR: PL/SQL: ORA-00936 …` 2줄 · 서버 `LAST_DDL_TIME` = 실행 시각(동일 소스의 VALID 객체는 Oracle이 재컴파일하지 않아 시각이 안 바뀐다 = 정상).
  캡처 `target/capture/objf5_0.png`.

## 5. ★ 객체 소스 읽기 원칙 + DBeaver 대조(09-30 밤 · 사용자 "최종 결과에 영향을 줄 수 있으면 보수적으로 · 다른 DBMS도 · DBeaver 기능을 검토해 유사하게")

### 5-1. 원칙(불변식 · CLAUDE.md §3 · 61 §1-11)
**소스 열기·DDL 생성이 만든 본문을 그대로 실행했을 때 서버의 최종 결과(객체의 소유 스키마 · 컬럼 이름/순서 · 참조 대상 · 상태)가 원본과 달라질 수 있으면, 사전(데이터 딕셔너리)의 정의를 명시해 달라질 수 없게 만든다.** 편의(짧은 본문)보다 결과 보존이 우선. 항목 = ① 이름의 소유 스키마 한정 ② 뷰의 컬럼 목록 ③ 트리거·인덱스·제약의 참조 테이블 한정 ④ 상태(DISABLED · FORCE) ⑤ 종결자. 설정 `explorer.source_schema`(기본 켬)를 끄면 ①③만 풀린다(사용자 선택).

- ★ **11-10 조용한 손실 금지 사례(D-479 · [journal 11-10 §15](journal/2026-11-10.md))** = "소스 전체를 한 단위로 실행"(F5)은 드라이버가 받은 문자열 **전부**를 실행하거나 오류를 내야 한다 — SQLite에서 rusqlite `prepare`가 첫 문장만 실행하고 나머지를 버려 `DROP VIEW …; CREATE VIEW …` 본문이 뷰를 지운 채 "완료"로 끝났다(8b069b69 = 비조회는 `execute_batch` · 조회/바인드 꼬리는 오류) · 새 드라이버·실행 경로는 "여러 문장 본문을 한 번에 넘겼을 때 전부 실행 또는 명시 오류"를 시험으로 확인한다.

### 5-2. DBeaver가 하는 방식(대조 · 09-30 조사)
| DBMS · 객체 | DBeaver | Nexa SQL(09-30 이후) |
|---|---|---|
| Oracle PL/SQL(프로시저·함수·패키지·타입) | `ALL_SOURCE` 본문 + 머리 줄 재조립(`PROCEDURE SCHEMA.NAME` · 공백 정리) · `/` 없음 | 같음(`normalize_plsql_header`) + `/`(스크립트 실행용) |
| Oracle 뷰 | `DBMS_METADATA.GET_DDL('VIEW')`(FORCE · NONEDITIONABLE · 컬럼 목록) | 같음(EMIT_SCHEMA = 설정 · `;`) |
| Oracle 트리거 | DBeaver는 `ALL_TRIGGERS.DESCRIPTION + TRIGGER_BODY`로 재조립(제 이해) — `ON` 테이블은 저장된 표기 | **더 보수적**: `ALL_SOURCE` + 이름 한정 + `ON 테이블`을 `TABLE_OWNER`로 한정 + DISABLED면 `ALTER TRIGGER … DISABLE;` |
| Oracle 테이블·인덱스·시퀀스·MV·시노님 | `GET_DDL`(DBMS_METADATA 사용 옵션 · 실패 시 자체 생성기) | `GET_DDL` + 옵션(qualified · compact · full · FK 분리) · 종속 인덱스는 본문에 없는 것만(`index_already_in`) |
| SQL Server 루틴·뷰·트리거 | `sys.sql_modules.definition`(= `OBJECT_DEFINITION`) 그대로 · 스키마는 저장된 표기 | 같은 원천 + `CREATE OR ALTER` + **머리 줄 `[schema].` 한정**(`qualify_tsql_header`) + `GO` |
| PostgreSQL 함수·프로시저 | `pg_get_functiondef`(스키마 한정) | 같음 |
| PostgreSQL 뷰·MV | `pg_get_viewdef` + 자체 머리 줄(컬럼 별칭은 본문에 `AS`로 남음) | 같음 |
| PostgreSQL 트리거 | `pg_get_triggerdef` | 같음 + **`ON schema.table` 한정**(`qualify_on_table` · search_path에 있으면 스키마가 빠지던 것) |
| MySQL 뷰·테이블 | `SHOW CREATE`(DB 한정됨) | 같음 |
| MySQL 루틴·트리거 | `SHOW CREATE`(이름에 DB 없음 = 현재 DB) | 같음 + **`\`db\`.` 한정 + 트리거 `ON \`db\`.\`t\``**(`qualify_mysql_header`) |
| SQLite | `sqlite_master.sql` 원문 | 같음(단일 스키마) |

순수 함수 넷 + 시험: `normalize_plsql_header`(11) · `qualify_on_table` · `qualify_tsql_header` · `qualify_mysql_header`(`conservative_qualification_helpers`). Oracle 전 유형 왕복 = `scripts/oracle-ddl-e2e.sh` 67/67(한정 형태 기대값 · `-s <OWNER>`).

### 5-3. 남은 격차(T-268) — ✅ 10-01 대부분 반영(SQL Server 트리거 `ON` · PG 함수 참조 · GET_DDL 폴백 · PG/MSSQL 왕복 E2E `scripts/dbms-source-e2e.sh` 33/33 · CLI `qualify=off`) · 남음 = MySQL 실서버 왕복(프로필 없음)
- SQL Server 트리거의 `ON table` 한정(`sys.triggers.parent_id`로 부모 표 알기) · MSSQL 뷰의 컬럼 목록(저장 정의 그대로 = DBeaver와 같음 · 결과 보존은 됨).
- PG `pg_get_triggerdef`의 함수 참조(`EXECUTE FUNCTION f()`)도 search_path 의존 → 한정.
- DBeaver의 "Use DBMS_METADATA" 실패 시 자체 생성기 폴백(권한 없는 계정) — 우리는 GET_DDL 실패 = 오류 표시.
- 소스 열기 탭 F5 직전 "객체 스키마 ≠ 세션 현재 스키마" Output 경고(T-267).

## 6. 구현 범위 현황(11-03 · 소스 기준 · 협업 분석 · 사용자 "객체 편집기 구현 범위 검토 · 어떤 기능을 사용할 수 있는지 안내")

> 설계가 아니라 **main e05ee1b2(v0.5.0) 소스에 구현된 것** 기준 · 근거 = 파일:줄(그 시점 줄 번호 · 이동하면 함수 이름으로 찾기) 또는 E2E 스크립트 · 조사 = 협업 세션의 읽기 전용 하위 에이전트 + 협업이 핵심 근거 2곳 직접 대조(Oracle `ALL_ERRORS` · 그리드 사전 검사) · 원문 임시 파일은 정리 배치(D-445) 대상이라 이 절이 원본 · [journal 11-03 §2](journal/2026-11-03.md).
>
> **결론 한 문장** = 객체 편집 = "텍스트 기반 DDL 왕복" — 탐색기 소스 열기(스키마·`ON 테이블`·DISABLED 보존 · §5) → 편집 → F5 한 객체 한 실행(§2) → Output 컴파일 결과(§3 · Oracle은 `ALL_ERRORS`까지) → 실행 직전 자동 백업(114 §7) + DDL 생성(Generate SQL) + 그리드 행/셀 편집(87 §14 보호 세 겹 · 설정으로 못 끔) + 탐색기 삭제(2단 확인 + 백업) + 객체 소스 탭 북마크(69 §4-2) + 상세 패널(86). **폼형 구조 편집기(119 = 설계만) · 권한 편집 · 이름 바꾸기 UI · 의존 그래프는 없음.**

### 6-1. 표 1 — 기능 × 방언(✓ · △ 부분 · ✗ 없음 · 근거)

| 기능 | Oracle | SQL Server | PostgreSQL | MySQL/MariaDB | SQLite | DuckDB | Db2 | ClickHouse | ODBC | 근거 |
|---|---|---|---|---|---|---|---|---|---|---|
| ① 탐색기 트리 종류 폴더 수 | 18 | 10 | 10(+DB 수준 2) | 5 | 4 | 2(Table·View) | 9 | 2 | 2 | `nsql-catalog/src/lib.rs:376-452` `kinds_for` · `:444-451` `db_kinds_for` |
| ① 소스 열기(더블클릭/Enter/우클릭) | ✓ | ✓ | ✓(Type 제외) | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ | `lib.rs:3188-3588` `source_with` · 메뉴 노출 = `lib.rs:268-287` `has_source` · `explorer.rs:8383` |
| ① 소스 열기 보수적 한정(스키마 · `ON 테이블` · DISABLED) | ✓ (`normalize_plsql_header` · `qualify_on_table` · `ALTER TRIGGER … DISABLE`) | ✓ (`[schema].` + 트리거 `ON` 부모 표) | ✓ (`ON schema.table` + `EXECUTE FUNCTION` 스키마) | ✓ (`` `db`.`name` `` + 트리거 `ON`) | ✓(단일 스키마) | — | △ 원문 `SYSCAT.*.text` 그대로(한정 안 함) | — | — | `lib.rs:3214-3246` · `:3344-3380` · `:3420-3455` · `:3504-3523` · `:3468-3500` |
| ① 소스 탭 = 그 객체 서버 세션에 묶음 · 작업 단위(DB/스키마) | ✓ | ✓(DB) | ✓ | ✓(DB) | ✓ | — | ✓ | — | — | `explorer.rs:7045-7050` `Req::Source{db: unit_for}` · `explorer.rs:927-939` `OpenSql{origin, db, server}` · docs/100 §2-2 |
| ② F5 = 객체 한 단위 실행(분할 없음) + Output 안내 + 스키마 불일치 경고 | ✓ | ✓ | ✓ | ✓ | ✓ | (소스 탭 없음) | ✓ | — | — | `app/run.rs:152-187` · `:257-259` `run_text_whole` |
| ② 컴파일 결과 메시지(`compiled · no errors` / `Warning … compilation errors` + 줄/열) | ✓ (`ALL_ERRORS`) | ✗(서버가 CREATE 자체를 거부 → 실행 오류 경로) | ✗(같음) | ✗ | ✗ | — | ✗ | — | — | `nsql-run/src/lib.rs:3501-3560` `report_compile_errors`(`caps.compile_errors`) · `nsql-catalog/src/lib.rs:3899` Oracle만 |
| ② 실행 오류 위치 → 편집기 캐럿 이동(T-371/372) | ✓ | ✓(줄 + 오타 분석) | ✓ | (추정 ✓) | ✓ | (추정) | (추정) | (추정) | (추정) | `scripts/win-error-goto-e2e.sh:1-12`(SQLite·PG·Oracle thin/OCI·SQL Server) · `app/run.rs:185-186` `run_src_base` |
| ② 컴파일 오류 줄 → 캐럿 | △ Output 줄 **두 번 클릭** = 그 줄로(자동 이동 없음) | — | — | — | — | — | — | — | — | `app/output.rs:52-59` |
| ② 실행 직전 객체 백업(`CREATE OR REPLACE`/`ALTER`/`DROP`/`TRUNCATE`/`RENAME` 대상) | ✓ | ✓ | ✓ | ✓ | ✓ | △(DDL 생성이 골격이라 테이블만) | ✓ | △ | △ | `app/objbackup.rs:1-13` · `:259-302` `objbk_plan` · `backups.rs:69` 폴더 |
| ③ DDL 생성(Generate SQL ▸ DDL) 테이블 | ✓ `DBMS_METADATA` + 옵션(qualified·compact·full·FK 분리) + 종속 인덱스 | ✓ DBeaver식(`-- DROP` · 인라인 제약 · FK 분리 · 인덱스 · 확장 속성) | ✓ `pg_table_ddl` | △ 골격(컬럼·기본값·NOT NULL) + `ALTER … ADD CONSTRAINT` + `CREATE INDEX`(`SHOW CREATE TABLE` 아님 · 엔진/charset 없음) | ✓ `sqlite_master` + 인덱스·트리거 한 벌 | △ 골격+키 | △ 골격+키 | △ 골격+키 | △ 골격+키 | `gen.rs:1058-1172` `object_ddl` · `:1473-1500` `table_with_keys` · `lib.rs:3618-3645` `table_ddl` |
| ③ DDL 생성 그 밖 종류 | ✓ 전 종류(`GET_DDL` 타입 표) · PL/SQL은 블록마다 `/` | ✓ 루틴·뷰·트리거(= 소스) · 인덱스 · 시퀀스 | ✓ 루틴·뷰·MV·인덱스·시퀀스·타입(enum/composite)·확장 | ✓ 뷰·루틴·트리거(= 소스) | ✓ 전부(`sqlite_master.sql`) | ✗ | ✓ 뷰·루틴·트리거(= 소스) · 시퀀스/인덱스/MV/동의어 ✗ | ✗ | ✗ | `gen.rs:957-978` `oracle_meta_type` · `:1059-1118` · `:1119-1144` · `:1145-1170` · `:1172` `_ => source` |
| ③ 하위 항목 DDL(제약 · 인덱스 · 트리거 잎 우클릭) | ✓ | ✓ | ✓ | ✗(폴더는 있으나 `sub_ddl` 분기 없음 · 추정 = 오류) | ✓ | — | ✗(추정) | — | — | `gen.rs:1630-1720` `sub_ddl` · 메뉴 = `explorer.rs:8411-8418` · `gen.rs:296-316` `gen_whats` |
| ③ SQL Preview 모달(옵션 · 새로고침 · 파일 저장 · 편집기에서 열기 · 복사) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `sqlprev_win.rs:1-6` · `explorer.rs:983-988` `Preview` |
| ④ 그리드 셀/행 편집 · 삽입 · 삭제 · 복제 · NULL/빈 문자열 · 지금 날짜 | ✓ | ✓ | ✓ | ✓ | ✓ | △(코드상 허용 · 실서버 미검증) | △(허용 · 미검증) | ✗ 추정(일반 `UPDATE`/`DELETE` 문장을 ClickHouse가 거부) | △(허용 · 미검증) | `gridedit_sql.rs:1-9` · `grid.rs:5768-5852`(메뉴 id) · `keymap.rs:1074-1079` |
| ④ 행 식별 2급 물리 식별자(숨은 열 주입) | ✓ ROWID | ✗(D-218 · 키/전 열 비교) | ✓ ctid+xmin | ✗(키/전 열) | ✓ rowid | ✗ | ✗ | ✗ | ✗ | `gridedit_sql.rs:339-360` `physical_cols` |
| ④ 데이터 보호 ① 사전 검사 `COUNT(*)=1` ② 영향 행 수 = 1 ③ 유일성(대기 변경 안 키 중복) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `gridedit_sql.rs:556` · `:860-863` · `nsql-run/src/lib.rs:1641-1730` · docs/87 §14 표 |
| ④ 자동 커밋 실패 = 전체 롤백 · 수동 커밋 = 세이브포인트로 이번 적용분만 · 되돌리기 실패 = "ROLLBACK 필요" | ✓ | ✓ `SAVE TRANSACTION` | ✓ | ✓ | ✓ | △ 세이브포인트 없음(수동 모드 실패 = `rollback_needed` 안내만) | ✓ `ON ROLLBACK RETAIN CURSORS` | △ 없음 | △ 없음 | `nsql-run/src/lib.rs:97-110` `savepoint_sql` · `:1634-1640` · `:1731-1785` · `nsql-core/src/caps.rs:210-212 · 243 · 253 · 310` |
| ④ 운영(PRD) 접속 = 그리드 편집 읽기 전용 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `gridedit_sql.rs:26-28` `ReadOnly::Prod` · `nsql-i18n/src/lib.rs:5627` |
| ④ 실서버 E2E | ✓ 임시 표(PK · 키 없음 · DATE · CLOB) | ✓(CLOB nvarchar(max)) | ✓(ctid · text) | ✗(프로필 없음) | ✓ 격리 홈 Local 8항목 | ✗ | ✗ | ✗ | ✗ | `scripts/mac-grid-edit-e2e.sh:1-10` · docs/87 §11 10-09 줄 |
| ⑤ 객체 상세 패널(속성 · 테이블 = PK/인덱스 · 뷰 = 의존 · 루틴 = 인자 · 그 밖 = 하위 폴더 표) | ✓ | ✓ | ✓ | ✓ | ✓ | △(속성·컬럼만) | ✓ | △ | △ | `nsql-catalog/src/detail.rs:66-70` · `:205-303` · `objdetail.rs:1-4` |
| ⑤ 탐색기 하위 폴더(컬럼 · 제약 · FK · 참조 · 인덱스 · 트리거 · 파티션 · 의존 …) 테이블 기준 | 8 | (UniqueKeys 등 · `tree.rs:192`) | (Policies 포함 · `tree.rs:202`) | 5 | 5 | Columns만 | 6 | Columns만 | Columns만 | `nsql-catalog/src/tree.rs:182-227` `sub_kinds` |
| ⑤ 상세 머리 줄 더블클릭 = 편집기 탭 · Shift+F4 ℹ 정보 탭 · CLI `nsql cat detail` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `objdetail.rs:24-31` `DetailAction::OpenInEditor` · `app/meta.rs:176-188` · `keymap.rs:537-542` |
| ⑥ 객체 소스 탭 북마크(문서 열쇠 = 프로필·방언·스키마·종류·이름·부분 · 재시작 뒤 유지 · DROP/RENAME 뒤 무효) | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | — | — | `nsql-bookmarks/src/lib.rs:17-32` `DocKey::Object` · `app/bookmarks.rs:141 · 310-348 · 642-650` · docs/69 §4-2 10-31 구현 줄 |
| ⑥ 객체 백업(DROP · REPLACE/ALTER/F5 · TRUNCATE/RENAME 각각 켬/끔 · 30일 · 객체별 N개 · 주기 정리) | ✓ | ✓ | ✓ | ✓ | ✓ | △ | ✓ | △ | △ | `app/objbackup.rs:19-48` `BkAction::setting` · `nsql-settings/src/lib.rs:3166-3206` |
| ⑥ 운영 접속 + 백업 실패 = 실행 막음 · 비운영 = 경고 + 계속 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `app/objbackup.rs:10-11` · `app/drop.rs:5-6` |
| ⑥ 탐색기 ▸ Delete…(모달 2단 타임아웃 · 백업 · 보수형 DROP · CASCADE 없음) | ✓ | ✓(+서버 수준 로그인·역할·연결된 서버 …) | ✓(루틴 서명 · 트리거 `ON`) | ✓(인덱스 `ON`) | ✓ | ✓(Table·View) | ✓ | ✓(Table·View) | ✓(Table·View) | `nsql-catalog/src/lib.rs:2890-2976` `drop_sql` · `app/drop.rs:1-9` · `explorer.rs:8397-8403` |
| ⑥ 삭제 E2E | ✓(PRD 모드 · `NSQLT_BK_PRC`) | ✗ | ✗ | ✗ | ✓(PRD/none 대조) | ✗ | ✗ | ✗ | ✗ | `scripts/win-objbackup-prod-e2e.sh:1-9` · `scripts/win-drop-prod-e2e.sh:1-6` |
| 보너스 — CREATE 문 ↔ 서버 DDL 비교 탭(`obj.compare` · 서버 DDL 열기) | ✓ | ✓ | ✓ | ✓ | ✓ | △ | △ | △ | △ | `app/compare.rs:1-8`(DDL 생성과 같은 원천) |


### 6-2. 소스 열기 종류 × 방언(탐색기에 그 종류 폴더가 있고 "소스 열기"가 실제로 본문을 내는가)

| 종류 | Oracle | SQL Server | PostgreSQL | MySQL | SQLite | Db2 | DuckDB/ClickHouse/ODBC | 근거(`nsql-catalog/src/lib.rs`) |
|---|---|---|---|---|---|---|---|---|
| 뷰 | ✓ `GET_DDL('VIEW')`(컬럼 목록·FORCE 보존) · 권한 없으면 `ALL_VIEWS.TEXT` + 컬럼 목록 폴백 | ✓ `OBJECT_DEFINITION` → `CREATE OR ALTER` + `[schema].` + `GO` | ✓ `pg_get_viewdef` | ✓ `SHOW CREATE VIEW` | ✓ `sqlite_master.sql` | ✓ `SYSCAT.VIEWS.text` | ✗ `no_source`(폴더는 있음 · 메뉴 "소스 열기"가 뜨되 실패 — 추정) | `:3250-3339` · `:3366-3421` · `:3424-3440` · `:3504-3545` · `:3546-3560` · `:3466-3503` · `:3562-3564` |
| 구체화 뷰(MV) | ✓ `GET_DDL`(`_` 분기) | — | ✓ `CREATE MATERIALIZED VIEW … AS` + `pg_get_viewdef` | — | — | ✗ `no_source`(폴더 있음) | — | `:3341-3351` · `:3424-3440` · `:3470-3487` |
| 프로시저 · 함수 | ✓ `ALL_SOURCE` + 머리 줄 정규화 + `/` | ✓ | ✓ `pg_get_functiondef`(오버로드 = 서명 포함 이름) | ✓ `SHOW CREATE` + `` `db`. `` 한정 | — | ✓ `SYSCAT.ROUTINES.text`(오버로드 = 전부 이어 붙임) | — | `:3198-3213` · `:3441-3462` · `:3471-3476` · `explorer.rs:7030-7038` PG 서명 |
| 패키지 · 패키지 본문 | ✓ 스펙 = 소스 열기 · 본문 = 우클릭 "Open Body" 별도 탭(`NAME.body.sql`) | — | — | — | — | — | — | `:3198-3213` · `explorer.rs:8385-8388` · `:8922` |
| 트리거 | ✓ `ALL_SOURCE` + `ON` 소유자 한정 + DISABLED 보존 | ✓ + 트리거 `ON` 부모 표 한정 · DB 트리거(스키마 없음) · 서버 트리거 | △ 종류 폴더 없음 → 테이블 ▸ Triggers 잎 ▸ Generate SQL ▸ DDL(`source()` 호출 · `ON`·함수 스키마 한정) | ✓ `SHOW CREATE TRIGGER` + `ON` 한정 | ✓ | ✓ `SYSCAT.TRIGGERS.text` | — | `:3214-3248` · `:3399-3418` · `:3353-3365` · `gen.rs:1670-1672` · `lib.rs:376-452` |
| 타입 | ✓ `ALL_SOURCE`(TYPE · 본문은 미분리 — 추정) | ✗ 추정(`OBJECT_DEFINITION`이 NULL → "object not found") | ✗ `no_source`(DDL 생성은 ✓ enum/composite) | — | — | — | — | `:3198-3213` · `:3380-3421` · `:3463` · `gen.rs:1091-1116` |
| 시퀀스 · 인덱스 · 동의어 · DB 링크 · 잡 | 소스 메뉴 없음(`has_source` 아님) → DDL 생성 ✓ | 소스 메뉴 없음 → 시퀀스·인덱스 DDL ✓ · 동의어 ✗ 추정 | 시퀀스·인덱스 DDL ✓ · Aggregate ✗ | — | 인덱스 DDL ✓ | ✗ | — | `lib.rs:268-287` · `gen.rs:1059-1170` |
| 테이블 | 소스 메뉴 없음 → DDL 생성(§2-1 ③) | 같음 | 같음 | 같음(`SHOW CREATE TABLE`은 `source()`에 있으나 탐색기 경로에서 안 쓰임) | 같음 | 같음 | 같음(골격) | `gen.rs:1172` · `lib.rs:3504-3508` |
| 연결된 서버 · 서버 트리거(SQL Server 서버 수준) | — | ✓ 재생성 스크립트 · `sys.server_sql_modules` | — | — | — | — | — | `:3367-3380` |


### 6-3. 자동 시험 근거(종류 × 방언 왕복 · 11-03 재실행 없음 · 문서·스크립트 머리 기록)

| 시험 | 범위 | 결과(문서 기록) |
|---|---|---|
| `scripts/oracle-ddl-e2e.sh` | Oracle 전 유형(테이블·PK·UK·CHECK·FK·인덱스·코멘트·시퀀스·ALTER·뷰·함수·프로시저·패키지 스펙+본문·트리거·타입·시노님·MV) · 소스 왕복 원문 일치 6종 · 컴파일 오류 보고 · DDL 생성 | 67/67 (docs/100 §4) |
| `scripts/dbms-source-e2e.sh` | PostgreSQL · SQL Server 소스 열기 보수적 한정 왕복(임시 객체 → `nsql cat source` → 형태 검사 → 재실행) | 33/33 (docs/100 §5-3) |
| `scripts/mac-grid-edit-e2e.sh` | 그리드 편집 SQLite 8항목 + Oracle/PG/MSSQL 실서버(PK 표 · 키 없는 표 · DATE · CLOB 40,000자) | 통과(docs/87 §11) |
| `scripts/win-objbackup-e2e.sh` · `win-objbackup-prod-e2e.sh` | 객체 백업 SQLite 7항목 · Oracle PRD 모드(REPLACE·DROP·탐색기 삭제) | 통과 기록(스크립트 머리) |
| `scripts/win-drop-prod-e2e.sh` | 탐색기 삭제 PRD/none 대조(SQLite) | 통과 기록 |
| `scripts/pkg-overload-e2e.sh` | Oracle 패키지 오버로드 멤버 · Generate SQL ▸ Call | 통과 기록 |
| `crates/nsql-drivers/tests/integration.rs:1916 · :2247` | ClickHouse · Db2 스크립트 DDL/DML(카탈로그 생성기가 아닌 실행 경로) | 통합 45/0(11-03 게이트 ②) |
| MySQL 실서버 왕복 | **없음**(프로필 없음 · 순수 함수 시험 `conservative_qualification_helpers`만) | T-268 남음 |


### 6-4. 표 2 — 사용 방법(어디서 · 조작 · 기동 명령 · 설정 키 = `nsql-settings` REGISTRY 이름 그대로)

| 기능 | 어디서 | 조작 · 단축키 · 메뉴 · 우클릭 | 기동 명령(`NSQL_STARTUP_CMD`) | 관련 설정 키(`nsql-settings` REGISTRY 그대로) |
|---|---|---|---|---|
| 소스 열기 | 객체 탐색기 객체 노드 | 더블클릭/Enter(테이블·뷰는 SELECT 템플릿 탭 · 소스 있는 종류는 소스 탭 · `explorer.rs:6476-6495`) · 우클릭 ▸ **Open Source** · 패키지 ▸ **Open Body** · 팔레트 `obj.source` | `explorer.expand:<라벨|행>` · `explorer.select` · `explorer.selpath` · `explorer.menu:<행>` + `explorer.pick:source` / `pick:body` · `editor.dump:<파일>` | `explorer.source_schema`(on · 머리 줄 스키마 한정) · `explorer.timeout` · `explorer.details` · `explorer.mssql_tree` · `explorer.show_system_schemas` |
| 소스 탭 편집 → 컴파일/실행 | 소스 탭(제목 `이름.sql` · 패키지 본문 `이름.body.sql`) | **F5**(`run.all` · 본문 전체 한 단위) · Ctrl+Enter(`run.statement` · 캐럿 문장만) · Output 탭 **Ctrl+Shift+O**(`view.output`) · Output 오류 줄 두 번 클릭 = 그 줄로 · 탭 우클릭 "객체 탐색기에서 보기"(`app/tabobj.rs`) | `output.dump:<파일>` · `editor.caret` · `editor.dump` | `output.show`(auto) · `output.activate`(no_results) · `output.done_lines` · `output.timestamps` · `output.max_lines` · `output.serveroutput`(Oracle `SET SERVEROUTPUT ON`) · `backup.object_replace`(F5 재생성 전 백업) · `tx.prod_*`(운영 2단 확인) |
| DDL 생성 | 탐색기 객체/잎(제약·인덱스·트리거) | 우클릭 ▸ **Generate SQL ▸ DDL**(테이블 = SELECT/INSERT/UPDATE/DELETE/MERGE + DDL · 루틴·패키지 = CALL + DDL · 시퀀스 = SELECT + DDL) → SQL Preview 모달(옵션 체크 · 새로고침 · 파일로 저장 · **편집기에서 열기** · 복사 · Esc) · CLI `nsql cat gen ddl <객체> [kind]` | `explorer.menu:<행>` + `explorer.pick:gen:ddl` · `sqlprev.dump:<파일>` | `gen.qualified`(on) · `gen.compact` · `gen.full_ddl` · `gen.separate_fk` · `gen.format` · `gen.bind_note` |
| 테이블 데이터 편집 | 결과 그리드(단일 테이블 SELECT 결과 · 읽기 전용 사유는 상태줄 `gridedit_sql.rs:12-32`) | 그리드 포커스에서 **F2** 셀 편집 시작 · **Ctrl+D** 행 복제 · **Ctrl+;** 지금 날짜 · **Ctrl+Shift+;** 지금 시간 · 우클릭 메뉴 = 값 보기 · NULL · 빈 문자열 · 지금 · 복제 · 행 추가 · 행 삭제 · 되돌리기/다시 · 변경 목록 · **SQL 미리보기** · **적용** · 되돌림(`grid.rs:5768-5852`) · 적용 결과 = 토스트 + 상태줄 + 트랜잭션 로그 창(`view.txlog`) | `grid.edit.cmd:<id>` · `grid.edit.set:<행>/<열>/<값>` · `grid.edit.load:<파일>` · `grid.dump` · `txlog.dump` | `grid.edit`(on) · `grid.edit_empty`(null) · `grid.edit_refresh`(rows) · `grid.edit_hidden_keys`(off) · `grid.edit_rowid`(off) · `grid.edit_all_cols`(on) · `grid.edit_concurrency`(key) · `grid.lob_view_max_mb` · `grid.paste_max_rows` · `tx.prod_hold*` · `login.*` 운영 유형 |
| 객체 메타 보기 | 탐색기 하위 폴더 + 객체 상세 패널(탐색기 아래) | View ▸ **Object Details**(`view.object_details`) · 객체 클릭 = 상세 · 머리 줄 ▾/▴ 축소 · 복사(Shift+클릭 = 종류-이름-설명) · 머리 줄 더블클릭 = 편집기 탭 · **Shift+F4** = ℹ 정보 탭(`obj.info`) · **F4** = 캐럿 아래 객체를 탐색기에서 찾기(`obj.reveal`) · Ctrl 홀드 = 객체 링크 툴팁 · CLI `nsql cat detail` | `details.dump:<파일>` · `objlink.reveal` · `objlink.dump` | `explorer.details` · `explorer.details_h` · `explorer.details_collapsed` · `explorer.tooltip` · `meta.warm_comments` · `objlink.*` |
| 객체 소스 탭 북마크 | 소스 탭 거터 | **Ctrl+F2**(또는 Ctrl+K,Ctrl+K) 토글 · 거터 우클릭 · 북마크 패널(`view.bookmarks`) 클릭 = 소스 다시 열고 그 줄(그 서버의 살아 있는 탐색기 칸이 있을 때만 · 자동 접속 없음) | `bm.open` · `bm.rows` · `bm.stat` | `bookmark.enabled` · `bookmark.persist` · `bookmark.stale_days` · `bookmark.save_copy_on_save` |
| 객체 백업 | 자동(편집기 F5·실행 · 탐색기 삭제 직전) | 상태바 저장 섹션 클릭 ▸ **객체 백업 위치 열기**(`app/menus.rs:111` · `<설정>/backups/objects/`) · 파일명 `<프로필>_<스키마>.<객체>_<종류>.<yyyyMMddHHmmssSSS>` · 머리 주석 `action:` | `objbk.dump:<파일>` | `backup.object_drop`(on) · `backup.object_replace`(on) · `backup.object_truncate`(on) · `backup.object_days`(30) · `backup.object_keep`(0 = 무제한) · `backup.prune_interval_min`(60 · HIDDEN) |
| 객체 삭제 | 탐색기 객체 노드 | 우클릭 ▸ **Delete…** → 모달(정보 + 2단 타임아웃 버튼) → 백업 → DROP(Output · 트랜잭션 로그 · 탐색기 그 폴더 갱신) · 백업 실패 시 운영 = 막음 · 그 밖 = "백업 없이 삭제" 재확인 | `explorer.menu` + `explorer.pick:drop` · `drop.confirm` · `drop.confirm_nobackup` · `drop.dump` | `login.delete_confirm_ms` · `backup.object_drop` |
| CREATE 문 ↔ 서버 비교 | 편집기 `CREATE …` 문장 | 팔레트 **`obj.compare`**(비교 탭 · 거터 띠 · "일치/N군데 다름") · **`obj.compare_server`**(서버 DDL 탭) | `compare.dump` | `gen.qualified`(한정 맞춤) |


### 6-5. 미구현 · 제한 목록(TODO 번호)

| # | 항목 | 상태 | 관련 TODO / 문서 |
|---|---|---|---|
| 1 | **테이블 구조 편집기(디자이너 · 컬럼 추가/변경/삭제 폼 · ALTER 미리보기 · 라이선스 안내 게이트)** | 📐 **설계만** — 소스 없음(`crates/` 안에 `nsql-ddl`·`objedit` 없음) | docs/119 §0 "상태 = 설계만 · D-316~326 대기 · 미착수(D-439)" · TODO 번호 = 착수 때 부여(T-382 후보) |
| 2 | **권한(GRANT/REVOKE) 보기·편집 · 권한 폴더** | ✗ 없음(`explorer.rs`·`nsql-catalog`에 GRANT 코드 0건 · SQL Server 로그인/역할은 속성 보기·삭제만) | T-206(권한 폴더 · 대기) · docs/83 §5 |
| 3 | **객체 이름 바꾸기(RENAME) UI** | ✗ 없음 — 편집기에서 `RENAME`/`ALTER … RENAME`을 직접 실행(백업·탐색기 갱신·북마크 무효화는 됨 `app/objbackup.rs:27-30` · `app/bookmarks.rs:141`) | T-167 B5 "rename 제안(보류)" · docs/69 §4-2 |
| 4 | **의존성 보기(그래프 · 역참조 전체)** | △ 탐색기 하위 폴더 `Dependencies`는 **Oracle 뷰/MV · PG 뷰/MV · Oracle 테이블**만(`tree.rs:182-237`) + 상세 패널 섹션 · 전체 의존(PG `pg_depend`)·Recycle Bin·트리거 컬럼·큐 테이블 없음 | T-206(대기) |
| 5 | 객체 삭제 후속(여러 객체 일괄 · CASCADE 옵션 · 삭제 뒤 선택 이동 · PG/MSSQL/MySQL 실서버 실기 · func-check 시나리오) | ☐ | T-269 · docs/28 §9 |
| 6 | 컴파일 오류 목록(`compiled · no errors` / 줄·열) = **Oracle만**(`caps.compile_errors` · `lib.rs:3899`) · 컴파일 오류 자리로 캐럿 **자동** 이동 없음(Output 줄 두 번 클릭) | 제한 | T-266(✅ 두 번 클릭) · docs/100 §3-3 |
| 7 | 소스 열기 빈 곳: PG Type/Aggregate/EventTrigger · SQL Server Type/Synonym(추정) · Db2 MV/시퀀스/인덱스/동의어 · DuckDB/ClickHouse/ODBC 뷰 · Db2 소스 한정 없음 | ✗/△ | 등재된 TODO 없음(108 §9 방언 결함 묶음 T-327은 다른 항목) |
| 8 | 테이블 DDL 원문: MySQL/Db2/DuckDB/ClickHouse/ODBC = 컬럼 골격 + 키/인덱스(`SHOW CREATE TABLE` 등 서버 원문 아님) | △ | 등재 없음 |
| 9 | MySQL 실서버 소스 왕복 미검증(프로필 없음 · 순수 함수 시험만) | 🚧 | T-268 |
| 10 | 그리드 편집: DuckDB/Db2/ODBC 실서버 미검증 · ClickHouse 일반 `UPDATE`/`DELETE` 미지원(추정 실패) · DuckDB/ClickHouse/ODBC 수동 커밋 세이브포인트 없음(`caps.syntax.savepoint = None`) · "수정용 실행(1회)" 미개발(D-225) · 값 창 큰 파일 모드 | △/☐ | T-182 남은 것(docs/87 §11 · §15) |
| 11 | 상세 패널 후속(섹션 접기/탭 · LRU · 컬럼 → 주인 요약) | 🚧 | T-225 · docs/86 §8 |
| 12 | CREATE ↔ 서버 비교 2단계(diff 뷰어 부품 · 맞은편 줄) | ☐ | T-283 2단계 · docs/19 §6-2 |
| 13 | 객체 소스 탭 후속(탭 줄 아이콘 `TabKind::Object` · F5 뒤 탐색기 VALID/INVALID 즉시 갱신 확인 · hover 차단 확산) | 🚧 | T-267 · docs/100 §3-3 |
| 14 | 객체 찾기 창 2단계(서버측 정규식 본문 검색 · 데이터 값 검색) · 실서버 대조 | ☐ | T-354 · T-355 |
| 15 | 메타 수집 계정(둘째 계정의 권한 스키마) | ☐ | T-264 |


### 6-6. 확인한 사실 vs 추정(사용자 안내문에는 "추정" 항목을 "확인 필요"로)

**확인한 사실(소스 줄을 읽음)**
- 소스 열기 종류·방언 분기 = `nsql-catalog/src/lib.rs:3188-3588` 전체 읽음 · 메뉴 노출 조건 `has_source` = `:268-287` · 탐색기 더블클릭 분기 = `explorer.rs:6476-6495` · 우클릭 메뉴 구성 = `explorer.rs:8360-8445`.
- DDL 생성 분기 = `gen.rs:1058-1172`(`object_ddl`) · `:1630-1720`(`sub_ddl`) · `:296-336`(`gen_whats`) · `:957-1056`(Oracle 타입 표 · `plsql_terminate` · `GET_DDL`).
- F5 한 단위 실행 + 스키마 불일치 경고 = `app/run.rs:152-187` · 컴파일 검사 = `nsql-run/src/lib.rs:3501-3560` · Oracle 한정 = `nsql-catalog/src/lib.rs:3899`.
- 데이터 보호 세 겹 = `nsql-run/src/lib.rs:1634-1785` 읽음(① 사전 검사 `n != Some(1)` → `ApplyPhase::PreCheck` ② `rows_affected != 1` → `ApplyPhase::Execute` ③ 자동 = `rollback()` · 수동 = `ROLLBACK TO SAVEPOINT` · `rollback_needed`) · 사전 검사문 생성 = `gridedit_sql.rs:860-863` · 세이브포인트 방언 표 = `caps.rs:210-312`.
- 객체 백업 흐름·설정 키 = `app/objbackup.rs:1-48 · 259-302` · 폴더 = `backups.rs:69` · 상태바 메뉴 항목 = `app/menus.rs:111`.
- 삭제 흐름 = `app/drop.rs:1-60` · DROP 문 종류 표 = `lib.rs:2890-2976`.
- 북마크 문서 열쇠 = `nsql-bookmarks/src/lib.rs:17-32` · 재개·무효화 = `app/bookmarks.rs:141 · 310-348 · 642-650`.
- 설정 키 이름·기본값 = `nsql-settings/src/lib.rs:2143-2201`(grid.edit*) · `:3156-3274`(gen.* · backup.object_* · explorer.source_schema) · `:5396-5440`(output.*) · `:4794`(login.delete_confirm_ms) · `:3883-3899`(explorer.details*).
- 단축키 = `keymap.rs:120 · 156 · 163 · 489-494 · 495-542 · 1074-1079`. 기동 명령 이름 = `app/startup_cmd.rs`의 `strip_prefix`/`id ==` 전수 목록.
- 시험 범위 = 각 E2E 스크립트 머리 주석(§2-3) · 통합 시험 함수 이름 `integration.rs:1916 · 2247`.

**추정(실행·실서버로 확인하지 않음)**
- "메뉴는 뜨지만 실패" 판정(DuckDB/ClickHouse/ODBC 뷰 · PG Type/Aggregate/EventTrigger · SQL Server Type/Synonym)은 코드 경로(`no_source` · `OBJECT_DEFINITION` NULL)로 추론 — 실제 오류 문구·토스트는 미확인.
- MySQL 하위 항목(제약·인덱스·트리거 잎) DDL = `sub_ddl`에 MySQL 분기가 없어 오류로 추론(`gen.rs:1630-1720`에 `Dialect::Mysql` 없음 · 뒤쪽 `_` 분기는 읽지 않음).
- ClickHouse 그리드 편집 실패 = 서버 문법(`ALTER TABLE … UPDATE`) 지식에 따른 추정 · 코드에 ClickHouse 편집 차단은 없음.
- Oracle TYPE BODY 분리 여부 · SchemaTrigger(`GET_DDL('TRIGGER')`) 소스 열기 결과 형태는 미확인.
- 실행 오류 캐럿 이동의 MySQL/DuckDB/Db2/ClickHouse/ODBC 동작은 E2E 대상이 아니라 "추정".
- E2E "통과" 수치는 문서(docs/100 §4·§5-3 · docs/87 §11)와 스크립트 머리 기록을 옮긴 것 — 이번 조사에서 재실행하지 않음(11-03 릴리스 게이트 ①~⑥ 통과는 journal 기록).
