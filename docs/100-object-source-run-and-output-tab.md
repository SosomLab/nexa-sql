# 100. 객체 소스 탭의 F5 = 객체 단위 실행 · Output 탭 · 소스 열기 vs DDL 생성(09-30 · 108차 win)

> 사용자 요구(09-30 저녁): ① 소스 열기와 DDL 생성의 차이 분석 ② 소스 열기 뒤 F5가 프로시저를 CREATE하지 않고 줄 단위로 도는 것 →
> 객체(뷰·패키지·함수·프로시저 …)에서 다르게 동작 ③ 프로시저 편집에서는 결과 그리드보다 "컴파일이 잘 되었는지" 같은 **메시지**가 더
> 뜻이 있다 → 일반 편집 탭에서도 프로시저 실행·PRINT 메시지를 보는 **Output 탭**(처음엔 없음 · 보기 설정 · 메시지가 오면 자동 · 다른
> 클라이언트의 기본 기능) ④ BISCM·BISCM_SB의 `SP_M4P_MP_VERSION_CREATE_BSY`를 DDL/소스 열기로 열어 실제 내용과 일치 확인 ⑤ 객체 선언·
> 테이블·인덱스·PK/UK·제약 등 **전 유형 DDL 전수 테스트**(biscm 자동 접속 · Debug 직접 실행).

## 1. 소스 열기(Open source) vs Generate SQL ▸ DDL — 차이 분석

| | 소스 열기 | Generate SQL ▸ DDL |
|---|---|---|
| 진입 | 탐색기 더블클릭/Enter · 우클릭 ▸ 소스 열기(패키지는 Body 항목 따로) | 우클릭 ▸ SQL 생성 ▸ DDL |
| 원천 함수 | `nsql_catalog::source` | `nsql_catalog::generate(GenWhat::Ddl)` → `object_ddl` |
| Oracle PL/SQL(프로시저·함수·패키지·본문·트리거·타입) | `ALL_SOURCE` 줄을 이어 붙이고 앞에 `CREATE OR REPLACE `, 끝에 `/` 한 줄 | `DBMS_METADATA.GET_DDL` → `plsql_terminate`(블록마다 `/` · 빈 줄) |
| 이름 표기 | ~~사용자가 쓴 그대로(스키마 없음 · `PROCEDURE          SP_X`)~~ → **09-30 저녁: `PROCEDURE BISCM_SB.SP_X`**(소유 스키마 한정 · 공백 1칸 · `explorer.source_schema` 기본 켬 · `normalize_plsql_header`) | Oracle 표기 `"BISCM"."SP_X"` + `NONEDITIONABLE` 등 수식어 |
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
- Output 첫 줄 `▶ PROCEDURE BISCM.NSQLT_P2 — 소스 전체를 한 단위로 실행(F5)` · 상태줄 `… 한 단위로 실행 중`.
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

## 4. 실기(09-30 · Debug · biscm 자동 접속 · 키 주입 0)

- **`SP_M4P_MP_VERSION_CREATE_BSY` BISCM(69줄) · BISCM_SB(495줄)**: CLI `cat source`/`cat gen ddl` = GUI 소스 열기/DDL 미리보기 덤프와 **글자 단위 일치** ·
  소스 열기 = `CREATE OR REPLACE ` + `ALL_SOURCE`(68/494줄) + `/` 일치 · DDL 본문(2행~) = `ALL_SOURCE`(2행~) 일치(머리 줄만 `NONEDITIONABLE "BISCM"."…"`) ·
  스키마별로 다른 판이 맞게 들어온다(둘 다 서버 상태 INVALID = 탐색기 `object!`).
- **전 유형 DDL E2E `scripts/oracle-ddl-e2e.sh -o <폴더> -n target/debug/nsql.exe -p biscm`** = **67/67 통과**(임시 객체 `NSQLT_*` · 끝에 0개 남김): 테이블 ×2 · PK · UK · CHECK ·
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

