# 76 · IntelliSense + 문서 아웃라인 — 타 IDE 조사(JetBrains 중심) · 설계 · 구현(사용자 09-23)

> 요청: "IntelliSense에 필요한 대상·범위·기능·기법·데이터 구조·캐시·적용 방식을 **가장 보편적이고 친숙한 방식**으로 구현" + "패키지 개발 때 쓸 **현재 문서 기반 Outline 캐시**(변수·함수를 빠르게) · IntelliJ/DataGrip 등 JetBrains 편의 기능 deep research · 다른 IDE도".
> 선행 = [47](47-intellisense-metadata.md)(메타 저장소 `nsql-run::meta` ✅ 구현 · D-79~86 ✅ 확정 · 설정 표 §8) · [29 §6](29-editor-syntax-palette-statusbar.md)(원칙 · 후보 소스 순위 · alias 탐지 · UI) · [28](28-object-explorer.md). 이 문서는 **조사 → "보편적" 정의 → 47과의 차이 → 구현 지도**다.

## 0. 결론 여섯 줄

1. **보편적 IntelliSense = 세 층**: ① 트리거(타이핑 자동 + `.` 즉시 + Ctrl+Space 수동) ② 문맥(캐럿 문장 안에서 `alias.`/FROM 뒤/식/문장 시작) ③ 후보 = 스키마 객체(탐색기와 같은 메타) + 문장 alias 컬럼 + **문서 심볼**(변수·CTE·서브프로그램) + 키워드 + 문서 단어 — 모든 도구(DataGrip · DBeaver · VS Code mssql · SSMS · SQL Developer)가 이 셋을 공유하고, 차이는 랭킹·삽입 옵션·JOIN 생성 같은 "위 층"이다.
2. **JetBrains가 보편에 더한 것**: 약어/camel/`_` 조각 일치 · MRU/ML 랭킹 · **Structure(아웃라인) 창 + File Structure 팝업(Ctrl+F12)에서 문장 단위 이동·실행** · JOIN 완성(FK) · INSERT 컬럼 목록 생성 · alias 자동 삽입 · 후위(postfix) 완성. 이 중 **아웃라인·조각 일치·MRU·alias 삽입**을 1차에, JOIN/INSERT 생성·후위 완성은 2차(§7).
3. **문서 아웃라인 = `nsql-script::outline`**(구현 ✅): 문장 머리 · `DEFINE`/`VARIABLE` · CTE · `CREATE` 대상 · PL/SQL `PROCEDURE/FUNCTION/PACKAGE [BODY]/TRIGGER/TYPE/CURSOR/<<라벨>>` · 선언부 변수 · T-SQL `DECLARE @v`. 캐시 = **탭별 · 본문 세대(`EditState::rev`) 열쇠** · 유휴 때 재계산 · 소비자 = 아웃라인 패널 · Goto Symbol 팔레트(Ctrl+R) · 완성 후보.
4. **문맥·랭킹 = `nsql-script::intel`**(구현 ✅ · 순수 함수 · 시험): `context_at`(멤버/관계/식/시작/없음 + alias 표(CTE·서브쿼리 포함)) · `score`(정확 1000 > 접두 900 > 단어 경계 700 > 포함 500 > 약어 400 > 부분열 200 · 한글 자모열) · `rank`(점수 → MRU → 출처 → 길이 → 이름 · 중복 제거) · `apply_case`.
5. **적용 = 기존 부품 재사용**: 팝업 = nexa-ctl `ContextMenu`(항목 + 오른쪽 열 = 타입/종류 · ↑↓ Enter Tab Esc · 창 안 배치 규칙) · 캐럿 위치 = 새 `TextBox::caret_point()` · 확정 = `replace_range` · 메타 = 탐색기가 채우는 `MetaStore`(47 §2 · 서버별 스냅샷) — 새 스레드 0(후보 계산은 O(log n + k) · 예산 안).
6. 결정 **D-200~D-204**(§8) — 전부 47의 확정을 잇는 "기본값" 수준이라 사용자 확인 없이 진행 · 값은 전부 설정 키(`intel.*`).

## 1. 조사 — 무엇이 "보편적이고 친숙한가"

| 도구 | 트리거 | 문맥·범위 | 후보 종류 | 랭킹 | 부가 기능 | 아웃라인/심볼 |
|---|---|---|---|---|---|---|
| **DataGrip / IntelliJ**([문서](https://www.jetbrains.com/help/datagrip/auto-completing-code.html) · [고급](https://www.jetbrains.com/help/datagrip/advanced-code-completion.html) · [기능](https://www.jetbrains.com/datagrip/features/completion.html)) | 타이핑 자동(지연 설정) · Ctrl+Space 기본 · Ctrl+Shift+Space 타입 일치 · 2회 = 더 넓게 | 문장 문법 기반(alias 인식 · 스키마 한정 · 서브쿼리) | 객체 전 종류 · 컬럼 · 키워드 · 함수 · 변수 · 라이브 템플릿 · **후위 완성** | 문맥 관련도 + **ML 정렬**(선택) · 최근 사용 · camel/`_`/hyphen 조각 첫 글자 일치 | **JOIN 절 완성**(FK) · **INSERT 컬럼 목록 생성** · alias 자동 삽입(설정) · 시노님 alias · 클라우드 완성(2025) | **Structure 창(Alt+7) + File Structure 팝업(Ctrl+F12)**: DDL/DML/SELECT 문장 체크박스 · 타이핑 필터 · Enter 이동 · **팝업에서 문장 실행** · Go to Symbol |
| **DBeaver**([문서](https://dbeaver.com/docs/dbeaver/SQL-Assist-and-Auto-Complete/)) | 자동 활성(지연) · `.` 트리거 · Ctrl+Space | 엔진 3(Semantic/Legacy/Combined) · alias · 스키마 | 객체 · 컬럼 · 키워드 · 서버 객체 · 프로시저 | 정렬(알파벳/관련도) · 중복 숨김 · 짧은 이름 | alias 삽입 · 공백 삽입 · 대소문자 · 긴 이름 | Outline 패널(문장·조각) |
| **VS Code mssql**([문서](https://learn.microsoft.com/en-us/sql/tools/visual-studio-code-extensions/mssql/mssql-extension-visual-studio-code)) | 타이핑 · Ctrl+Space | 접속 DB · T-SQL 파서(언어 서비스) | 테이블·컬럼·키워드·스니펫 | LS 점수 | 오류 검사 · quick info | Outline(LS 문서 심볼) |
| SSMS / Redgate SQL Prompt | 타이핑 · Ctrl+Space · `.` | T-SQL 파서 · alias | 객체·컬럼·키워드·스니펫 | 관련도 · 최근 | JOIN 조건 제안 · `SELECT *` 펼치기 · alias 삽입 | Object Explorer · (Prompt) 코드 아웃라인 |
| Oracle SQL Developer | 타이핑(Completion Insight) · Ctrl+Space | PL/SQL 파서 · 패키지 멤버 | 객체·컬럼·패키지 서브프로그램 | 이름 | 코드 템플릿 | PL/SQL 편집기 **아웃라인 트리**(선언·서브프로그램 · 클릭 이동) |
| Sublime Text(DR-5) | 타이핑 · Ctrl+Space | 문서 단어 · 완성 파일 | 단어·스니펫 | 접두·퍼지 | — | **Goto Symbol(Ctrl+R)** · Goto Definition |

**공통 분모(= 1차 목표)**: 타이핑 자동 + `.` + Ctrl+Space · alias 인식 · 객체/컬럼/키워드/문서 단어 · 접두+조각 일치 · MRU · 종류 아이콘 + 타입 열 · Enter/Tab 확정 · 아웃라인 창 + 심볼 팔레트.
**JetBrains만의 것(2차)**: JOIN 완성 · INSERT 컬럼 생성 · 후위 완성 · ML 정렬 · 팝업에서 실행.

## 2. 대상 · 범위(무엇을 · 어디까지)

| 후보 출처(29 §6-2 순위) | 무엇 | 범위 | 데이터 |
|---|---|---|---|
| 1 alias 컬럼 | `a.` → 그 테이블 컬럼(이름 · 타입 · NULL · PK) | 캐럿 문장의 alias 표 | `MetaStore::columns(id)`(없으면 "불러오는 중" + 즉시 채움 요청) |
| 2 문장 테이블·alias | `SELECT a.`, `WHERE d.` 의 `a`·`d`, CTE·서브쿼리 alias | 캐럿 문장 | `intel::alias_table` |
| 3 스키마 객체 | 테이블·뷰·시노님·시퀀스·프로시저·패키지 | 현재 스키마(+ `SCHEMA.` 입력 시 그 스키마 · D-80) | `Snapshot::prefix/prefix_any` |
| 3' **문서 심볼**(신설) | `DEFINE`/`VARIABLE`/CTE/선언 변수/서브프로그램/라벨 | **현재 문서** 전체 | `outline::Outline::names()` |
| 4 시스템 객체·내장 함수 | `DBMS_OUTPUT.` · `NVL(` · `ALL_TABLES` · `sys.tables` … | 접속 방언 | 정적 표 `nsql-script::builtins`(T-178 ✅ · 시그니처 도움 `intel.signature_help`) |
| 5 키워드 | `SELECT`·`GROUP BY`… | 문장 시작·식 | `intel::KEYWORDS` |
| 6 문서 단어 | 방금 친 식별자·바인드 `:V` | 현재 문서 | 문서 낱말 집합(outline `words`) |

## 3. 기능 · 기법(어떻게)

| 기능 | 규칙(설정 키 · 47 §8-1) |
|---|---|
| 트리거 | `.` 즉시(`intel.trigger_chars`) · 식별자 `intel.min_chars`(2)자 + `intel.delay_ms`(250) 무입력 · Ctrl+Space 항상(`edit.complete`) · 문자열/주석 안 = 안 뜸 · 자동은 `intel.auto_activation`/`intel.activate_on_typing`으로 끔 |
| 문맥 | `intel::context_at` — 멤버(`qualifier.`) · 관계(FROM/JOIN/UPDATE/INTO/DESC 뒤 · 콤마 목록) · 식 · 시작 · 없음. alias 표 = `[schema.]table [AS] alias` · CTE · 서브쿼리 |
| 일치 | `intel.match` = fuzzy(기본)/contains/prefix · 대소문자 무시 · **한글 자모열**(97차 규칙) |
| 랭킹 | 등급 → MRU(최근 확정 20 · `intel.recent_boost`) → 출처 순위 → 길이 → 이름 · 중복 제거(`intel.hide_duplicates`) · 상한 `intel.max_items` |
| 삽입 | 접두 구간 교체(`replace_range`) · `intel.insert_case`(default/upper/lower/match) · 함수 `NAME()`+캐럿 안(`intel.insert_parens`) · 테이블 뒤 alias(`intel.insert_alias`) · 키워드 뒤 공백(`intel.insert_space`) · `INSERT INTO t (` 전체 컬럼 조각(`intel.insert_columns`) |
| 표시 | 종류 아이콘 · 오른쪽 열 = 타입/스키마/종류(`intel.show_types`) · `intel.popup_rows`(12) · 코멘트 툴팁(`intel.show_comments` · 2차) |
| 아웃라인 | 패널(활동 막대 · 필터 틀 · 클릭 = 이동 · 문장/선언/서브프로그램 깊이) · **Goto Symbol 팔레트**(Ctrl+R · 이름 타이핑 → Enter 이동) · 탭별 캐시(세대) |

## 4. 데이터 구조 · 캐시

- **메타**: 47 §3 `MetaStore`(인터닝 · 버킷 · `Arc<Snapshot>` · 접두 이진 탐색) — 탐색기 메타 스레드의 응답(`Resp::Schemas/Objects/Columns`)을 **탐색기 트리와 함께** 저장소에 넣는다(서버별 · 접속당 1 세션 · 26 §8) → 팝업은 스냅샷만 읽는다. 컬럼이 `Unknown`이면 즉시 채움 요청(탐색기의 같은 큐 · 큐 앞).
- **문서 아웃라인 캐시**: `Editors`의 탭별 `(rev, Outline)` — 편집 뒤 유휴(틱)에 재계산 · 큰 파일 단계(L1+)는 끔(72 §2) · 비용 = 문장 분리 + 낱말 걷기(2 MB ≈ 수 ms).
- **문맥 캐시**: 문장 텍스트 해시 → alias 표(29 §6-3).
- **MRU**: 최근 확정 20개(세션 · 설정 `intel.recent_boost`).

## 5. 적용 방식(UI)

- 팝업 = `ContextMenu`(창 안 배치 규칙 · 61 §2-2) — 항목 라벨 = 후보 · `with_shortcut` 열 = 타입/종류 · 아이콘 = 종류 · 선택 = ↑↓ · 확정 = Enter/Tab · 닫기 = Esc/바깥 클릭/문맥 이탈 · 타이핑은 편집기로 가고 팝업은 다시 거른다(같은 자리) · 12행 넘으면 스크롤.
- 캐럿 위치 = `TextBox::caret_point()`(캐럿 아래 기준선) → 팝업 `open_at(x, y)`.
- 확정 = `TextBox::replace_range(prefix 구간, 후보)` + `apply_case` + (테이블이면) alias/공백 옵션.
- Goto Symbol = 팔레트에 심볼 목록(`sym:<byte>` id) → `Pick` → 이동. 아웃라인 패널 = 활동 막대 `view.outline`.

## 6. 구현 지도(이번 세션)

| 층 | 파일 | 상태 |
|---|---|---|
| 문서 심볼 | `nsql-script/src/outline.rs`(`outline()` · `Outline::names/find` · `words()`) · 시험 3 | ✅ |
| 문맥·랭킹 | `nsql-script/src/intel.rs`(`context_at` · `alias_table` · `score/rank` · `KEYWORDS` · `apply_case`) · 시험 3 | ✅ |
| 캐럿 픽셀 | nexa-ctl `TextBox::caret_point()` | ✅ |
| 설정 | `intel.*` 13키(분류 Editors ▸ Code completion) + `edit.complete` Ctrl+Space(맥 ⌃Space = `control+space` · 09-23 맥 열 `ctrl+`=⌘ 결함 수정) · `goto.symbol` Ctrl+R(mac ⌘R) | ✅ |
| 호스트 | `crates/nexa-sql/src/intel.rs`(`IntelCfg` · `Intel` = 팝업(ContextMenu) · 트리거/디바운스 · 후보 조립 · `pick` · MRU · 탭별 `DocCache`) · main.rs `intel_after_event/intel_request/intel_apply/open_goto_symbol/goto_byte` · 시험 2 | ✅ |
| 메타 feed | `Explorer.meta: MetaStore` — `Resp::Schemas/Objects/Columns`를 트리와 함께 저장 · `Req::ColumnsMeta` 즉시 채움 · `ExplorerSet::meta_view/request_columns`(spec의 칸 · 없으면 보이는 칸) | ✅ |
| 아웃라인 UI | `outline_panel.rs`(활동 막대 `view.outline` · 필터 · 클릭/Enter 이동 · 탭·세대 열쇠 동기) + Goto Symbol 팔레트(`sym:<byte>`) | ✅ |
| 자동 점검 | `scripts/win-func-check.ps1` S66(Ctrl+Space) · S67(Goto Symbol) · S68(아웃라인 패널) · 실기 U-78~U-80 | ✅ |
| **정적 표(T-178 · 99차 §129)** | `nsql-script/src/builtins.rs` — 방언별 내장 함수(시그니처) · Oracle `DBMS_*` 패키지 12 · 사전 객체(`ALL_*`/`V$*` · `pg_catalog.`/`information_schema.` · `sys.`/`INFORMATION_SCHEMA.` · `sqlite_master` …) · `signature()` · `system_members()` · 시험 3 | ✅ |
| **괄호 주인(T-178)** | `intel::Context::paren_owner/paren_into` — 시그니처 도움 · `INSERT INTO t (` 컬럼 목록 조각 | ✅ |
| **확정 손질 · 스크롤(T-178)** | 호스트 `pick` = 함수 `NAME()`+캐럿 안 · 키워드 공백 · FROM 뒤 alias(`gen_alias`) · 팝업 `set_max_rows(popup_rows)` · 컬럼 상세 PK/FK/UQ/NOT NULL · 예산 로그 · 설정 7(`intel.functions`·`insert_parens`·`insert_alias`·`insert_space`·`insert_columns`·`signature_help`·`budget_ms`) · S77·S78 · 위키 "3분 사용법" | ✅ |

## 7. 2차(JetBrains 상위 기능 · 후속 T)

~~INSERT 컬럼 목록 생성 · alias 자동 삽입 · 시그니처 도움(상태줄) · 시스템 객체 정적 패키지(29 §6-4) · 팝업 스크롤~~ = **99차 §129 ✅**. 남음 = JOIN 완성(FK = `nsql-catalog::keys` · MetaStore에 FK 대상이 들어온 뒤) · 후위 완성(`.count`) · 시그니처 도움을 캐럿 옆 카드로 · hover 카드(47 D-84 · T-179) · 팝업에서 문장 실행 · ML 정렬(수요 시) · 디스크 캐시(D-85) · 후보 워커(예산 로그가 실측되면 · D-202).

## 8. 결정

| # | 결정 | 근거 |
|---|---|---|
| D-200 | 1차 후보 출처 = alias 컬럼 · 문장 alias · 스키마 객체 · **문서 심볼** · 키워드 · 문서 단어(시스템 객체 패키지는 2차) | 조사 공통 분모 |
| D-201 | 팝업 부품 = `ContextMenu` 재사용(새 컨트롤 0) | 30 §2 부품 규칙 · 팝업 배치 규칙 |
| D-202 | 후보 계산 = UI 스레드에서 스냅샷 조회(예산 `intel.budget_ms`) · 워커는 예산 초과가 실측되면 | 47 §2는 워커 안이나 O(log n + k)라 먼저 측정 |
| D-203 | 아웃라인 캐시 열쇠 = 본문 세대 · 유휴 재계산 · L1+ 끔 | 72 §2 |
| D-204 | Goto Symbol = 팔레트(Sublime Ctrl+R) · 아웃라인 = 활동 막대 패널 | DR-5 · 조사 |

## 9. 메타 즉시 채움 · 미리 읽기 · 즉시 회수(100차 mac · 09-23)

사용자 09-23(맥 실기): "FROM 뒤 `M4S_`에 테이블이 안 보인다" → "현재 스키마는 접두 없이 · 다른 스키마는 `스키마.` 뒤" → "권한에 맞는 `ALL_`/`DBA_`(다른 DBMS도)" → "기본 스키마는 접속 시 바로 메모리 · `스키마.` 시점에 캐싱" → "단일 원천 + 유니버설 어댑터 · 중복 0" → "미사용 판정 즉시 회수".

**결함 셋(원인)**: ① 현재 스키마 = 접속 계정과 같은 이름의 스키마만 → SQL Server(`dbo`)·PG(`public`)에서 비어 테이블 후보 0 ② 버킷은 탐색기가 폴더를 펼쳐야만 채워짐 ③ `Snapshot::prefix(…, "", 200)`이 이름순 앞 200개만 넘겨 랭킹 전에 `M4S_*`가 잘림.

**구조(단일 원천 위의 어댑터)**: 원천은 `nsql-run::meta::MetaStore` 한 벌(Arc 스냅샷 · 이름 인터닝 · 버킷 = (스키마, 종류) + `Coverage`). 소비자 = 탐색기 트리 · 완성(`MetaView`) · 툴팁 · (예정) hover 카드·객체 정보 탭 — 모두 스냅샷만 읽는다(복사 0 · 77 §1-2). 채우는 길은 둘: 트리 펼침(`Req::Objects`)과 **완성의 즉시 채움**(`Req::ObjectsMeta`/`DictMeta` · 노드 없이).

| 때 | 무엇 | 어디 |
|---|---|---|
| 접속 직후(스키마 목록이 오면) | 서버가 알려 준 **현재 스키마**(`nsql_catalog::current_schema` → 계정 이름 → `public`/`dbo`/`main` → 단일 · `pick_current_schema`) · `intel.preload`면 그 스키마의 관계 종류(테이블·뷰·구체화 뷰·시노님 — 방언이 가진 것만) + **권한 반영 사전 뷰**(`nsql_catalog::dictionary` · `DICT_SCHEMA = "$dict"` 버킷) | `Explorer::preload_meta` |
| FROM/JOIN 자리 | 현재 스키마 버킷이 `Missing`이면 `NeedObjects(현재)` + "객체 불러오는 중…" · `Loaded`면 **상한 없이** 전부 → 랭킹(`intel.max_items`)이 자른다 · 사전 버킷 `Loaded`면 그것(정적 표는 폴백) | `Intel::request` Relation |
| `스키마.` | 그 스키마의 관계 종류 버킷 `Missing` → `NeedObjects(스키마)`(그 시점에 캐싱) · `sys.`/`pg_catalog.`/`INFORMATION_SCHEMA.` = 사전 버킷의 `스키마.이름`에서 뒷부분 | `Intel::request` Member 2·2-b |
| 메타 도착 | `Explorer::drain`이 참이고 팝업이 대기 중(`Intel::is_loading`)이면 같은 틱에 다시 조립 | `App` drain |
| 유휴 틱 | `스키마.`로 읽은 버킷(현재 스키마·사전·트리가 펼친 것 제외) 가운데 **열린 문서 어디에도 그 이름이 없는 것**(탭별 낱말 캐시 `Intel::doc_mentions`)은 즉시 `MetaStore::drop_bucket`(목록·정확 조회·전역 정렬·컬럼 해제 · 객체 행 40 B는 id 안정성 때문에 남음) | `App` 유휴 틱 → `ExplorerSet::reclaim_intel_buckets` |

**사전 뷰 = 권한 반영**: Oracle `ALL_VIEWS`(OWNER=SYS · `ALL_/DBA_/USER_/CDB_`)는 접근 가능한 뷰만 돌려주므로 `DBA_*`는 SELECT ANY DICTIONARY·SELECT_CATALOG_ROLE이 있을 때만 보인다 + PUBLIC 시노님 `V$`/`GV$`(`ALL_SYNONYMS`도 접근 가능한 것만) · PG `pg_catalog`·`information_schema`에서 `has_table_privilege(SELECT)` · SQL Server `sys`·`INFORMATION_SCHEMA`(메타데이터 가시성 규칙) · MySQL 시스템 스키마(`information_schema.tables`는 권한 있는 것만) · SQLite 내장 표. 이름은 Oracle만 맨이름, 그 밖은 `스키마.이름`(FROM 뒤에 그대로).

**약어 퍼지**(`MI40` → `M4S_I002040`): `score`의 마지막 단계(부분열)가 이미 받았다 — 후보가 목록에 들어오지 못한 것(③)이 문제였다. 시험 `relation_requests_missing_buckets_then_lists_all_and_fuzzy_abbrev`(300개 뒤의 `M4S_*` · 약어 · 사전 · `스키마.` 요청 · 문서 언급) · `pick_current_schema` · `drop_bucket`.

**남은 중복(T-184)**: 탐색기 트리 노드가 `ObjectInfo`(이름 문자열)를 따로 들고 있다 — 노드가 `ObjId`만 가리키고 이름은 인터너에서 읽게 바꾸면 이름 복사가 0이 된다(탐색기 그리기 경로 수정 · 별도 작업).

## 10. 상세 카드(100차 mac · 09-24)

팝업 옆 같은 높이의 반투명 카드(`intel_card.rs` · 77 §1-3 `HoverCard`의 첫 구현). 틀은 하나(제목 · 부제 · 속성 행 · 절 목록 · 읽는 중), 대상별로 채운다.

| 대상 | 속성 행 | 절 |
|---|---|---|
| 컬럼 | Table · Column · Seq · Type · Description(코멘트) · Not Null · Default | Keys(PK/UQ `#i/n`) · Indexes(`#i/n` · unique) · Constraints(FK → 대상 · CK) — **이 컬럼이 든 것만** |
| 테이블/뷰/시노님 | Schema · Kind · Description(코멘트) · Status · Info · Columns(수) | Keys(PK 열) · Columns(12개 + `… +n`) · Indexes |
| 내장 함수 | Signature | — |
| 스키마 | Objects(읽은 수) | — |
| 키워드 · 문서 낱말 · 조각 | 종류 | 조각 = 컬럼 목록 |

데이터 = `nsql_catalog::table_detail`(제약 P/U/R/C + 인덱스) → `Req::DetailMeta`(백그라운드 세션 · 급한 컬럼을 막지 않음) → `MetaStore::set_detail`. 팝업 항목 = `이름 : 타입` + 오른쪽 표식(PK/FK/UQ/NN) · 클릭 = 선택(카드) · 더블 클릭/Enter = 확정. 설정 `intel.detail_card` · `intel.detail_bg_alpha`(**투명도 %** · 기본 0 = 불투명 · 09-24 사용자 "투명도 0" · 처음엔 20) · `intel.detail_text_alpha`(0 · 처음엔 50) · `intel.popup_rows` 10 · ★ **카드 머무름** `intel.card_settle_ms`(150 · 09-24 "빠른 스크롤 중엔 누적 · 마지막 대상만") = hover 효과 규칙과 같은 구조(사건은 목표 덮어쓰기만 `note_hover` · 틱 `card_tick`이 머문 마지막 목표만 카드에 · 그때만 `DetailMeta` 선조회 · 첫 대상은 즉시 · 깨움 = 마지막 변경 + settle).

## 11. 정렬 기준 · 매칭 점수(09-24 · 사용자 "친숙한 정렬 · 필터 뒤는 점수 우선")

- **둘러보기(접두 없음)**: 출처 그룹 순(alias 컬럼 1 → 문장 테이블·CTE 2 → 스키마 객체 3 → 사전·시스템 4 → 키워드 5 → 문서 낱말 6) → 그룹 안 `Cand.order`(컬럼 = 테이블 안 **순번** · 객체 = 종류 테이블→뷰→구체화 뷰→시노님 → 이름 · 키워드 = 표 순서) → 이름.
- **거르기(접두 있음)**: ① 일치 등급 — 정확 1000 · 접두 900−(길이차) · 낱말 경계(`_`/숫자↔글자/camel) 700 · 포함 500−위치 · 약어(낱말 시작 부분열) 400 · 부분열 100~399 ② 같은 등급 = 최근 확정(MRU 20) ③ 출처 ④ 둘러보기 순서 ⑤ 길이 ⑥ 이름. 한글 = 자모열 포함 500.
- **부분열 점수 `fuzzy_quality`**: 앞에서부터 탐욕 매칭 → `200 + 20·(낱말 시작에 걸린 글자 수) − 10·(빈틈 수) − (길이 초과)/5`, 100~399. 연속 일치·낱말 시작·짧은 이름이 앞으로.
- 같은 글(대소문자 무시)은 앞 것만(테이블 `EMP` 있으면 문서 낱말 `emp` 숨김) · 조각(컬럼 목록)은 늘 맨 위.

## 12. 접두 없는 컬럼 완성(09-24 · 사용자 "JOIN 다중 테이블에서 alias 없이 요청하면 전 테이블 정보 · 기본 A.컬럼 · Alt = 컬럼만")

`Expr` 문맥에서 문장의 모든 alias(비-CTE)에 대해 컬럼을 후보로(`Cand.qualifier` = alias · 오른쪽 열 = alias · `order` = alias 순서×1000 + 순번) · 안 읽은 테이블은 `NeedColumns`(급함)로 채우고 "불러오는 중". 확정(`Intel::pick_with(id, plain)`) = `qualify_columns != plain`이면 `alias.컬럼` — 호스트가 Alt 상태(`App.alt`)를 `plain`으로 넘긴다(Enter/Tab·더블 클릭 공통). 설정 `intel.qualify_columns`(on). 시험 `unqualified_columns_from_all_aliases_and_alt_plain`.

## 13. 완성 목록 페이지 로딩 — ✅ 구현(§175 · T-196) · 설계(09-24 · 사용자 "제한 200은 좋아 · 끝까지 스크롤하면 결과 그리드처럼 자동 추가 페치 · 끝에 읽는 상태 표시 · 메모리 점검·미사용 정리") — 📐 T-196

### 13-1. 현상과 원인(BISCM 실측 · CLI `nsql run -c BISCM -`)
- BISCM = 테이블 449 · 뷰 1(`VM4S_I002040`) · 함수 2 · 패키지 1 · 프로시저 72. FROM 자리 빈 접두에서 뷰가 안 보인 까닭 = ① 동점 정렬이 종류 우선(테이블 → 뷰)이라 뷰가 449개 뒤 ② `intel.max_items`(200)에서 잘림. ①은 같은 층 종류 무구분으로 고쳤고(§163 `kind_order` 0), ②가 이 절.
- 정렬은 등급 → 레이어 → 점수 → MRU → 출처 → 순서 → **길이 → 이름**이라 `VM4S_I002040`(12자)은 11자 테이블들 뒤 = 목록 끝. 사용자 기대("끝에 보여야")와 같다.

### 13-2. 다른 도구
| 도구 | 긴 목록 |
|---|---|
| VS Code suggest | 전부 들고 가상 스크롤(행 위젯은 보이는 것만 재사용) · 상한 없음 |
| JetBrains | 전부 · 가상 목록 · "더 보기" 없음(⌃Space 두 번 = 범위 확장) |
| DBeaver | `Max proposals` 기본 100(초과 = 잘림 · 안내 없음) |
| SSMS IntelliSense | 전부 · 가상 목록 |
| 결과 그리드(우리) | 페치 크기 단위 · 끝 근처에서 다음 페치 · 끝 줄에 "불러오는 중…"(43 §5) |

공통 = 데이터는 한 벌, 보이는 것만 그린다. 우리 팝업(nexa-ctl `ContextMenu`)은 이미 **보이는 행만 그리고**(`vis_range` · `set_max_rows`) 항목 벡터는 통째로 든다 → 병목은 그리기가 아니라 **항목(CtxItem) 조립 수**와 그 문자열이다.

### 13-3. 설계 = 한 세트 + 창 투영(DR-33과 같은 사상)
- **원천 하나**: `Intel.cands` = 랭킹 끝난 전체(≤ `intel.max_total` · HIDDEN · 기본 5000 — 그 위는 "더 좁혀 주세요" 안내). 랭킹은 전부 한 번(n ≤ 5000 · 정렬 ≈ 1 ms · 상위 k만 뽑는 `select_nth_unstable`은 불필요 — 페이지마다 다시 고르는 편이 더 비싸다).
- **창(`shown`)**: 팝업 항목은 `cands[..shown]`만 조립 · 처음 `shown = intel.max_items`(200) · 끝 항목 = `intel:more`("N개 더 — 아래로 스크롤하면 이어서") 비활성.
- **트리거**: 메뉴가 스크롤(휠·PgDn·End·트랙 클릭·↓)로 **마지막 실제 항목이 보이는 범위에 들어오면** 깃발(nexa-ctl `ContextMenu::take_reached_end()` · `scroll_rows`/`move_hover`에서 `first + vis_count ≥ items.len() - 1`) → 호스트가 `Intel::extend()` → `shown += max_items` → 항목 다시 조립 → `ContextMenu::replace_items(items)`(**`first`·`hover`·위치·폭 유지** · 폭은 상한 안에서 넓어질 때만) → 끝 항목 갱신(남은 수) 또는 제거.
- **읽는 상태 표시**: 이어 붙이는 동안 끝 항목 글을 "불러오는 중…"으로(그리드 43 §5와 같은 글 `StIntelLoading`). 지금은 데이터가 메모리에 있어 한 프레임 안에 끝나지만, 버킷이 `Coverage::Loading`이면 그 글이 그대로 남아 **"메타를 읽는 중"과 "페이지를 붙이는 중"이 같은 자리·같은 표현**이 된다(사용자 요구 "맨 끝에 추가 데이터가 읽어지는 상태").
- **End 키** = 남은 페이지를 전부 붙인 뒤 마지막으로(그리드 End와 같은 뜻 · `max_total` 안이라 유계). **Home** = 첫 항목(창은 그대로).
- **타이핑·Backspace** = 새 요청 → `shown` 초기화(첫 페이지). MRU·선택 행은 종전대로.
- **Alt/Enter/클릭 확정** = `cands[i]`(창과 무관 · 인덱스는 전체 기준) — `pick_with` 변경 없음.

### 13-4. 성능·메모리 점검(71 체크리스트)
| 항목 | 값 · 판단 |
|---|---|
| 상주 메모리 | **+0** — 전체 후보(`Vec<Cand>` · 5000 × ≈ 120 B ≈ 600 KB 최악)와 항목(200 × ≈ 200 B ≈ 40 KB/페이지)은 **팝업이 열린 동안만** · 닫히면 둘 다 `Vec::new()`로 해제(13-5) |
| 프레임 | 페이지 조립 200개 ≈ 0.1 ms(문자열 clone · 아이콘 Rc clone) · 랭킹 5000 ≈ 1 ms · 그리기는 보이는 10행 그대로 |
| 스레드·소켓·디스크 | 없음(메타는 이미 있는 스냅샷 · 네트워크 0 · 26 §8 변경 없음) |
| 상한 키 | `intel.max_items`(페이지) · `intel.max_total`(HIDDEN 5000) · `intel.popup_rows`(보이는 행) |
| 회귀 위험 | `replace_items`가 폭·위치를 다시 잡으면 팝업이 튄다 → **폭은 넓어질 때만, 위치는 고정**(§133 앵커 규칙) · 시험 = 항목 교체 뒤 `rect.x/y` 불변 |

### 13-5. 미사용 메모리 정리(사용자 "적당한 시점에 정리")
1. **팝업 닫힘 즉시**: `Intel::close()` → `cands = Vec::new()` · `shown = 0` · 문맥/앵커 비움. nexa-ctl `ContextMenu::close()`는 지금 항목을 **비우지 않는다**(다음 `open_at`까지 든다) → `close()`에서 `items = Vec::new()`(라벨·아이콘 Rc 해제) — 모든 메뉴에 이롭다.
2. **아이콘 캐시**(`Intel.icons` · 종류 ≤ 16 × 32×32×5 B ≈ 80 KB): 상수 상주 → 39 §3 표에 "고정 비용"으로 등재 · 테마 색이 바뀌면 통째 비움(색이 캐시 열쇠에 없으므로) · 유휴 회수 대상은 아님(다시 만드는 비용 > 80 KB).
3. **문서 낱말 캐시·`스키마.` 버킷**: 종전 유휴 틱 회수(`reclaim_intel_buckets` · §9) 그대로 — 페이지 로딩은 새 캐시를 만들지 않는다.
4. **메타 스냅샷 Arc**: 요청 동안만 잡는다(`MetaView`) — 변경 없음.

### 13-6. 구현 순서(T-196 · 작은 단계 셋)
① nexa-ctl: `take_reached_end()` · `replace_items()`(first·hover·위치 유지) · `close()`가 항목 해제 · 시험 3 → ② nexa-sql: `Intel.shown` · `extend()` · 끝 항목 글 두 가지("N개 더" / "불러오는 중…") · End = 전부 · 호스트 배선(`intel` 사건 뒤 `take_reached_end`) · 시험(BISCM 재현 = 449+1+72에서 두 페이지 뒤 `VM4S_I002040` 보임) → ③ 설정 `intel.max_total`(HIDDEN) · 39 §3 · 위키 · U-121. 결정 = **D-205**(끝 항목 = 비활성 안내 한 줄 · 그리드와 같은 글) · **D-206**(End = 전부 붙인 뒤 마지막 · 상한 `max_total`).

## 14. 4-DBMS 절별 대상 검토(09-24 · 사용자 "오라클·MSSQL·Postgres·SQLite 각 clause 별로 정확한 대상이 표시되는지") — 실측 = CLI `nsql cat`/`nsql run`(BISCM · M4PLAN · Repository · Demo)

| 절 / 자리 | Oracle(BISCM) | SQL Server(M4PLAN) | PostgreSQL(Repository) | SQLite(Demo) |
|---|---|---|---|---|
| 문장 시작 | 공통 키워드 + `ROWNUM`·`CONNECT BY`·`MERGE`… | + `TOP`·`CROSS APPLY`·`EXEC`·`GO` | + `ILIKE`·`LATERAL`·`ON CONFLICT`·`RETURNING` | + `PRAGMA`·`AUTOINCREMENT`·`ATTACH` |
| `FROM \|` | 테이블 449·뷰 1·MV·시노님(층 0) → 함수 2·패키지 1(층 1~2 · 프로시저 72 제외) → 사전 `ALL_`/`DBA_`/`CDB_`/`V$`/`GV$` + **`DUAL`**(층 1 · 09-24 추가) · 스키마 43 | 테이블 215·뷰(층 0) → 테이블 반환 함수 `FN_TABLE_CO`(IF · 층 1 · 스칼라·프로시저 15 제외) → `sys.`/`INFORMATION_SCHEMA.` 사전 · 스키마 `dbo` | 테이블 178·뷰·MV(층 0) → `SETOF` 함수(층 1 · 프로시저 제외) → `pg_catalog.`/`information_schema.` 사전 · 스키마 `public` | 테이블 4·뷰(층 0) · `sqlite_master`·`sqlite_schema`… 정적 사전 · 스키마 `main` |
| `FROM 스키마.\|` | 그 스키마 관계 + 함수·패키지(프로시저 제외) · `SYS.`는 사전 버킷 | `dbo.` 같음 · `sys.`/`INFORMATION_SCHEMA.` = 사전 접두 | `public.` 같음 · `pg_catalog.` = 사전 접두 | `main.` = 그대로 |
| `FROM 스키마.테이블.\|` | 팝업 없음 · **`스키마.패키지.`** = 테이블 함수·함수(프로시저 제외 · `SP_MPS_PEGGING_PKG` = 프로시저 2 → FROM에서는 빔 · 식 자리에서 둘) | 팝업 없음 | 팝업 없음 | 팝업 없음 |
| `alias.\|` · `테이블.\|` | 컬럼(`VARCHAR2(50)` · N/Y · 순번) | 컬럼(`varchar(100)`) | 컬럼(`character varying(50)`) | 컬럼(`INTEGER`·`TEXT`) |
| SELECT/WHERE 식 | 전 alias 컬럼(`A.컬럼`/Alt) · 내장 함수 `ORACLE` 표 · `DBMS_*` 패키지 멤버(정적) · 사용자 패키지 멤버(사전) · 키워드 · 문서 낱말 | 내장 `MSSQL` 표 · 키워드 | 내장 `POSTGRES` 표 · 키워드 | 내장 `SQLITE` 표 · 키워드 |
| `*` / `A.*` | 모든 컬럼 (N) 조각 | 같음 | 같음 | 같음 |
| `INSERT INTO t (` | 컬럼 목록 조각 | 같음 | 같음 | 같음 |

고친 것(§183): ① Oracle `DUAL`을 사전 결과에 넣음(정적 표는 사전이 읽히면 숨어 `FROM DUAL`이 완성되지 않았다) ② 방언 키워드 표 4개 + `keywords_for(dialect)`(공통과 중복 없음). 남은 것 = SQLite `temp.` 스키마 · MSSQL 임시 테이블 `#t` · PG 다른 스키마의 함수(현재 스키마만 미리 읽음 · `스키마.`로 읽힘) · Oracle 컬렉션 반환(비파이프라인) 함수 판정.
