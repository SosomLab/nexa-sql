# 49 · Auto Indent — 변수 정의 · 동작 규칙(Sublime 1순위 · VS Code 보조) · 구현

> **상태**: ✅ 1차 구현(09-17 52차 · 사용자 요청 "Auto Indent 설계 · Sublime 기준, 부족하면 VS Code · 정의할 변수와 동작을 먼저 정리한 뒤 개발"). 선행 = [31 들여쓰기 설정 계층](31-indentation-settings.md)(tab_size · 공백/탭 · 탭 정지점 · 감지 · 계층) · [29 §7](29-editor-syntax-palette-statusbar.md)(편집 명령) · [12 사용자 Sublime 프로필](12-user-sublime-profile.md).

## 0. 결론 다섯 줄

1. Sublime의 auto indent는 설정 4개(`auto_indent` · `smart_indent` · `indent_to_bracket` · `trim_automatic_white_space`)와 문법 메타데이터 4패턴(`increaseIndentPattern` · `decreaseIndentPattern` · `bracketIndentNextLinePattern` · `unIndentedLinePattern`)으로 끝난다. VS Code는 같은 골격에 `onEnterRules`(beforeText/afterText → indent/outdent/indentOutdent/appendText)와 `editor.autoIndent` 5단(none/keep/brackets/advanced/full)을 더한다.
2. 우리는 **Sublime 변수 4개를 그대로** 설정 키로 두고, 문법 규칙은 **규칙 세트 하나**(`editor.indent_rules` = sql/brackets/none)로 고른다. SQL 세트 = 괄호 + PL/SQL·T-SQL 블록 키워드.
3. 동작은 세 자리에서만 일어난다: **Enter**(유지 · 증가 · 괄호 사이 indentOutdent · 괄호 열 정렬) · **닫는 토큰 입력**(`)` `END` … 줄 앞이면 한 단계 내어쓰기) · **캐럿이 떠날 때**(자동으로 넣은 공백만 남은 줄은 비움).
4. 들여쓰기 단위·문자는 [31](31-indentation-settings.md) 계층 값(`tab_size` · `indent_spaces` · 탭 정지점) — 여기서 새로 정하지 않는다.
5. 다중 커서에서는 **유지(auto)만**(증가/정렬은 주 캐럿 기준으로 애매) · 붙여넣기는 건드리지 않는다(VS Code `formatOnPaste`는 포매터 몫 · 후속).

## 1. 조사 — Sublime · VS Code

| 항목 | Sublime Text(Preferences · `.tmPreferences`) | VS Code(`editor.*` · `language-configuration.json`) | 우리 |
|---|---|---|---|
| 새 줄 = 현재 줄 들여쓰기 유지 | `auto_indent: true` | `autoIndent: keep` 이상 | `editor.auto_indent`(on) |
| 열림 뒤 한 단계 증가 · 닫힘 입력 시 감소 | `smart_indent: true` + `increaseIndentPattern`/`decreaseIndentPattern` | `autoIndent: brackets`(괄호) · `advanced`(indentationRules) · `full`(onEnterRules) | `editor.smart_indent`(on) + `editor.indent_rules` |
| 괄호 사이 Enter → 가운데 줄 +1, 닫힘 줄 원래 | (smart_indent 포함 · 괄호쌍) | `onEnterRules: indentOutdent` | ✅ Enter 규칙 ③ |
| 열린 괄호 열에 정렬 | `indent_to_bracket: false` | (없음 · 확장이 제공) | `editor.indent_to_bracket`(off) |
| 자동 공백만 남은 줄 비우기 | `trim_automatic_white_space: true` | `trimAutoWhitespace: true` | `editor.trim_auto_whitespace`(on) |
| 다음 한 줄만 들여쓰기(`if (x)` 뒤) | `bracketIndentNextLinePattern` | `indentNextLinePattern` | 후속(SQL엔 드묾) |
| 들여쓰기 무시 줄 | `unIndentedLinePattern` | `unIndentedLinePattern` | 후속(주석 줄) |
| 들여쓰기 감지 | `detect_indentation` | `detectIndentation` | 31 §2-3(Guess) |
| 붙여넣기 재정렬 | (없음 · `reindent` 명령) | `formatOnPaste` | 후속(포매터 T-계획) |
| 탭/공백·폭·정지점 | `tab_size` · `translate_tabs_to_spaces` · `use_tab_stops` | `tabSize` · `insertSpaces` · `useTabStops` | 31 ✅ |

## 2. 변수 정의(설정 키 · 전부 [24](24-settings-and-vscode-analysis.md) 레지스트리 · 카테고리 편집기)

| 키 | 종류 · 기본 | 동작 | 끄면 |
|---|---|---|---|
| `editor.auto_indent` | Bool · **on** | Enter 때 현재 줄의 앞 공백(탭/공백 그대로)을 새 줄에 복사 | Enter = 개행만(아래 전부 꺼짐) |
| `editor.smart_indent` | Bool · **on** | 규칙 세트로 **증가**(Enter 직전 텍스트가 열림으로 끝남) · **감소**(줄 앞에 닫는 토큰을 입력) · **괄호 사이** indentOutdent | 유지만 |
| `editor.indent_rules` | Choice `sql`/`brackets`/`none` · **sql** | `brackets` = `(`·`[`·`{` ↔ `)`·`]`·`}` · `sql` = brackets + 증가 키워드 `BEGIN THEN ELSE ELSIF LOOP DECLARE IS AS CASE EXCEPTION DO` + 감소 키워드 `END ELSE ELSIF WHEN EXCEPTION UNTIL` · `none` = 규칙 없음 | — |
| `editor.indent_to_bracket` | Bool · **off** | Enter 직전 줄에 닫히지 않은 `(`가 있으면 새 줄을 **그 괄호 다음 열**에 맞춘다(공백으로 · 탭은 `tab_size`로 환산) — 증가 규칙보다 우선 | 증가 규칙 |
| `editor.trim_auto_whitespace` | Bool · **on** | 자동으로 넣은 들여쓰기만 있는 줄에서 캐럿이 떠나면(위/아래/클릭/Enter 한 번 더) 그 공백을 지운다 · 사용자가 한 글자라도 치면 해제 | 공백 남음 |
| (기존) `editor.tab_size` · `editor.indent_spaces` · `editor.tab_stops` | 31 | 단위 = `indent_spaces`면 공백 `tab_size`개 · 아니면 `\t` | — |

종속 잠금: `auto_indent=off` → `smart_indent`·`indent_rules`·`indent_to_bracket`·`trim_auto_whitespace` 잠김 · `smart_indent=off` → `indent_rules` 잠김.

## 3. 동작 규칙(Enter · 입력 · 이탈)

**Enter**(멀티라인 · 조합 중 아님 · 단일 캐럿):
1. `ws` = 현재 줄 앞 공백(탭/공백 원문) · `before` = 줄 시작~캐럿(문자열·주석 안은 무시하지 않음 — 1차) · `after` = 캐럿~줄 끝.
2. `indent_to_bracket`이고 `before`에 닫히지 않은 `(` `[` `{`가 있으면 `ws` = 그 괄호 **다음 열**까지 공백.
3. 아니면 `smart_indent`이고 `before.trim_end()`가 열림 괄호로 끝나거나 마지막 단어가 증가 키워드(대소문자 무시)면 `ws += unit`.
4. 3에서 열림 괄호였고 `after.trim_start()`가 그 짝 닫힘으로 시작하면 **indentOutdent**: `"\n" + ws+unit + "\n" + ws_원래`를 넣고 캐럿은 가운데 줄 끝.
5. 넣은 들여쓰기가 비어 있지 않으면 그 줄을 `auto_ws_line`으로 기억(이탈 규칙).
- 다중 캐럿: 1만(캐럿마다 자기 줄의 `ws`) — 증가·정렬·indentOutdent 없음.

**닫는 토큰 입력**(`smart_indent`): 글자를 넣은 뒤 현재 줄이 `공백 + 닫는 토큰`과 정확히 같으면(`)` · `END` · `ELSE` …) 줄 앞 공백에서 **한 단위**를 뺀다(공백이면 `tab_size`개 · 탭이면 1개 · 모자라면 전부). 같은 줄에서 두 번 빼지 않는다(`outdented_line` 기억).

**이탈**(`trim_auto_whitespace`): 키/마우스 처리 뒤 캐럿 줄이 `auto_ws_line`과 다르고 그 줄이 공백만이면 공백 삭제(히스토리 한 단계). 사용자가 그 줄에 글자를 넣으면 기억 해제.

## 4. 구현(nexa-ctl `TextBox` · nexa-sql 배선)

- nexa-ctl `controls/textbox.rs`: `AutoIndent { enabled, smart, to_bracket, trim }` + `IndentRules { open, close, increase_words, decrease_words }`(`sql()`/`brackets()`/`none()`) · `set_auto_indent(cfg, rules)` · Enter 경로 `newline_with_indent()` · `Char` 경로 뒤 `outdent_on_close()` · 이벤트 끝 `trim_auto_ws()` · 테스트 4(유지·증가·indentOutdent·닫힘 내어쓰기·비우기).
- nexa-sql: 설정 5키 등록 + `apply_indent`에서 편집기 전 탭에 전달 · 상태줄 들여쓰기 세그먼트는 그대로.
- 부하: Enter/글자 입력 때 현재 줄 문자열만 본다(O(줄 길이)) · 타이머·스레드 0.

## 5. 후속

`bracketIndentNextLinePattern`/`unIndentedLinePattern`(주석 줄) · 문자열/주석 안 괄호 무시(강조 토큰 재사용) · 붙여넣기 재정렬(`reindent` 명령 · 포매터) · 문법 패키지의 `.tmPreferences` 읽기(DR-5 Sublime 패키지 호환) · 다중 캐럿 증가 규칙.
