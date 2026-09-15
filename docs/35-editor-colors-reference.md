# 35 · 편집기 색 배치·선택 모드 참조 — DBeaver · Golden · (Sublime) 조사

> **요청**(사용자 09-15): *"DBeaver의 SQL 문법에 대한 색상 배치 정보를 조사해 줄 수 있어?"* · *"Golden의 색상 표시와 선택 모드를 Study 해줘"*.
> **방법**: 두 도구 모두 **이 기기의 설치본**에서 읽었다(문서·기억이 아니라 실제 값). DBeaver = `C:\Program Files\DBeaver\plugins\org.jkiss.dbeaver.ui.editors.sql_1.0.185.202608161900.jar`의 `plugin.xml` + 다크 CSS + Eclipse 편집기 기본값 클래스 · 사용자 워크스페이스 prefs. Golden = `C:\Program Files\Benthic\Golden8_64bit.exe`(Golden 8 · SynEdit 엔진)의 옵션 폼 리소스 + 레지스트리 `HKCU\Software\Benthic\Golden8` + 동봉 매뉴얼 PDF 캡처 픽셀. 읽기 전용(설치본·워크스페이스·레지스트리 변경 0).
> **용도**: 구문 강조 테마(`Theme.syn_*` · `.nexa-syntax` 팔레트) · 선택 렌더링 규칙(T-57 인텔리전스 전 단계) · 사용자 취향 반영의 근거. **결정은 D-53**(§4).

---

## 1. DBeaver — SQL 편집기 토큰 색

사용자의 DBeaver는 **라이트 테마 · 색 오버라이드 0**(워크스페이스 prefs에 SQL 색 키 없음 · 글꼴 D2Coding 12) → 아래 (a)가 그대로 보이는 값이다. 키 접두 `org.jkiss.dbeaver.sql.editor.color.` 생략.

### (a) 라이트(기본)

| 분류 | 키 | RGB | 비고 |
|---|---|---|---|
| 키워드 | `keyword.foreground` | **#800000** | SELECT/FROM/WHERE (SWT `COLOR_DARK_RED`) |
| 자료형 | `datatype.foreground` | #000080 | VARCHAR2 · NUMBER |
| 함수 | `function.foreground` | #000080 | NVL · COUNT |
| 문자열 | `string.foreground` | **#008000** | `'…'` |
| 숫자 | `number.foreground` | #0000FF | |
| 주석 | `comment.foreground` | **#808080** | `--` · `/* */` **한 색**(블록 주석 별도 키 없음) |
| 구분자 | `delimiter.foreground` | #FF0000 | `;` · `GO` |
| 클라이언트 명령 | `command.foreground` | #800080 | `@set` · `@echo` |
| 바인드 파라미터 | `parameter.foreground` | #000080 | `?` · `:p` · `${v}` |
| SQL 변수 | `sqlVariable.foreground` | #000080 | |
| 테이블 / 별칭 | `table.foreground` · `table.alias.foreground` | #8E00C6 | **의미 강조**(접속 + `SQLEditor.Highlighting.advanced.enable`일 때만) |
| 컬럼 / 파생 컬럼 | `column.foreground` · `column.derived.foreground` | #006464 | 의미 강조 |
| 스키마 | `schema.foreground` | #956037 | 의미 강조 |
| 복합 타입 필드 | `composite.field.foreground` | #640064 | |
| 의미 오류 | `semanticError.foreground` | #6C5151 | 미해결 식별자 |
| 본문 / 배경 | `text.foreground` · `text.background` | #000000 / #FFFFFF | |
| 읽기 전용 배경 | `disabled.background` | OS `COLOR_WIDGET_BACKGROUND`(≈#F0F0F0) | 리터럴 아님 |
| AI 제안 | `aiSuggestion.foreground/background` | #808080 / #E8F2FE | 고스트 텍스트 |
| 경고 / 오류 텍스트 | `ui.general.color.warning/error.foreground` | #808000 / #800000 | 로그 패널 |

편집기 크롬(Eclipse `org.eclipse.ui.editors` 코드 기본값 — 라이트 CSS 없음):

| 분류 | 키 | RGB |
|---|---|---|
| 현재 줄 | `currentLineColor` | #E8F2FE |
| 줄번호 | `lineNumberColor` | #787878 |
| 인쇄 여백선 | `printMarginColor` | #B0B4B9 |
| 선택 배경/전경 | `AbstractTextEditor.Color.Selection*` | **OS 기본**(Windows 11 ≈ #0078D4 / #FFFFFF · 리터럴 없음) |
| 괄호 짝 | (색 키 없음) = 편집기 전경 `#000000` | `SQLEditor.matchingBrackets` on/off · 박스/채움 토글 |
| 식별자 출현(읽기/쓰기) | `occurrenceIndicationColor` / `writeOccurrence…` | #D4D4D4 / #F0D8A8 |
| 하이퍼링크 | `hyperlinkColor` | #0000FF |

### (b) 다크

| 분류 | RGB | | 분류 | RGB |
|---|---|---|---|---|
| 키워드 | #739ECA | | 자료형 / 함수 | #C1AA6C |
| 문자열 | #CAC580 | | 숫자 | #C0C0C0 |
| 주석 | #669768 | | 구분자 | #EECC64 |
| 명령 | #D3BAD3 | | 파라미터 / 변수 | #7EBAD3 |
| 테이블 / 별칭 | #B788D3 | | 컬럼 | #00B8B8 |
| 스키마 | #CC9B75 | | 복합 필드 | #D7B98C |
| 의미 오류 | #B19B9B | | 본문 | #9E9E9E |
| **편집기 배경** | **#2F2F2F**(`AbstractTextEditor.Color.Background` · DBeaver 키의 #000000은 하위 위젯용) | | 전경 | #CCCCCC |
| 선택 배경 / 전경 | #214283 / #93A1A1 | | 현재 줄 | #373737 |
| 줄번호 | #77919A | | 여백선 | #515658 |
| 찾기 범위 | #1E789B | | 출현 표시 | #1B6291 |
| 검색 결과 | #5E5E5E(박스) | | 하이퍼링크 | #66AFF9 |

**메모**: ① 블록/줄 주석 한 색 · 작은따옴표/큰따옴표 문자열 한 색. ② "활성 문장" 배경색 키 없음(DBeaver는 거터 표시). ③ 괄호 짝은 전용 색 없이 전경색 재사용.

---

## 2. Golden 8 — 색 · 선택 모드

설치본 = **Golden 8**(Benthic · 2025-07) · INI 없음 · 레지스트리에 **색 키 0**(전부 기본값) · 저장된 옵션 = `EditorFont2 = D2Coding 12` · `TabSize=4` · `UseSpaces=0` · `ShowLineNumbers/Bookmarks/ChangeTracker=1`. 편집 엔진 = **SynEdit**(pyscripter 계열 · Direct2D · `TSynSelection` 다중 캐럿) + Benthic 커스텀 SQL 하이라이터(토큰 18종 · 사용자 설정 가능은 3종).

### (a) 색

| 분류 | 색 | 스타일 | 근거 |
|---|---|---|---|
| 키워드 | **#800000**(clMaroon) | **굵게** | 옵션 폼 `SHKeyWordColor` · 매뉴얼 캡처 픽셀 |
| 주석 | **#008000**(clGreen) | 기울임(SynEdit 기본 · 미검증) | `SHCommentColor` |
| 문자열 | **#0000FF**(clBlue) | 보통 | `SHStringColor` · 캡처 |
| 내장 함수/패키지(SYSDATE) | #FF0000 | 굵게 | 캡처(설정 UI 없음) |
| 식별자·기호·숫자·`/` | #000000 | 보통 | `clWindowText` |
| 배경 | #FFFFFF | | |
| 바인드 변수 `:V` | **미확인**(하이라이터에 속성은 있으나 UI·레지스트리·샘플 없음) | | |
| 괄호 짝 | #FF00FF(clFuchsia) | | `BraceFGColor` |
| 다크 키워드/주석/문자열 | #FFFBF0 / #C0DCC0 / #A6CAF0 | | `Dark*Color` |
| **선택 배경** | OS `COLOR_HIGHLIGHT`(#0078D4) × **불투명도 115/255** → 흰 바탕에서 ≈ #8CC2EC | | `TSynSelectedColor.Opacity=115` |
| 선택 전경 | `COLOR_HIGHLIGHTTEXT`이나 반투명 워시라 **구문 색이 그대로 비친다** | | |
| 현재 줄 | 없음(`clNone`) | | `ActiveLineColor` |
| 변경 추적 띠 | #33A033 · 4px | | `TSynTrackChanges` |

사용자 캡처(Script_354.sql)와 대조: 키워드 굵은 적갈색 · 문자열 파랑 · 주석 초록 기울임 · 바인드 `:V_*` 검정 — **설치본 기본값과 일치**.

### (b) 선택 제스처

| 제스처 | 동작 |
|---|---|
| 본문 드래그 | 스트림 선택 · **기존 선택을 끌면 텍스트 이동**(`eoDragDropEditing` 기본 on) |
| 거터 클릭/드래그 | 줄 단위 선택(엔진 `sfGutterDragging` — 매뉴얼엔 없음) |
| 블록(열) 선택 | **없음** — 대신 **열 모양 다중 캐럿**(`ecSelColumnUp/Down…` = Alt+Shift+방향키) |
| Alt+클릭 | 캐럿 추가(타이핑은 전 캐럿에) |
| Alt+End(여러 줄 선택 뒤) | 각 줄 끝에 캐럿 · Home으로 줄머리/첫 비공백 토글 |
| Esc | 보조 캐럿·선택 전부 취소 |
| 더블클릭 | 단어 · **트리플클릭 없음** |
| Ctrl+M / Ctrl+Shift+M / Ctrl+Shift+A | 단어 선택 → 다음 출현 추가 / 마지막 추가 제거 / 전부 선택(= Sublime Ctrl+D 계열) |
| Tab·Shift+Tab(선택) | 블록 들여쓰기/내어쓰기 · Ctrl+- 주석 토글 · Ctrl+T 대소문자 · Ctrl+B 괄호 안 선택 |

### (c) 렌더링 차이(세 도구)

- **Golden**: 선택 = 반투명 워시(구문 색 유지) · 현재 줄 강조 없음 · 거터에 선택 표시 없음. 선택 블록이 줄 끝에서 멈추는지(캡처) vs 오른쪽 끝까지(`FillWholeLines` 기본 True)는 실행 없이 확정 불가 — **캡처를 따른다(줄 끝까지)**.
- **DBeaver**: 불투명 선택 블록 + 선택 전경색 교체 · 현재 줄 띠 · 줄번호 셀 강조.
- **Sublime**: 완전 선택된 줄은 편집기 전폭 · 현재 줄 띠 상시 · 거터 강조.

---

## 3. Nexa SQL 현재값과 비교

| 토큰 | Nexa(라이트 · `Theme.syn_*`) | DBeaver | Golden |
|---|---|---|---|
| 키워드 | 파랑 계열(현재) | #800000 | #800000 굵게 |
| 문자열 | 빨강 계열(현재) | #008000 | #0000FF |
| 주석 | 회색/초록(현재) | #808080 | #008000 기울임 |
| 숫자 | (현재 `syn_number`) | #0000FF | #000000 |
| 선택 | 불투명 `sel_bg` + 전폭(Sublime식 · 09-14 사용자) | 불투명 | 반투명 워시 |

---

## 4. 결정 대기 · 할일

| # | 질문 | 권장 |
|---|---|---|
| **D-53** | 기본 구문 팔레트를 어느 쪽에 맞출까 — ⓐ DBeaver 라이트(사용자가 매일 보는 값) ⓑ Golden(적갈색 굵은 키워드 · 파란 문자열) ⓒ 지금 값 유지 + **프리셋 3종**(`editor.color_preset = nexa|dbeaver|golden`)으로 전환 | **ⓒ** — 프리셋은 설정 키 하나 · 색 창(T-39)에서 개별 덮어쓰기 |
| **D-54** | 선택 렌더링 — 불투명(지금) / Golden식 반투명 워시(구문 색 유지 · `selection.opacity`) | 설정으로(기본 불투명) |
| **T-78** | 토큰 종류 확장 — 자료형·함수·바인드 파라미터·구분자·명령(`TokenKind` 5종 추가 · `.nexa-syntax` 팔레트 키) → 프리셋 표를 그대로 실을 수 있게 | — |
