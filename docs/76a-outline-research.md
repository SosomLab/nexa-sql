# 76a — 아웃라인(Outline/Structure) 조사 원문 (10-24 · 138차 Linux)

> [76 §20 아웃라인 방향](76-intellisense-and-outline.md)의 근거 자료. 협업 세션(Opus)이 하위 에이전트 2개로 조사 = ① 타 제품(공식 문서·소스 · 출처 URL · 직접 확인 못 한 것은 **(추정)**) ② nexa-sql 코드 현황(조회만 · file:line · 노드 수는 **추정(규칙 모사)**). 설계 결론은 76 §20(D-357)이 SSOT이고 이 문서는 고치지 않는다.


## 결론
- **타 제품 공통 = 문장 단위 1단 목록 또는 "선언된 이름"(CREATE 대상·프로시저·함수·PL/SQL 선언)만** — DataGrip = 문장 목록 + 종류 토글(DDL/DML/Select/Other) + Group by type + 가나다/파일 순 정렬 + 커서 추종 옵션 · Sublime = CREATE 이름·함수만 · PL/SQL Developer·Toad·SQL Developer = PL/SQL 프로그램 단위만 · SSMS·VS Code 기본·TablePlus·Beekeeper·Golden = 개요 창 없음. 절·컬럼·별칭까지 펼치는 곳은 **DBeaver뿐**(3단 자동 펼침).
- **nexa-sql = 문장 행이 45~90%**(SQL*Plus 명령 줄까지 1행씩) + `CREATE` 같은 줄 2행 중복 + 프로시저 매개변수를 변수 선언으로 오탐 + 평면 들여쓰기 목록(접기·종류 거르기·커서 추종·아이콘 없음 · `outline.*` 설정 0개) → "너무 많이 보임"의 원인.
- 방향 후보(개발 판단용 · 협업 의견) = ① DataGrip식 **종류 토글**(문장/명령/객체/선언 · 기본 = 객체·루틴·CTE만?) ② CREATE 문장 행 생략(대상 행만) ③ Declared 매개변수 오탐 수정 ④ 커서 추종 ⑤ 진짜 트리 + 접기 — 기본값을 줄이는 것이 사용자 불편에 가장 직접.

---
# ① 타 제품(하위 에이전트 조사 · 출처 URL·추정 표시 포함)
## (b) 비교표

| 제품 | 대상 | 노드 종류·계층 | 기본 접힘 | 표시(아이콘·줄·필터·정렬·추종) | 클릭 | 근거(URL) |
|---|---|---|---|---|---|---|
| **DataGrip** | .sql 파일·콘솔 · 데이터 편집기(표)도 있음 | 1단 = 파일 안의 **문장**(DDL/DML/SELECT/Other). "Group by type"을 켜면 종류 묶음 아래 문장이 오는 2단. 문장 아래 하위 노드(CREATE TABLE의 컬럼 등)는 **(추정)**. 데이터 편집기에서는 **표의 컬럼 목록**(숨김/표시·정렬) | 문서 언급 없음 · Expand All/Collapse All 버튼 있음 | 아이콘(객체 종류) · 줄 번호 없음(추정) · **타자로 거르기**(CamelHumps · 팝업은 "Narrow down on typing") · **Sort Alphabetically**(끄면 소스 순서) · **Show 토글 = DDL / DML / Select / Other** · **Always Select Opened Element**(커서 추종) · VCS 변경은 파랑/초록 | **Navigate with Single Click**(한 번 클릭으로 이동하는 옵션) · 팝업은 Enter/클릭으로 이동 · 우클릭 **Execute**로 선택한 문장 실행 | https://www.jetbrains.com/help/datagrip/2021.1/structure-tool-window-file-structure-popup.html · https://www.jetbrains.com/help/datagrip/viewing-structure-of-a-source-file.html · https://www.jetbrains.com/help/datagrip/2025.2/code-structure.html |
| **DBeaver** (24.0+) | SQL 편집기 스크립트(의미 분석을 켰을 때만) | **깊은 의미 트리**(소스 확인): 스크립트 → 문장 미리보기 → `SELECT (컬럼들)` · `INTO` · `FROM` · `WHERE` · `GROUP BY` · `HAVING` · `ORDER BY` → 테이블 참조·별칭·JOIN(`ON`/`USING`/NATURAL/CROSS) · CTE·서브쿼리 · 함수·프로시저 호출 · 변수 · 컬럼(타입 아이콘) · INSERT/UPDATE(SET)/DELETE 하위 · CREATE/ALTER TABLE의 컬럼·제약. 사실상 무제한 깊이 | **3단까지 자동 펼침**(`setAutoExpandLevel(3)`) | 노드마다 아이콘(테이블·컬럼·함수·필터·정렬·값 타입별) · 줄 번호 없음 · 필터/정렬 도구 **없음**(소스에 액션 없음) · **커서 추종 항상**(caret 리스너) · 의미 분석이 꺼져 있으면 안내 노드 | 클릭하면 해당 구간을 **선택하고 그 위치로 스크롤**(`selectAndReveal`) | https://dbeaver.com/docs/dbeaver/SQL-Editor/ · 소스 https://github.com/dbeaver/dbeaver/blob/devel/plugins/org.jkiss.dbeaver.ui.editors.sql/src/org/jkiss/dbeaver/ui/editors/sql/semantics/SQLEditorOutlinePage.java · https://dbeaver.io/2024/02/ |
| **Oracle SQL Developer** | PL/SQL 편집기(패키지·프로시저). 워크시트 지원은 **(추정)** 제한적 | 패키지 → 서브프로그램(→ 선언) 2~3단 **(추정)** · "Code Outline"은 파서 기반이라 구문 오류가 있으면 "Partial parse tree"가 된다 | 확인 못 함 | 확인 못 함 · 사용자 보고: 복잡한 패키지에서는 내부 커서·로컬 서브프로그램이 빠짐(VS Code판) | 이동 **(추정)** | https://community.oracle.com/mosc/discussion/4527627/sql-developer-code-outline-error · https://forums.oracle.com/ords/apexds/post/improvement-in-outline-view-6595 · https://forums.oracle.com/ords/apexds/post/code-outline-support-3131 |
| **PL/SQL Developer** | Program Window(패키지·타입 본문)가 주 대상 · SQL Window에도 같은 편집기 기능이 있다(추정) | **Code Contents** = 함수·프로시저·변수·상수·타입·매개변수·예외(프로그램 단위 구조 2~3단) · 위쪽에 **Code Hierarchy**(현재 위치의 경로 표시) | 확인 못 함 | 트리 · 정렬·추종은 확인 못 함 | 클릭하면 이동 · 우클릭으로 select/delete/execute/test | https://www.allroundautomations.com/?p=4898 · https://www.allroundautomations.com/?p=1779 |
| **Toad for Oracle** | Editor의 **Navigator** 패널 · PL/SQL 탭이 중심(SQL 탭과 레이아웃이 따로이며, .sql에서 숨긴다는 사용자 보고) | 패키지 → **Local Subprograms** 등 PL/SQL 요소 2~3단 · SQL 스크립트의 문장 목록은 **(추정)** | 확인 못 함 | **"Sort PL/SQL elements"**(켜면 가나다순, 끄면 소스 순서) · 왼쪽 도킹이 기본 | 이동 **(추정)** | https://support.quest.com/kb/4229370/how-to-sort-procedures-in-the-navigator-or-schema-browser-panel- · https://forums.toadworld.com/t/navigator-panel/38994 · https://support.quest.com/technical-documents/doc1684993 |
| **SSMS** | 없음. 가장 가까운 것 = **Code Outlining(접기)** | 접기 구역 = ① 배치(`GO`) ② `BEGIN…END`/TRY/CATCH ③ 여러 줄 문장(한 줄 문장은 제외) | 모두 펼침 | 여백의 +/− · 접힌 구역에 툴팁 · 설정 "Outline statements" | 접기/펼치기만 | https://learn.microsoft.com/en-us/ssms/scripting/code-outlining |
| **VS Code** | 기본 SQL·SQLTools: **없음**("No symbols found in document" · 기능 요청 #1070이 2022년부터 열려 있음) · mssql: 저장소에 DocumentSymbolProvider가 없음(**없음 추정**) · Oracle "Language PL/SQL" 계열: 서브프로그램 개요 + 브레드크럼 | 확장마다 다름 · PL/SQL 확장 = 프로시저/함수 1~2단 | Outline 뷰 기본(추정) | VS Code Outline 공통 기능: 정렬(위치/이름/종류) · 커서 추종 · 거르기 | 이동 | https://github.com/mtxr/vscode-sqltools/issues/1070 · https://code.visualstudio.com/docs/languages/tsql · https://forums.oracle.com/ords/apexds/post/code-outline-support-3131 |
| **Sublime Text** | Goto Symbol(Ctrl+R)·색인 기호(Ctrl+Shift+R) | **평평한 1단** = CREATE 대상 이름만(`entity.name.struct` = table·index·database·partition·other) · 함수/프로시저(`entity.name.function`) · 타입. 일반 SELECT·DML은 **안 나온다** | 해당 없음(목록) | 퍼지 검색 · 줄 위치 · 정렬 없음(파일 순서) | 고르면 그 줄로 이동(목록을 움직이는 동안 미리보기) | https://github.com/sublimehq/Packages/tree/master/SQL (Symbol List.tmPreferences · SQL (basic).sublime-syntax) |
| **TablePlus · Beekeeper Studio · Golden(Benthic)** | **없음**(추정 — 기능 목록에 없음) · Beekeeper는 비교표에서 code outlining·Document Outline이 "없음"으로 나온다 | — | — | — | — | https://www.beekeeperstudio.io/features/sql-editor · https://devart.com/dbforge/edge/beekeeper-studio-alternatives.html |
| (참고) dbForge | **Document Outline** 창(긴 스크립트 이동용) | 확인 못 함 | — | — | — | https://devart.com/dbforge/edge/beekeeper-studio-alternatives.html |

## (c) DataGrip 상세

- 열기: **Structure 도구 창**(Alt+7, 고정 패널) / **File Structure 팝업**(Ctrl+F12 · Navigate ▸ File Structure). 팝업은 정보가 적은 대신 이동이 빠르다.
- 기본으로는 "현재 파일의 모든 객체·쿼리·기타 요소"를 보여 주고, 도구줄 토글로 줄인다. **Show = DDL statements / DML statements / Select statements / Other**(종류별로 켜고 끔). **Group by type**을 켜면 종류 묶음 노드가 생긴다(문서 예: tables·views·procedures).
- 정렬: **Sort Alphabetically** 하나뿐이고, 끄면 파일 순서다.
- 추종·이동: **Always Select Opened Element**(편집기 커서 위치의 노드를 선택) · **Navigate with Single Click**(한 번 클릭으로 소스 이동) · Expand All / Collapse All.
- 검색: 창에서 바로 타자를 치면 speed search(CamelHumps). 팝업은 "Narrow down on typing"을 켜면 맞지 않는 항목을 숨기고, 끄면 강조만 한다.
- 작업: 문장을 선택하고 우클릭 **Execute**로 실행. VCS에서 바뀐 요소는 파랑, 새 요소는 초록.
- 데이터 편집기(결과·표)의 Structure = **컬럼 목록**(숨김/표시·정렬). 결과 그리드에서도 Ctrl+F12 뒤 타자로 컬럼을 거른다.
- 문서에는 문장 아래 하위 노드(서브쿼리·CTE·별칭·컬럼)가 **언급되지 않는다**. 노드 단위가 "문장"이라는 점은 필터 체계로 확인된다.

## (d) 추정 항목

1. DataGrip: 문장 노드 아래 하위 노드(CREATE TABLE의 컬럼·키, 루틴 본문의 변수 등)가 있는지와 그 깊이 · 기본 접힘 상태 · 줄 번호를 표시하지 않음.
2. Oracle SQL Developer(데스크톱): Code Outline/Outline의 계층(패키지 → 서브프로그램 → 선언) · 워크시트(일반 SQL) 지원 범위 · 클릭 동작. 공식 문서를 찾지 못했고 포럼 글만 확인했다(일부는 응답 시간 초과로 본문을 열지 못함).
3. PL/SQL Developer: SQL Window에도 Code Contents가 나오는지("SQL 편집기 = PL/SQL 편집기 기능"이라는 판매처 문구에서 추론) · 정렬·추종.
4. Toad: Navigator가 SQL 스크립트를 문장 목록으로 보여 주는지 · 클릭 동작.
5. VS Code: mssql 확장(언어 서버 쪽 포함)에 문서 기호가 없다는 점(저장소 코드 검색 0건 근거) · Outline 뷰의 기본 펼침.
6. Sublime: `entity.name.function`이 기본 기호 목록에 들어간다는 점(Default 패키지의 전역 규칙으로 추론 · SQL 패키지 자체는 `entity.name.struct`만 등록).
7. TablePlus·Golden: 개요 기능 없음(기능 목록에 없다는 데서 추론).

---
# ② nexa-sql 현황(하위 에이전트 코드 탐색 · 조회만 · file:line)
## 1. 노드 종류와 동작

### 1-1. 노드 종류(`SymKind` · OL:16-38 · 라벨 OL:43-60)
| 종류 | 라벨 | 만드는 규칙(근거) | depth | 패널 표시(OP:595-627) |
|---|---|---|---|---|
| Statement | statement | 모든 SQL 항목 + Define/Variable이 아닌 명령(SET·SHOW·EXEC·PRINT…) 1개씩 · 이름 = 문장 첫 60자(OL:338-358) · detail = select/dml/ddl/block/sql/command(OL:409-434) | 0 | 흐린 글자 |
| Define | define | `DEFINE/DEF name [= v]`(OL:392-400) | 0 | 본문색 |
| Variable | variable | `VARIABLE/VAR name TYPE`(OL:402-407) | 0 | 본문색 |
| Cte | cte | `WITH [RECURSIVE] a [(cols)] AS (…), b AS (…)`(OL:496-552) | 1+pkg | 본문색 |
| Object | object(detail=table/view/index/sequence/synonym/schema/database) | `CREATE [수식어] KIND name`(OL:265-281, 467-494) | 1 | 본문색 |
| Procedure / Function | procedure / function | `CREATE PROCEDURE/FUNCTION` 또는 블록·패키지 안·문장 첫 낱말의 `PROCEDURE x`(OL:554-565) | 1(패키지 안 2) | **굵게 · accent** |
| Package / PackageBody | package / package body | `CREATE PACKAGE [BODY] x`(OL:476-490) → 이후 같은 문장 안 깊이 +1 | 1 | 굵게 · accent |
| Trigger | trigger | `CREATE TRIGGER x` | 1 | 굵게 · accent |
| Type | type(`type body`) | `CREATE TYPE [BODY]` · 블록/패키지 안 `TYPE x IS`(OL:580-588) | 1~2 | 본문색 |
| Cursor | cursor | 어디서든 `CURSOR x`(OL:566-579 · 블록 조건 없음) | 1~2 | 본문색 |
| Label | label | `<<x>>`(OL:589-597) | 1~2 | 본문색 |
| Declared | declared(detail = 타입 요약) | T-SQL `DECLARE @v TYPE`(OL:598-605) · 선언부(DECLARE/IS/AS ~ BEGIN) 줄 첫 낱말 + 다음 낱말이 이름(OL:606-653) | 1~2 | 본문색 |

- **아이콘 없음**: 패널 행 = `줄번호(흐림) · 이름 · 오른쪽 detail/종류 라벨`(OP:595-627). 활동 막대 아이콘 하나뿐(`mi_act_outline` · toolicons.rs:1371 · activity.rs:110).
- **계층 = 진짜 트리가 아님**: `Symbol.depth`(0~2 · OL:74-75) 값만큼 들여쓰기(INDENT 14px · OP:52, 593). 부모-자식 연결 자료 없음 · 평면 `Vec<Symbol>`(OL:80-82). 최대 깊이 2(패키지 안 서브프로그램·선언 · `depth_pkg`는 문장 안에서 0→1만 · OL:453, 488-490).
- CREATE 대상은 **문장 행(depth 0) + 대상 행(depth 1)이 같은 줄에 두 번** 나온다(OL:427-435 뒤 OL:487).
- 접기/펼치기 **없음**(상태·키 0) · 기본 = 전부 펼친 평면 목록.
- 정렬 **없음** = 문서 순서(split 순서 · OL:364-366 · `rebuild`도 순서 유지 OP:213-230).
- 머리 = `Outline (전체 개수)`(OP:556 · 필터와 무관한 `all.len()`).

### 1-2. 필터 · 이동 · 추적
| 항목 | 동작 | 근거 |
|---|---|---|
| 필터 틀 | `FilterBar`(한글 자모 일치 · 이력 키 `filter.outline` 공유) · 대상 문자열 = `"이름 detail 종류라벨"` · 걸러진 **평면** 결과(부모 문맥 유지 안 함) | OP:111-113, 213-225 |
| 필터 키 | ↓/Tab(이력 끝) = 첫 행 선택 · Esc = 검색어 지움 · 필터 포커스 + Enter(선택 없음) = 첫 행 열기 | OP:272-279, 513-529 |
| 타입어헤드 | 목록 포커스에서 글자 = 이름 접두 이동 · ↑↓ 순환 · 설정 `explorer.typeahead*` 공유 | OP:43-45, 315-356 |
| 클릭 | **MouseDown 한 번 = 즉시 이동**(`open = byte`) → `outline_pump` → `goto_byte` = 캐럿 이동 + 편집기 포커스 | OP:424-426 · CP:384-398 |
| 캐럿 추적(follow caret) | **없음** — 캐럿 위치를 패널에 알리는 경로 0 · 선택은 클릭/키로만 | OP 전체(sel 쓰는 곳 = 클릭·키·타입어헤드뿐) |
| 갱신 | 이벤트 루프마다 `outline_sync` → 탭id·`rev`가 바뀌면 **동기 재계산**(디바운스 없음 · 패널 보일 때만) | event_loop.rs:617-618 · CP:358-381 |
| 캐시 | `Intel.docs` 탭별 `(rev, Outline, words)` · 32탭 넘으면 현재 탭만 | IN:166-170, 308-350 |
| 부적합 | `intel.enabled` off · 큰 파일 L1+ · 비SQL 문법 · 이진(NUL/U+FFFD≥8) = 빈 목록 | CP:368-373 · CP:103-127 |

### 1-3. Goto Symbol(Ctrl+R)
- 키 `goto.symbol` = Ctrl+R / mac ⌘R(keymap.rs:311-318). 같은 캐시 `outline_for` 사용(GT:585-614).
- 팔레트 항목 = **아웃라인 전체(Statement 포함)** · id `sym:<byte>` · 라벨 `"  "×depth + 이름 - 종류 · detail :줄`(GT:600-613). Goto Anything(Ctrl+P) `@` 접두도 같은 목록(GT:111, 584).
- 차이: Goto Symbol은 `intel.enabled`를 보지 않는다(`intel_unsuitable`만 · GT:587) — 패널은 봄(CP:368).
- 완성 후보는 `Outline::names()` = Statement 제외 · 중복 제거(OL:96-107 · IN:498-505).

## 2. 설정 키
`outline.*` 키는 **하나도 없다**(ST grep 0). 아웃라인에 영향 주는 키:
| 키 | 기본 | 의미 | 근거 |
|---|---|---|---|
| `intel.enabled` | on | off면 패널 = 빈 목록(Goto Symbol은 영향 없음) | ST:6153 · CP:368 |
| `intel.max_doc_kb` | 1024 | 넘으면 **완성의** 문서 심볼·낱말만 끔(패널·Goto Symbol은 무관) | ST:6322 · IN:468, 494-497 |
| `file.large_l1_mb` / `_l1_lines` | 5 / 100000 | L1 이상 = 아웃라인 빈 목록 | ST:3912, 3920 · CP:104-108 |
| `file.large_l2_mb` / `_l2_lines` | 20 / 300000 | L2(동일하게 끔) | ST:3931, 3939 |
| `explorer.typeahead` | on | 아웃라인 타입어헤드 켬 | ST:3176 · settings.rs:602 |
| `explorer.typeahead_timeout_ms` | 2000 | 접두 유지 시간 | ST:3184 |
| `explorer.typeahead_space` / `_special` | on / on | 공백·특수 글자 허용 | ST:3195, 3203 |
| `explorer.typeahead_pos` | bottom_left | HUD 위치 | ST:3211 |
| 키맵 `goto.symbol` | ctrl+r | Goto Symbol | keymap.rs:311-318 |
| 상수(설정 아님) | ROW_H 22 · INDENT 14 · head 60자 · 캐시 32탭 | 하드코딩 | OP:49-52 · OL:352 · IN:345-347 |

## 3. docs/76 설계 항목 대조
| 설계 항목(76) | 상태 | 근거 |
|---|---|---|
| 심볼 종류(문장·DEFINE/VARIABLE·CTE·CREATE·PL/SQL 7종·선언 변수·`DECLARE @v`)(§0-3) | 구현됨 | OL:16-38, 389-653 |
| 문자열·주석 안 무시 | 구현됨 | OL:129-136 |
| 탭별 캐시 · 본문 세대 열쇠(D-203) | 구현됨(위치는 `Editors`가 아니라 `Intel.docs`) | IN:166-170, 308-350 |
| "유휴 때 재계산"(§0-3 · §4 · D-203) | 부분 — 유휴 디바운스 없이 루프마다 rev 비교 후 즉시 동기 재계산 | event_loop.rs:617-618 · CP:358-381 |
| 큰 파일 L1+ 끔(D-203) | 구현됨 | CP:104-108, 368-372 |
| 패널 = 활동 막대 `view.outline`(D-204) | 구현됨 | activity.rs:110 · menus.rs:827-838 |
| 필터 틀(한글 자모) | 구현됨 | OP:213-225 |
| 클릭/Enter = 이동 | 구현됨 | OP:424-426, 513-521 · CP:384-398 |
| "문장/선언/서브프로그램 깊이"(§3) | 부분 — 들여쓰기만 · 트리·접기 없음 | OP:593 · OL:74-75 |
| Goto Symbol 팔레트 Ctrl+R(§3 · §5 · D-204) | 구현됨 | keymap.rs:311-318 · GT:585-614 |
| 완성 후보에 문서 심볼(D-200) | 구현됨 | OL:96-107 · IN:494-505 |
| JetBrains Structure / File Structure 팝업에서 **문장 단위 실행**(§0-2) | 안 됨(이동만) | OP·GT에 실행 경로 0 |
| 종류 아이콘(§3 "표시"는 완성 팝업 대상) | 아웃라인은 안 됨(텍스트 라벨만) | OP:595-627 |
| 캐럿 위치 추적(SQL Developer·DataGrip 공통 · §1 표) | 안 됨 | OP 전체 |
| 정렬·종류별 숨김 옵션 | 설계에도 없음 · 안 됨 | — |
| 자동 점검 S66~S68 · 실기 U-78~80(§6) | 문서상 ✅(본 조사에서 실행 안 함) | docs/76:78 |

## 4. 실제 예제 노드 수 — **추정(규칙 모사)**
`examples/*.sql` 상위 2개는 작아서(4.9 KB·3.0 KB) `examples/lob/` 최대 2개도 함께. 도구 = 스크래치패드 `outline_mimic.py`(split.rs·outline.rs 규칙 단순 모사 · Oracle `/`·T-SQL `GO` · 명령 낱말 표 command.rs:146-179). 코드 실행 아님 — ±몇 개 오차.

| 파일(줄) | 전체 | Statement(세부) | Object | Proc/Func | Pkg/Body | Type | Declared | Variable | 문장 비율 |
|---|---|---|---|---|---|---|---|---|---|
| examples/prod-mode-scenario.sql (77) | 30 | **25**(dml 10 · ddl 7 · command 3 · block 2 · sql 2 · select 1) | 3 | 2/0 | — | — | 0 | — | 83% |
| examples/oracle-refcursor-pkg.sql (79) | 31 | **14**(command 12 · block 2) | — | 2/2 | 1/1 | 1 | **9** | 1 | 45% |
| examples/lob/oracle.sql (201 · 43 KB) | 35 | **20**(block 7 · dml 5 · select 4 · ddl 3 · sql 1) | 3 | 1/0 | — | — | **11** | — | 57% |
| examples/lob/mssql.sql (129 · 28 KB) | 20 | **18**(dml 11 · ddl 4 · select 3) | 2 | — | — | — | 0 | — | 90% |

손 검증(refcursor-pkg): Declared 9 중 **6~7개는 프로시저 매개변수**(`P_NAME IN VARCHAR2` 줄 — 패키지 `AS` 뒤 in_decl이 켜진 채 `(` 다음 줄 첫 낱말이 걸림 · OL:606-653) · 진짜 지역 변수는 `V_PAT`·`V_RC` 2개. `EXEC`·`PRINT`·`SHOW` 줄이 각각 Statement(command) 행(12개).

## 5. "너무 많이 보이는" 원인(우선순위)
1. **Statement 행이 지배**(45~90%): DML/SELECT/DDL 한 줄마다 1행 + SQL*Plus 명령(EXEC·PRINT·SET·SHOW·COMMIT 등)마다 1행 — 종류별 숨김·접기 수단 없음(OL:409-434 · OP:213-225).
2. **CREATE 중복**: `CREATE TABLE X` = 문장 행 + Object 행이 같은 줄에 2행(OL:427-435 + 487). 블록 CREATE(PROCEDURE/PACKAGE)도 동일.
3. **Declared 오탐**: 선언부 판정이 서브프로그램 매개변수 줄까지 잡음(패키지 명세·본문에서 매개변수 수만큼 증가 · OL:606-653). `CURSOR`는 블록 조건 없이 어디서나(OL:566).
4. 트리가 아니라 평면 + 들여쓰기 → 접기 불가 · 필터 결과도 부모 없이 평면(OP:593, 213-225).
5. 캐럿 추적 없음 → 긴 목록에서 "지금 위치"를 못 찾아 더 많아 보임(OP).
6. 머리 개수가 필터와 무관한 전체 수(OP:556) · Goto Symbol도 Statement 전부 포함(GT:600-613).
