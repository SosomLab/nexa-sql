# 31. 들여쓰기(탭 크기·공백) 설정 계층 — Sublime · VS Code · IntelliJ 비교와 권장 설계 (2026-09-15)

> **상태**: 📐 설계 권장안(사용자 요청 09-15 · 캡처 2장 = Sublime 상태바 "Tab Size: 4" · 클릭 팝업). 구현 = **T-69**. 상위 = [09 편집기·패키지](09-editor-and-packages.md)(`.sublime-settings` 계층 → `nexa-conf`) · [29 상태줄](29-editor-syntax-palette-statusbar.md) · [18 세션](18-session-and-projects.md) · [30 확장점 원장](30-architecture-patterns.md).

## 1. 세 제품 비교(연구 요약)

| 항목 | Sublime Text | VS Code | IntelliJ |
|---|---|---|---|
| 설정 계층(아래로 갈수록 우선) | Default → Packages → **User** → **문법별**(`SQL.sublime-settings`) → **프로젝트**(`.sublime-project` settings) → **뷰**(버퍼 · 세션에만) | Default → User → Workspace → Folder → **언어별**(`[sql] { editor.tabSize }`) → **문서**(메모리) · `.editorconfig`(확장) | IDE 스킴 → **프로젝트 코드 스타일** → **언어별**(Tabs and Indents) → **EditorConfig** → **파일 자동 감지** → **파일**(상태바 위젯 · 메모리) |
| 키 | `tab_size` · `translate_tabs_to_spaces` · `detect_indentation` · `use_tab_stops` · `trim_trailing_white_space_on_save` | `editor.tabSize` · `editor.insertSpaces` · `editor.detectIndentation` · `editor.useTabStops` · `files.trimTrailingWhitespace` | Use tab character · Tab size · Indent · Continuation indent · Detect and use existing file indents |
| 상태바 | `Tab Size: 4` / `Spaces: 4` — 클릭 = 팝업(캡처 2) | `Spaces: 4` / `Tab Size: 4` — 클릭 = 퀵픽 | `4 spaces` / `Tab` — 클릭 = 팝업 |
| 팝업 항목 | Indent Using Spaces(토글) · Tab Width 1~8 · **Guess Settings From Buffer** · Convert Indentation to Spaces / Tabs | Indent Using Spaces / Using Tabs · Change Tab Display Size · **Detect Indentation from Content** · Convert Indentation to Spaces / Tabs · Trim Trailing Whitespace | Set indent size(이 파일) · Use tab character · **Detect indents** / Disable detection · Configure Indents for <언어>… · Reindent |
| 자동 감지 | 파일 열 때(`detect_indentation`) · 뷰에만 적용 | 파일 열 때(`detectIndentation`) · 문서에만 | 파일 열 때 · 파일에만 · 감지 결과와 스타일이 다르면 상태바에 알림 |
| 감지 결과의 영속 | 안 함(세션에 뷰 설정 저장) | 안 함 | 안 함 |
| 상위 계층으로 올리기 | 없음(설정 파일 직접 편집) | 없음(설정 편집) | **있음**(팝업 → Configure Indents for Language…) |

세 제품 공통: **감지·수동 변경은 "현재 문서"에만** 적용되고 영속되지 않는다 · 언어(문법)별 계층이 있다 · 변환(Convert)은 버퍼 변환 명령이다. IntelliJ만 팝업에서 **상위 계층 설정으로 바로 갈 수 있다** — 가져올 가치가 있다.

## 2. 권장 설계 — 사용자 제안(전역 → 확장자 → 현재 탭)을 그대로 채택하고 두 칸을 보탠다

```
Default(REGISTRY)  →  전역 User(settings.conf)  →  문법/확장자(Packages/<pkg>/<Syntax>.nexa-settings)
                   →  프로젝트(.nexa-project · 18 · 나중)  →  현재 탭(메모리 · 세션 hot exit에만 저장)
```
- **해석 = 가까운 계층 우선**(nearest wins) · 값이 없으면 상위로.
- **현재 탭 계층**은 ① 파일을 열 때 자동 감지(`editor.detect_indent` 켜짐이면) ② 상태바 팝업의 수동 선택으로 채워지고, **디스크 설정을 바꾸지 않는다**(세 제품 공통 관례). 세션 복원([18](18-session-and-projects.md))에는 탭 메타로 함께 저장.
- **상위로 올리기**(IntelliJ식): 팝업 아래에 `이 문법의 기본으로` · `전역 기본으로` 두 항목 — 현재 값을 그 계층 파일에 쓴다. 사용자가 "이 값으로 계속"을 원할 때 설정 파일을 열지 않아도 된다.
- 확장자 계층은 **문법 단위**로 둔다(`.sql`/`.pks`/`.pkb`가 한 문법 SQL을 가리키므로 확장자보다 문법이 안정 계약 · Sublime 동일). 파일은 [09](09-editor-and-packages.md)의 `Packages/<pkg>/<Syntax>.nexa-settings`(key=value · `nexa-conf`).

### 2-1. 설정 키(레지스트리 · 노출)

| 키 | 종류 | 기본 | 뜻 |
|---|---|---|---|
| `editor.tab_size` | Int 1~16 | 4 | 탭 폭(문자 수) — 표시·공백 삽입 단위 |
| `editor.indent_spaces` | Bool | on | Tab 키 = 공백 삽입(SQL 관례 · VS Code 기본과 같음 · Sublime은 off) |
| `editor.detect_indent` | Bool | on | 파일 열 때 버퍼에서 감지해 **현재 탭 계층**에 넣는다 |
| `editor.trim_trailing_ws` | Bool | off | 저장 시 줄 끝 공백 제거(팝업엔 없음 · 설정 화면) |

문법 계층 파일 예 — `Packages/SQL/SQL.nexa-settings`: `tab_size = 2` · `indent_spaces = on`(키 이름은 레지스트리 키의 `editor.` 접두어를 뗀 것).

### 2-2. 상태바 세그먼트(캡처 1 · [29](29-editor-syntax-palette-statusbar.md) 표에 추가)
- 표시: 공백 모드면 `Spaces: 4`, 탭 모드면 `Tab Size: 4`(Sublime 표기). 값의 **출처 계층**을 툴팁에(`Tab Size: 4 · SQL 문법 설정`).
- 클릭 = 팝업(nexa-ctl `ContextMenu` · id만 돌려주는 규약 그대로):
  1. `Indent Using Spaces`(체크 토글 = 현재 탭 계층에 `indent_spaces`)
  2. 구분선 · `Tab Width: 1` ~ `8`(현재 값 ✓ · 현재 탭 계층)
  3. 구분선 · `Guess Settings From Buffer`(감지 실행 · 결과를 현재 탭 계층에)
  4. 구분선 · `Convert Indentation to Spaces` · `Convert Indentation to Tabs`(버퍼 변환 · 되돌리기 1단계)
  5. 구분선 · `이 문법(SQL)의 기본으로 저장` · `전역 기본으로 저장`(IntelliJ식 · 파일에 씀)
- 팔레트에도 같은 명령(`Indent: …`) — T-67 Command 레지스트리가 오면 한 표에서.

### 2-3. 편집기 동작(nexa-ctl `TextBox` 멀티라인)
- **탭 폭 렌더링**: 지금 `nexa-gfx::Font::control_advance`가 `\t`를 **4칸 고정**으로 잰다(하드코딩 · 30 §0-4 위반) → 측정 API에 탭 폭을 주입(`Font::measure_with_tab(text, size, tab_cols)` 또는 `RasterCtx.set_tab_size`) · 탭은 **탭 정지점**(다음 배수 열)으로 잰다(Sublime `use_tab_stops`).
- **Tab 키**: 선택 없음 → `indent_spaces`면 다음 탭 정지점까지 공백, 아니면 `\t` · 선택이 여러 줄이면 **줄 들여쓰기** · Shift+Tab = 내어쓰기 · 단일 행 텍스트박스는 포커스 이동 유지.
- **감지 휴리스틱**(Guess): 줄 앞 공백만 본다 — 탭으로 시작하는 줄 수 vs 공백으로 시작하는 줄 수로 모드 결정 · 공백이면 연속 줄 들여쓰기 차이의 최빈값(2·4·8 후보)으로 폭 결정 · 표본 200줄 · 표본이 없으면 상위 계층 값 유지(VS Code 방식).
- **변환**: 줄 앞 공백만 변환(문자열 리터럴 안은 건드리지 않음 — 줄 앞만이라 안전) · 편집 히스토리 1단계.

### 2-4. 세션 · 프로젝트
- 탭 계층 값은 세션 파일의 탭 메타(`indent: {tab_size, spaces}`)로 저장 → hot exit 복원 시 그대로.
- 프로젝트 계층은 [18](18-session-and-projects.md) `.nexa-project`가 생길 때 같은 키로 끼운다(해석기는 계층 목록만 늘린다).

## 3. 왜 이 순서인가(결정 근거)
- 사용자 제안(전역 → 확장자 → 현재 탭)이 세 제품의 공통 골격과 같다. 확장자 대신 **문법**으로 둔 것만 다르며, Sublime과 같은 이유(한 문법 = 여러 확장자).
- "현재 탭은 영속하지 않는다"는 세 제품 모두의 관례 — 파일마다 다른 들여쓰기를 존중하면서 설정 파일을 어지럽히지 않는다.
- IntelliJ의 "상위 계층으로 올리기"는 설정 파일을 열게 만드는 마찰을 없앤다 — 우리 팝업의 마지막 두 항목.
- 확장점 규칙([30 §1](30-architecture-patterns.md)): 해석기는 **계층 목록**(포트) + 계층별 저장소(레지스트리 · 파일) + 선택(설정) — 프로젝트 계층은 목록에 한 줄 추가로 끝난다.

## 4. 작업 — T-69(순서)
1. nexa-gfx 탭 폭 주입 + 탭 정지점 측정 · TextBox `set_indent(tab_size, spaces)` · Tab/Shift+Tab 동작(선택 줄 들여쓰기).
2. `nsql-settings` 키 4개 · 문법 계층 파일 로더(`syntax.rs` 옆 · `nexa-conf`) · 해석기 `IndentResolver`(계층 목록).
3. 감지 휴리스틱 + 변환 명령(단위 테스트: 탭/공백/혼합/빈 버퍼).
4. 상태바 세그먼트 + 팝업 + 팔레트 명령 · 세션 메타 저장.
