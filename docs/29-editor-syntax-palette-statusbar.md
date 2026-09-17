# 29 — 편집기 확장 설계: 구문 강조 · 명령 팔레트 · 서식 복사 · 상태줄 · 안내선/공백 · 인텔리전스 · 다중 커서 편집 · 창 포커스

> 2026-09-14 12차(사용자 요청 연속). 구현된 것은 ✅ · 설계만은 ☐. 편집기 단계 원칙은 [17](17-editor-incremental-plan.md), 성능 규칙은 [26](26-performance-architecture.md), 설정 체계는 [24](24-settings-and-vscode-analysis.md), 탐색기 메타(Catalog)는 [28](28-object-explorer.md)을 따른다.

## 1. 구문 강조 ✅ (Sublime Text 차용 · DR-5)

| 항목 | 결정 |
|---|---|
| 엔진 | nexa-ctl `highlight.rs` — **데이터 주도 토크나이저**(키워드 집합 · 줄/블록 주석 · 문자열 구분자 · 숫자 · 식별자). 정규식 0(외부 crate 0 · DR-3). 줄 단위 `Highlighter::line_spans(line, &mut state, out)` — `state`는 블록 주석 이어짐. |
| 규격 파일 | `.nexa-syntax`(`key = value` · `keywords =` 반복) — 내장 `SQL`(Oracle·MSSQL·PG 공통 키워드 · 확장자 sql/ddl/pks/pkb/…) · `Plain Text`(txt/log/md). |
| 기본 선택 | 탭 제목의 **확장자** → 레지스트리 매칭 · 확장자 없는 `Script_N` = SQL. 탭마다 독립(`Editors.syntax[i]`). |
| 사용자 변경 | 명령 팔레트 `Set Syntax: <이름>` · 상태줄 오른쪽 **구문 이름 클릭**(팔레트가 `Set Syntax: ` 접두로 열림) · 탭 툴팁 카드에 구문 표시. 변경은 그 탭에만 · 재시작 시 다시 확장자 기본(탭별 영속은 세션 파일 [18](18-session-and-projects.md)과 함께). |
| 플러그인 설치 | `<설정 폴더>/Packages/<패키지>/<이름>.nexa-syntax`(Sublime 패키지 배치). 부팅 시 스캔 · 같은 이름은 나중 것이 내장을 덮음(사용자 오버라이드) · 파싱 오류는 stderr + `SyntaxRegistry::errors`. `.sublime-syntax`(YAML+정규식 컨텍스트) 완전 호환은 **정규식 엔진이 전제** → T-59. |
| 테마 | `Theme.syn_keyword/syn_string/syn_comment/syn_number`(라이트·다크 팔레트) — 화면은 색만(굵게는 고정폭 폭이 바뀌어 캐럿 어긋남) · HTML 복사에서만 키워드 굵게. |
| 성능 | 강조 없음 = 경로 그대로(비용 0). 강조 있음 = 보이는 행만 그리되 첫 표시 행까지 상태를 이어오느라 위 행을 토크나이즈 — 큰 파일(수만 줄)에서는 **행 시작 상태 캐시**(줄 편집 시 그 줄 이후만 무효화)로 O(보이는 행) — T-57과 함께. |

## 2. 명령 팔레트 ✅ (`Ctrl/⌘+⇧P` · View ▸ Command Palette…)

- 창 상단 중앙 오버레이(모달 · 바깥 클릭/Esc 닫힘). 입력 = nexa-ctl TextBox · 목록 ≤ 12행 · ↑↓ · Enter · 행 클릭.
- **명령 어휘 = 메뉴 액션 id 그대로**(`file.new` … `help.about`) + `syntax.set:<이름>`. 라벨 = `메뉴: 항목`(i18n) · `Set Syntax: SQL`. 팔레트를 열 때마다 다시 만든다(언어 전환 반영).
- 퍼지 = 대소문자 무관 부분열 · 연속 매치 +3 · 단어 첫 글자 +2 · 점수 내림차순 → 라벨 순. 빈 질의 = 전부.
- 후속: 최근 사용 순 가중 · 단축키 표시(키맵 T-53) · `>` 없는 파일 열기(Goto Anything) · `:줄` 이동 · `@` 심볼.

## 3. 서식 있는 복사 ✅ (설정 `editor.copy_rich` · 기본 켬)

- 편집기에서 복사(`Ctrl+C` · 메뉴 · 우클릭)하면 **평문 + HTML**을 함께 올린다 → PowerPoint/Word/Outlook에 붙여넣으면 구문 색·키워드 굵게·고정폭·배경색(현재 테마)이 유지. 평문만 받는 곳(편집기·터미널)은 그대로 평문.
- HTML = nexa-ctl `to_html`(`<pre style>` + `<span style="color">`, 키워드 `font-weight:bold`, 이스케이프). 폰트 스택 `Consolas, Cascadia Mono, D2Coding, Menlo`.
- OS: **Windows** `CF_UNICODETEXT` + 등록 형식 `HTML Format`(CF_HTML 헤더 · UTF-8 바이트 오프셋) ✅ · **macOS** `osascript` `{«class HTML»:«data HTML…», string:"…"}`(실패 시 평문) · **Linux** 평문만(wl-copy/xclip은 한 번에 한 형식 → T-60: 자체 Wayland/X11 클립보드 소유).
- RTF는 두지 않는다(HTML을 Office 전부가 받는다). 잘라내기는 평문만.

## 4. 상태줄 설계 ✅(1차) — Sublime · DBeaver · Golden 참고

참고: Sublime = 왼쪽 메시지(파일 상태·git) / 오른쪽 `main ⑴ · UTF-8 with BOM · Windows · Tab Size: 4 · SQL`(클릭 = 팝업). DBeaver = 프로젝트 콤보 / `KST · en · Writable · Smart Insert · 382 : 5 [80]`. Golden = `Done, ran single statement.` / `344 : 1 · 52 rows · Script: 0.016s`.

```
[⏳] 메시지(마지막 실행 결과/오류/힌트) ………… │ 접속 │ Ln 34, Col 10 │ 52 rows │ 0.016s │ SQL │
```

| 세그먼트 | 내용 | 클릭(설계) | 상태 |
|---|---|:--|:--:|
| 메시지 | 실행 결과·오류·진행(⏳) — 왼쪽 · 넘치면 말줄임 | 로그 창 열기 | ✅ |
| 접속 | `프로필 · DB종류 · 사용자@호스트`(연결 전 `—`) · 색 = ●상태 | 접속 창(T-31) | ✅(문자) |
| 세션 모드 | `Auto-commit` / `Manual(변경 3)` · `Read Only` 배지 | 토글 | ☐ T-54 |
| 위치 | `Ln n, Col m` (+ 선택 시 `· 12 chars · 3 lines`) | Goto line(팔레트 `:줄`) | ✅ |
| 결과 | `52 rows · 0.016s`(마지막 조회 · 페치 진행 시 `1,000+ rows`) | 결과 탭 포커스 | ✅ |
| 인코딩·줄끝 | `UTF-8` · `CRLF`(파일 열기 T-16 이후) | 재열기/변환 메뉴 | ☐ |
| 탭 크기 | `Tab: 4`(설정 `editor.tab_size` 예정) | 팝업 | ☐ |
| 구문 | 활성 탭 구문 이름 | **팔레트 `Set Syntax:`** | ✅ |
| 시간대·언어 | DBeaver식 `KST · en`은 툴팁으로만(공간) | — | ☐ |

원칙: 오른쪽 세그먼트는 **오른쪽부터** 채우고 창이 좁으면 왼쪽 것부터 숨긴다(메시지 최우선 보존). 세그먼트는 `(텍스트, 클릭 동작)` 목록 — T-39 설정 화면에서 표시 여부를 켜고 끌 수 있게 `statusbar.*` 키로 등재 예정.

## 5. 세로 안내선 · 공백 표시 ✅ (설정 · `Editor` 카테고리)

| 키 | 기본 | 뜻 |
|---|---|---|
| `editor.rulers` | `80` | 글자 열 목록(쉼표) — 각 열에 1px 세로선(`Theme.border`) · 빈 값 = 없음. Sublime `rulers: [80, 120]`과 같다. |
| `editor.whitespace` | `selection` | `none` · `selection`(선택 영역 안만) · `all` — Sublime `draw_white_space`. |
| `editor.whitespace_chars` | `·→$` | 글자 3개 = 공백 · 탭 · 줄끝(`_` = 표시 안 함 · 예 `·→¶`). |
| `editor.whitespace_color` | (빈 값) | 16진 RRGGBB · 비우면 테마 흐린 글자색. |
| `editor.whitespace_alpha` | `40` | 불투명도 % — 배경(`field_bg`)과 섞어 그린다(별도 알파 블렌딩 없이 비용 0). |

줄끝 표시(`¶`)는 켰을 때만 전체 글자 벡터를 만든다(기본 꺼짐 = 비용 0).

## 6. 인텔리전스(자동 완성) ☐ 설계 — **끄면 비용 0 · 켜면 UI 스레드 무부하**

### 6-1. 원칙
1. **꺼짐 = 순수 편집기**: 설정 `intel.enabled=off`(기본 off까지는 아니고 **접속 전에는 off와 같다**)이면 훅이 하나도 등록되지 않는다 — 키 입력 경로에 분기 1개뿐.
2. **켜짐 = 모든 무거운 일은 워커 스레드**: 토큰 인덱스·메타 조회·후보 계산은 `intel` 스레드(우선순위 낮음). UI 스레드는 "요청 보내기"와 "도착한 후보 그리기"만 한다. 요청은 **세대 번호**를 달아 뒤늦은 응답은 버린다(취소).
3. **디바운스 + 예산**: 입력 후 120ms 무입력 시 요청(`.` 뒤는 즉시). 후보 계산 예산 30ms · 초과 시 부분 결과. 팝업은 ≤ 12행 가상화.
4. **메타는 캐시에서**: 테이블/컬럼/함수 목록은 [28](28-object-explorer.md) 탐색기 **Catalog 캐시**(메타 세션 · 요청별 스레드)를 공유한다. 캐시에 없으면 비동기로 채우고 그동안은 캐시된 것만 보여준다(팝업을 막지 않는다).
5. **결과 상한**: 후보 ≤ 500 · 컬럼 목록 ≤ 2,000 · 인덱스 메모리 예산(단어 인덱스 = 편집 중 문서의 식별자 집합 · 줄 단위 증분).

### 6-2. 후보 소스(우선순위)
| 순위 | 소스 | 예 |
|:--:|---|---|
| 1 | **Alias 컬럼**(§6-3) | `A.` → `LOG_START_TIME, LOG_END_TIME, …`(MTX_AGENT_LOG) |
| 2 | 현재 문장의 테이블·alias | `FROM … A, … C` → `A`, `C`, `MTX_DBMS` |
| 3 | 스키마 오브젝트(Catalog) | 테이블·뷰·프로시저·패키지·시퀀스·시노님 |
| 4 | **DBMS 시스템 오브젝트 · 내장 함수**(§6-4) | `DBMS_OUTPUT.`, `NVL(`, `sys.objects`, `pg_catalog.` |
| 5 | 키워드(구문 규격) | `SELECT`, `WHERE` |
| 6 | 문서 단어 인덱스 | 방금 친 식별자·바인드 `:V_USER` |

### 6-3. Alias 탐지(Smart) — 사용자 예시
```sql
FROM MTXRPTY2.MTX_AGENT_LOG A, MTXRPTY2.MTX_DBMS C, MTXRPTY2.MTX_REPORT D, MTXRPTY2.MTX_SQL E
```
- `nsql_script::statement_at`으로 **캐럿 문장**만 본다(문서 전체 파싱 없음). 문장 안 `FROM`/`JOIN`/`UPDATE`/`INTO`/`MERGE INTO`/`DELETE FROM` 뒤 `[schema.]table [AS] alias` 패턴을 토큰 단위로 걷어 `alias → (schema, table)` 표를 만든다(콤마 조인 · ANSI JOIN · 서브쿼리 `(SELECT …) X`는 서브쿼리 SELECT 목록을 컬럼으로 · CTE `WITH X AS (…)`도 같은 규칙). 예약어(`WHERE ON AND OR`)는 alias로 잡지 않는다.
- `A.` 입력 → 표에서 `A`= `MTXRPTY2.MTX_AGENT_LOG` → Catalog 캐시 `columns(schema, table)` → 팝업(컬럼명 · 타입 · NULL 여부 · 코멘트 툴팁). 스키마 생략 시 접속 기본 스키마 → 시노님 → PUBLIC 순.
- alias 없이 `MTX_AGENT_LOG.`도 같은 경로. `A.LOG_` 처럼 접두가 있으면 접두 필터(대소문자 무관 · 퍼지 아님 — 컬럼은 접두 일치가 직관적).
- 표는 **문장 텍스트 해시**로 캐시 — 같은 문장이면 재파싱 0.

### 6-4. DBMS별 시스템 오브젝트 · 내장 함수(접속 감지)
- 접속되면 방언(`Dialect`)에 따라 **내장 카탈로그 패키지**를 켠다: `Packages/Intel/<dialect>.nexa-intel`(데이터 파일 · 플러그인과 같은 배치 · 갱신 가능):
  - Oracle: `DBMS_*`/`UTL_*` 패키지와 서브프로그램 시그니처 · `V$`/`DBA_`/`ALL_`/`USER_` 뷰 · 내장 함수(`NVL`, `TO_CHAR(x, fmt)`, `LISTAGG`, 분석 함수) · 의사 컬럼(`ROWNUM`, `ROWID`, `SYSDATE`).
  - MSSQL: `sys.*` 카탈로그 뷰 · `sp_*` 시스템 프로시저 · `@@` 함수 · 내장 함수(`ISNULL`, `DATEADD`, `STRING_AGG`) · 힌트.
  - PostgreSQL: `pg_catalog.*` · `information_schema` · 내장 함수 · 연산자 · `::` 캐스트 타입.
  - SQLite: `sqlite_master` · 내장 함수 · pragma.
- 표시: 후보에 종류 아이콘(패키지·함수·뷰·컬럼) · 함수는 **시그니처 힌트**(`TO_CHAR(value, format)` — 인자 위치 굵게) · 시스템 오브젝트는 흐린 색으로 구분. 서버에서 실제 목록을 못 가져와도(권한) 정적 파일로 동작 → 오프라인에서도 같다.
- 강조에도 연결: 접속 방언의 내장 함수는 `TokenKind::Builtin`(신설 예정 · 색 `syn_builtin`)로 칠한다 — 규격 파일 `builtins =` 키 + 접속 시 방언 내장 목록 병합.

### 6-5. UI
- 팝업 = 캐럿 아래 카드(≤ 12행 가상화 · ↑↓·Tab/Enter 확정·Esc) · 오른쪽에 종류·타입 · 300ms 머물면 상세 툴팁(코멘트·시그니처).
- 트리거: `.` · `Ctrl+Space`(수동) · 식별자 2글자 이상 + 디바운스. 문자열/주석 안에서는 뜨지 않는다(강조 상태 재사용).
- 설정: `intel.enabled` · `intel.auto_popup` · `intel.delay_ms`(120) · `intel.budget_ms`(30) · `intel.max_items`(500) · `intel.system_objects`(on).
- CLI: `nsql shell`의 Tab 완성이 같은 엔진(스레드 대신 동기 · 예산 동일).

## 7. 다중 커서 · 선택 편집 🚧 (E5 · Sublime 키 그대로) — ✅ 09-16 48차 T-98: 줄 편집 명령 14종 + `Ctrl+K` 2단 코드 + 캐럿 추가 ↑/↓ + 찾기 일치 전부 표시 · 잔여 = `Ctrl+K, Ctrl+D` 건너뛰기 · `Ctrl+U` 소프트 되돌리기 · `Alt+Enter` 전부 선택

| 기능 | 키(Win/Linux · mac) | 동작 |
|---|---|---|
| 다음 항목 추가 선택 | `Ctrl+D` · `⌘D` | 현재 단어(또는 선택)와 같은 다음 텍스트를 **선택에 추가** · `Ctrl+K, Ctrl+D` 건너뛰기 · `Ctrl+U` 되돌리기 |
| 모두 선택 | `Alt+F3` · `Ctrl+⌘G` | 같은 텍스트 전부 다중 선택 |
| 줄로 쪼개기(컬럼 모드) | `Ctrl+⇧L` · `⌘⇧L` | 여러 줄 선택 → **줄마다 커서 1개**(줄 끝) · 이어서 Home/End/입력이 모든 줄에 |
| 컬럼 선택 | `⇧+우클릭 드래그` · `Ctrl+Alt+↑/↓` · `⌥ 드래그` | 사각 블록 — 짧은 줄은 가상 공간 없이 줄 끝 |
| 정규식 찾기 → 전부 선택 | 찾기 패널(`Ctrl+F`) 정규식 토글 → **`Alt+Enter`** | 매치 전부 다중 선택 → 편집 · `Ctrl+H` 바꾸기(`$1` 캡처) |
| 커서 취소 | `Esc` | 첫 커서만 남김 |

- 모델: `Selection = Vec<Region{anchor, head}>` **정렬·병합 불변식**(겹치면 합침). 편집 명령은 *모든* 리전에 **뒤에서 앞으로** 적용해 오프셋을 흐트러뜨리지 않는다. 트랜잭션 1개 = Undo 1번([17](17-editor-incremental-plan.md) E1 History).
- 그리기: 커서마다 캐럿 + 리전 반전(§블록 선택 규칙 — 행 피치 높이 · 줄 넘김 포함 시 오른쪽 끝까지 ✅ nexa-ui 29fe8d7).
- 정규식: **자체 엔진**(백트래킹 없는 Thompson NFA · 문자 클래스·그룹·수량자·앵커·`\b`·캡처 · 대소문자 옵션) — 외부 crate 0. 찾기 패널이 먼저 쓰고 `.sublime-syntax`(T-59)와 인텔리전스 토크나이저가 뒤에 같이 쓴다. 시간 예산(큰 파일 증분 검색 · 보이는 영역 우선 강조).
- 붙여넣기: 커서 수 = 줄 수면 줄별 분배(Sublime 규칙).

### 7-1. 커서 이동 규칙(Sublime 기본 키맵 · ✅ 09-17 사용자 요청)

| 동작 | Win/Linux | mac | 규칙 | 상태 |
|---|---|---|---|---|
| 단어 이동 | Ctrl+←/→ | ⌥←/→ | `words`/`word_ends`: 공백 건너뛰고 같은 부류(단어=영숫자·`_`·비ASCII · 구분자) 런의 시작/끝 · 줄바꿈은 한 번에 하나 | ✅ `Key::WordLeft/Right` |
| 서브워드 | Alt+←/→ | ⌃←/→ | `_` 양쪽 · 소→대 · `HTML|Parser` · 글자↔숫자 | ✅ `Key::SubwordLeft/Right` |
| 줄 처음/끝 | Home/End | ⌘←/→ · Home/End | Home = **스마트**(첫 글자 ↔ 열 0) · End = 줄 끝 | ✅ |
| 문서 처음/끝 | Ctrl+Home/End | ⌘↑/↓ · Ctrl+Home/End | bof/eof | ✅(Ctrl+Home/End) · ⌘↑/↓ 후속 |
| 선택 확장 | +Shift | +⇧ | 위 전부 | ✅ |
| 캐럿 추가 | Ctrl+클릭 | ⌘클릭 | 같은 자리 = 제거 · 마지막 하나 유지 | ✅ `toggle_caret` |
| 열 선택 | Alt+Shift+드래그 · 가운데 드래그 | ⌥드래그 | 시작 Col보다 짧은 줄 제외 · 끝 Col은 포인터 기준 줄마다 클램프 | ✅(가운데 버튼 후속) |
| 괄호 짝 | Ctrl+M | ⌃M | `move_to: brackets` | ☐ |
| 괄호 안 확장 | Ctrl+Shift+M | ⌃⇧M | `expand_selection: brackets` | ☐ |
| 스코프 확장 | Ctrl+Shift+Space | ⌃⇧Space | `expand_selection: scope` | ☐ |
| 스크롤만 | Ctrl+↑/↓ | ⌃⌥↑/↓ | `scroll_lines`(캐럿 유지) | ☐ |
| 다중 커서 실행 | — | — | 문장 실행(Ctrl+Enter)·툴바 버튼 **차단** · 전체 실행만 | ✅ |

## 8. 창 포커스 ✅ (설정 `window.focus` · 카테고리 Window)

- `group`(기본): 창 하나(메인/로그)를 선택하면 **모든 창이 함께 앞으로**, 현재 z-order 유지 · 방금 선택한 창만 맨 위. Windows = `SetWindowPos(HWND_TOP, SWP_NOACTIVATE)`(포커스 안 뺏음 · user32 FFI). macOS는 AppKit 기본이 이와 같고 Linux는 WM 정책이라 no-op(문서화).
- `single`: 선택한 창만 활성화 · 나머지는 그대로.

## 9. 후속 항목
T-55 ✅(탭) · **T-57** 인텔리전스(§6) · **T-58** 다중 커서·정규식 찾기(§7 · E-2/E-3과 합류) · **T-59** 정규식 엔진 + `.sublime-syntax` 호환 + 강조 상태 캐시 · **T-60** Linux 다중 형식 클립보드 · macOS 서식 복사 실기 · **T-61** 상태줄 세그먼트 설정(`statusbar.*`)·세션 모드·인코딩·탭 크기.
