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
| 설정 | `intel.*` 13키(분류 Editors ▸ Code completion) + `edit.complete` Ctrl+Space · `goto.symbol` Ctrl+R(mac ⌘R) | ✅ |
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
