# 51 · 첫 플러그인 — 레인보우 괄호(인용부호 포함) + 괄호 이동 메뉴 · 유사 패키지 조사 · 설계

> **상태**: 📐 설계(09-17 · 사용자 요청 "괄호·인용부호 `{ " ' [ ( <`를 rainbow로 순차 색 · 짝으로 이동 우클릭 메뉴 · '괄호 이동' 서브메뉴(짝·형제·상위·하위) · 유사 패키지 조사 · 설계 뒤 결정이 끝나면 개발"). 결정 = **D-91~D-95**(§7 · 사용자 답 대기). 선행 = [50 플러그인 시스템](50-plugin-system.md)(3층 · WASM 기본 · D-87~90 대기) · [29 §7-1](29-editor-syntax-palette-statusbar.md)(Ctrl+M 짝 이동 · Ctrl+Shift+M 괄호 안 확장 ✅) · [49](49-auto-indent.md) · [46](46-minimap-features.md).

## 0. 결론 여섯 줄

1. **첫 플러그인 = "in-process 플러그인"으로 먼저**: 플러그인 런타임(T-118 · WASM)이 아직 없으므로, 나중에 WASM이 쓸 **같은 호스트 API(`PluginHost`)** 를 정의하고 레인보우 괄호를 그 API 위에 **Rust 모듈로** 구현한다(외부화는 API 변경 없이). 이렇게 해야 플러그인 API가 "실제 필요"에서 나온다.
2. **색은 깊이 순환**(기본 6색 · 테마별 팔레트 · 설정) · **짝 없는 괄호 = danger** · 캐럿이 괄호 옆/안이면 **현재 쌍 강조**(밑줄·굵은 테두리) — Sublime BracketHighlighter + IntelliJ Rainbow Brackets의 교집합.
3. **문자열·주석 안 괄호는 제외**(구문 토큰 재사용 · SQL `'…'`는 `''` 이스케이프 · `"…"`/`[…]`는 식별자 인용 → 인용부호 자체는 **쌍으로 취급**하되 그 안은 스캔하지 않는다). `<>`는 SQL에서 연산자(`<>`, `<=`)라 **기본 제외**(설정으로 켬).
4. **이동 메뉴 "괄호 이동 ▸"**: 짝으로(Ctrl+M ✅) · 이전 형제 · 다음 형제 · 상위로 · 하위로 · 괄호 안 선택(Ctrl+Shift+M ✅) — 우클릭 메뉴와 편집 메뉴 양쪽 · 키맵 등록.
5. **SQL 특화 확장(조사 결과)**: `BEGIN…END` · `CASE…END` · `IF…END IF` · `LOOP…END LOOP` 같은 **키워드 쌍**도 같은 트리로(BracketHighlighter의 tag/custom 규칙 · 2순위).
6. **성능**: 편집마다 O(n) 스캔(문자열/주석 토큰 위를 걷는다) · 결과 = 쌍 표(`(open, close, depth)` 정렬) · 페인트는 보이는 창만 색 오버레이 · 상한(`rainbow.max_kb` 2048) 넘으면 끔 · 향상 모드 off.

## 1. 유사 패키지 조사(사용자 요청 "추가 구현 필요 기능 검토")

| 패키지 | 핵심 | 우리에게 가져올 것 | 우선 |
|---|---|---|---|
| **Sublime BracketHighlighter**(가장 많이 쓰임) | 짝 괄호·인용부호·HTML 태그 강조(밑줄/외곽선/거터 아이콘) · 문자열 안 괄호 별도 규칙 · **이동/선택/교체/제거 명령**(짝으로 · 안쪽 선택 · 괄호 포함 선택 · 괄호 종류 바꾸기 · 괄호 제거 · 감싸기) · 사용자 정의 쌍(regex) · 큰 파일 상한 | 현재 쌍 강조 · 이동/선택 · **괄호 종류 바꾸기·제거·감싸기** · 사용자 정의 쌍(키워드 쌍) · 상한 | 1(강조·이동) · 2(바꾸기/제거/감싸기) |
| Sublime RainbowBrackets | 깊이별 색 · 문자열/주석 제외 · 짝 없는 괄호 빨강 | 기본 기능 그대로 | 1 |
| **VS Code 내장 Bracket Pair Colorization**(2021~) | 깊이 색(6색 · 테마 토큰) · **bracket pair guides**(쌍을 잇는 세로 안내선 · `editor.guides.bracketPairs`) · 독립 색 풀(`independentColorPoolPerBracketType`) · `editor.bracketPairColorization.enabled` | **안내선**(들여쓰기 안내선 위에 쌍 색) · 괄호 종류별 독립 색 풀 옵션 | 2(안내선) |
| VS Code Bracket Pair Colorizer 2(퇴역) | 위와 같음 + "현재 스코프 강조"(캐럿이 든 쌍만 진하게) | **현재 스코프 강조**(밖은 흐리게) | 2 |
| **IntelliJ Rainbow Brackets** | 깊이 색 · **현재 스코프 강조/나머지 흐림**(Ctrl+우클릭) · 그라데이션 · 언어별 토글 · 코드 블록 선택 | 스코프 강조(옵션) · 블록 선택(Ctrl+Shift+M ✅) | 2 |
| Emacs rainbow-delimiters · Neovim nvim-ts-rainbow2 | 깊이 색(트리시터 기반 · 언어별 노드) | (SQL 트리 = 우리는 토큰 스캔) | — |
| Sublime 코어 `auto_match_enabled` · VS Code `autoClosingBrackets`/`autoSurround` | **괄호/인용부호 자동 닫기** · 선택 감싸기(`(` 입력 = `(선택)`) · 닫힘 타이핑 시 건너뛰기 | **자동 닫기·감싸기·건너뛰기**(49 auto indent와 짝) | 1(자동 닫기·감싸기) |
| VS Code `editor.matchBrackets` (always/near) | 캐럿 근처 짝 강조 시점 | 설정 `always`/`near` | 1 |
| Sublime `Expand Selection to Brackets` / `Goto Matching Bracket` | ✅ 이미 구현 | — | ✅ |

**추가 구현 권장(결론)**: ① 현재 쌍 강조(+ 짝 없음 빨강) ② **자동 닫기·감싸기·건너뛰기**(`rainbow`와 별개지만 같은 쌍 표를 씀) ③ 괄호 종류 바꾸기 `( )`↔`[ ]`↔`{ }` · 괄호 제거 · 선택 감싸기 ④ 쌍 안내선(세로선) ⑤ 현재 스코프 강조/나머지 흐림 ⑥ SQL 키워드 쌍(`BEGIN/END` · `CASE/END` · `IF/END IF` · `LOOP/END LOOP`) — ①②는 1순위 함께, ③~⑥은 2순위.

## 2. 쌍 표(데이터) — 한 번 계산해 전부가 쓴다

```rust
pub struct Pair { open: usize, close: usize, depth: u16, kind: PairKind /* Round|Square|Curly|Angle|DQuote|SQuote|BQuote|Keyword(u8) */ }
pub struct PairTable { pairs: Vec<Pair> /* open 오름차순 */, by_close: Vec<u32> /* close 정렬 인덱스 */, unmatched: Vec<(usize, PairKind)> }
```
- 스캔: 문자열/주석 토큰(강조기)을 건너뛰며 스택으로 짝짓기. 인용부호는 강조기가 이미 문자열 토큰으로 주므로 그 **양 끝**을 쌍으로 등록(안쪽은 스캔 X). 짝 없는 열림/닫힘 → `unmatched`.
- 조회: `pair_at(caret)`(캐럿 옆) · `enclosing(caret)`(이진 탐색 · 가장 안쪽) · `parent(pair)` · `children(pair)` · `siblings(pair)` = 같은 부모 아래 open 순서.
- 캐시: 텍스트 버전(`undo_len` + 길이 + 해시)으로 무효화 · 상한 초과 시 비움 + 기능 off 표시.

## 3. 표시

| 요소 | 규칙 | 설정 |
|---|---|---|
| 괄호 글자 색 | 깊이 `d % N`번째 팔레트 색(문자열 안 제외) | `rainbow.colors`(`#RRGGBB,...` · 비면 테마 기본 6색) · `rainbow.enabled` |
| 인용부호 | 열림/닫힘 따옴표 글자만 색(문자열 본문은 `syn_string` 그대로) | `rainbow.quotes`(on) |
| `<>` | 기본 제외(SQL 연산자) | `rainbow.angle`(off) |
| 짝 없음 | `danger` 색 + 물결 밑줄 | `rainbow.unmatched`(on) |
| 현재 쌍 | 캐럿이 괄호 옆/안 → 그 쌍 2글자에 밑줄 2px(+굵게) | `rainbow.match`(near/always/off) |
| 현재 스코프 강조 | 옵션: 캐럿이 든 쌍 밖의 괄호를 흐리게 | `rainbow.scope_dim`(off · 2순위) |
| 안내선 | 옵션: 쌍의 열림 열에 세로선(들여쓰기 안내선 위) | `rainbow.guides`(off · 2순위) |
| 성능 | `rainbow.max_kb`(2048) · 향상 모드 `rainbow.enabled=off` | 39 §4 |

렌더: nexa-ctl `TextBox::set_color_overlay(Vec<(usize, usize, Color)>)`(문자 범위 → 색 · 기존 구문 색 위에 우선) + `set_underlines(...)` — 호스트(플러그인)가 쌍 표에서 만든다. 오버레이는 정렬 배열 · 페인트는 보이는 행 범위만 이진 탐색.

## 4. 이동 메뉴 "괄호 이동 ▸"(우클릭 · 편집 메뉴 · 팔레트 · 키맵)

| 항목 | 동작 | 키(제안 · D-93) |
|---|---|---|
| 짝으로 이동 | ✅ Ctrl+M(캐럿 옆 괄호 짝 · 아니면 감싸는 쌍 닫힘) | Ctrl+M |
| 이전 형제 | 감싸는 쌍(또는 캐럿 옆 쌍)의 부모 아래 **앞** 쌍의 열림으로 | Ctrl+Alt+, |
| 다음 형제 | 부모 아래 **뒤** 쌍의 열림으로 | Ctrl+Alt+. |
| 상위로 | 감싸는 쌍의 부모 열림으로 | Ctrl+Alt+[ |
| 하위로 | 캐럿이 든 쌍의 첫 자식 열림으로 | Ctrl+Alt+] |
| 괄호 안 선택 | ✅ Ctrl+Shift+M(재입력 = 괄호 포함 → 바깥) | Ctrl+Shift+M |
| (2순위) 괄호 종류 바꾸기 / 제거 / 선택 감싸기 | `( )`→`[ ]`→`{ }` 순환 · 쌍 제거 · 선택을 `( )`로 | 팔레트 |

동작 규칙: Shift 조합은 선택 확장 · 대상이 없으면 상태줄 안내 · 이동 뒤 캐럿 추종 스크롤 · 다중 커서는 주 캐럿만.

## 5. 플러그인 구조(50과의 관계)

```
crates/nexa-sql/src/plugins/mod.rs      PluginHost 트레이트(호스트 API · WASM이 나중에 같은 표면을 노출)
  ├─ editor: text() · caret() · set_caret · select · syntax_tokens(range) · set_color_overlay · set_underlines
  ├─ ui: add_context_menu(submenu, items) · notify · status
  ├─ settings: get(own prefix) · on_change
  └─ events: on_edit(doc_version) · on_caret · on_settings
crates/nexa-sql/src/plugins/rainbow.rs  첫 플러그인(in-process): PairTable · 오버레이 · 명령 6 · 메뉴 기여
manifest(가상): id "rainbow-brackets" · capabilities [editor.read, editor.overlay, editor.caret, ui.menu, settings]
```
- 나중에 WASM으로 옮길 때: `PluginHost`를 WIT로 내보내고 `rainbow.rs`를 `plugin.wasm`으로 빌드 — 호스트 코드는 그대로.
- 부하원: 편집당 O(n) 스캔 1회(스레드 0 · 상한 있음) · 39 §3 등재.

## 6. 단계

1. 쌍 표 + 오버레이 API(nexa-ctl) + 깊이 색 + 짝 없음 + 현재 쌍 강조 + `rainbow.*` 설정 + 향상 모드.
2. 이동 메뉴 6항목(우클릭 서브메뉴 · 편집 메뉴 · 팔레트 · 키맵) + 테스트(형제/상위/하위).
3. 자동 닫기·감싸기·건너뛰기(`editor.auto_close` · 49와 조율).
4. 2순위: 종류 바꾸기/제거/감싸기 · 안내선 · 스코프 흐림 · SQL 키워드 쌍.

## 8. 기술 구조 분석(사용자 09-17 "이런 기능들을 구현하는 데 적합한 기술 구조")

### 8-1. 쌍 데이터를 어떻게 얻을 것인가 — 3안

| 안 | 방식 | 편집당 비용 | 메모리 | 정확도 | 판정 |
|---|---|---|---|---|---|
| **A. 토큰 스캔 + 스택**(전체 재계산) | 강조기 토큰(문자열·주석 경계)을 따라 한 번 훑으며 스택으로 짝짓기 → `PairTable` | O(n) · 1MB SQL ≈ 2~5ms(측정 예정) | 쌍당 16B(10k 쌍 = 160KB) | 문자열/주석 안 제외 · 키워드 쌍은 규칙 표로 | **1차 채택** — 단순 · 결정적 · 오버레이·이동·자동 닫기가 같은 표를 씀 |
| B. 증분 괄호 트리(VS Code 방식) | 괄호만 담은 **길이 기반 균형 트리**((2,3)-tree) · 편집 범위만 부분 재파싱 O(log n + 변경 길이) · [VS Code "10,000x faster" 설계] | O(log n) | 노드당 ~40B | A와 같음 | 2차(파일 > 5~10MB에서만 이득 · 구현 복잡도 높음) |
| C. 트리시터(전체 구문 트리) | 언어 문법으로 AST · 괄호는 노드 | 증분 파서 · 큰 런타임 | 큼 | 최고(문법 인식) | 보류 — DR-3(외부 crate 최소) · SQL 방언 문법 관리 비용 |

- 근거: 우리 문서는 SQL 스크립트(대개 < 1MB) · 편집 1회당 몇 ms 스캔은 프레임(16~33ms) 안 · 상한(`rainbow.max_kb`) 넘으면 끄거나 백그라운드로. 강조기가 이미 토큰을 만들므로 스캔은 토큰 경계만 존중하면 된다(문자열 안 `(`를 세지 않는 정확도).
- **캐시 키** = 문서 버전(`undo_len` + 길이 + FNV 해시) · 캐럿 이동은 재계산 없음(현재 쌍 판정은 표에서 이진 탐색).
- **스레드**: 스캔이 임계(예 8ms)를 넘는 문서는 텍스트 보기 변환과 같은 패턴(세대 번호 · 백그라운드 스레드 · 도착하면 오버레이 교체)으로 옮긴다 — 첫 구현은 UI 스레드(상한 안).

### 8-2. 표시 — "데코레이션 층" 하나

```
TextBox 페인트 = 구문 토큰 색(강조기) ─▶ 데코레이션(정렬 배열 · 보이는 행만 이진 탐색)
   Decoration { range: (usize, usize), style: Fg(Color) | Underline(Color, px) | Guide { col, color } | Dim }
```
- 지금은 구문 색 · 선택 · 동일 출현 상자 · 찾기 마크 · 공백 마크가 각각 그려진다. **데코레이션 배열 하나**를 추가하면 레인보우 색 · 짝 없음 물결 · 현재 쌍 밑줄 · 안내선 · 스코프 흐림이 전부 같은 경로로 들어온다(부품 규칙 30 §2 · 종류마다 그리기 코드를 늘리지 않음). 플러그인은 "범위+스타일" 배열만 넘긴다(콜백 0 · WASM으로 옮겨도 배열 전달만).
- 우선순위: 데코레이션 `Fg`가 구문 색을 덮고 · 선택 반전이 그 위 · 캐럿 최상위. 보이는 행 밖은 안 그린다(비용 O(보이는 글자)).

### 8-3. 편집 핵심(자동 닫기·감싸기·건너뛰기)은 EditState에

- 플러그인이 아니라 nexa-ctl 편집 코어(`EditState`)의 규칙: `(`를 치면 `()` 넣고 캐럿 사이 · 선택이 있으면 `(선택)` · 닫힘을 치는데 다음 글자가 그 닫힘이면 건너뜀 · Backspace로 빈 쌍 `()` 한 번에 삭제 · 인용부호는 앞 글자가 영숫자면 자동 닫기 안 함(Sublime 규칙). 쌍 집합은 레인보우와 **같은 설정**(`rainbow.brackets` · `rainbow.quotes`)을 읽는다. 49 auto indent와 조합: `(`+Enter = indentOutdent ✅.

### 8-4. 이동·선택 명령은 쌍 표 위의 순수 함수

`PairTable::{pair_at, enclosing, parent, first_child, prev_sibling, next_sibling}` → 캐럿/선택만 바꾼다. 테스트가 쉽고(문자열 → 표 → 명령 → 위치) UI 무관 · 팔레트/키맵/메뉴/WASM 명령이 같은 함수를 부른다.

### 8-5. SQL 키워드 쌍(2순위)의 구조

규칙 표(방언별 · 확장점 = 레지스트리): `("BEGIN","END")` · `("CASE","END")` · `("IF","END IF")` · `("LOOP","END LOOP")` · `("DECLARE","BEGIN")`(선택). 스캔은 강조기의 **키워드 토큰**을 보며 같은 스택에 넣는다(대소문자 무시 · `END IF`처럼 두 토큰 닫힘은 미리 결합). 괄호와 섞여도 스택 하나로 처리(불일치 = unmatched).

### 8-6. 플러그인 API 표면(50 §4의 ②와 동일한 모양)

| API | in-process(1차) | WASM(나중) |
|---|---|---|
| `editor.text_version()` / `text()` / `syntax_tokens(range)` | 참조 · 슬라이스 | 문자열 복사(버전으로 최소화) |
| `editor.set_decorations(Vec<Decoration>)` | 이동 | 배열 직렬화 |
| `editor.caret()/set_caret/select` | 직접 | 호출 |
| `ui.add_menu(path, items)` / `ui.status` | 직접 | 호출 |
| `settings.get(prefix)` | 직접 | 호출 |
| 이벤트 `on_edit(version)` · `on_caret` · `on_settings` | 트레이트 메서드 | export 함수 |

결론: **A(토큰 스캔) + 데코레이션 층 + 순수 함수 명령 + EditState 자동 닫기**가 이 기능군에 맞는 구조다. B(증분 트리)는 상한을 넘는 파일이 실제로 나올 때 같은 `PairTable` 인터페이스 뒤에서 교체한다.

## 9. "in-process 플러그인"이란 · 언제 어느 방식인가(사용자 09-17 D-91 보류 질문)

**정의**: 앱 실행 파일 안에 컴파일된 Rust 모듈이지만, 편집기·그리드의 내부 구조를 직접 만지지 않고 **플러그인 호스트 API(`PluginHost` 트레이트)만** 통해 동작하는 코드. 등록/해제 · 설정 · 메뉴 기여 · 이벤트 수신 경로가 외부(WASM) 플러그인과 **완전히 같다** — 다른 점은 "어디서 실행되는가"(같은 프로세스 · 샌드박스 없음 · 사용자가 설치/삭제 못 함)뿐.

| 기준 | 내장 기능(코어) | in-process 플러그인 | WASM 플러그인 | 프로세스 플러그인 |
|---|---|---|---|---|
| 코드 위치 | nexa-ctl/nexa-sql 내부 | nexa-sql `plugins/*.rs` · 호스트 API만 사용 | `plugin.wasm`(GitHub 설치) | 외부 실행 파일 |
| 호출 비용 | 0 | 함수 호출(0) | 인터프리터 5~20× · 메모리 복사 | IPC 0.1~1ms |
| 샌드박스·권한 | 없음(신뢰) | 없음(신뢰) | 능력 기반 · 상한 | OS 샌드박스 |
| 배포·갱신 | 앱 릴리스 | 앱 릴리스 | 독립(매니저) | 독립 |
| 누가 만드나 | 우리 | 우리 | 누구나 | 누구나 |
| 언제 | 편집 핵심 경험(자동 닫기 · 들여쓰기 · 선택) · 매 키 입력 경로 | 우리가 만들되 **플러그인 API를 검증**하고 싶은 기능 · 켜고 끌 수 있어야 하는 것 · 나중에 외부화 가능성 | 서드파티 · 신뢰 못 하는 코드 · 릴리스와 독립된 주기 | 무거운 언어/라이브러리(Python 등) |

**레인보우 괄호에 적용**: 두 층으로 나뉜다.
- **편집 코어(내장)**: 쌍 표(`PairTable`)·자동 닫기/감싸기/건너뛰기·짝 이동/확장 — 매 키 입력에 닿고 다른 기능(auto indent · 찾기)도 같은 표를 쓰므로 nexa-ctl 내장이 맞다.
- **플러그인 층(in-process)**: 깊이 색 · 짝 없음 · 현재 쌍 강조 · 이동 서브메뉴/명령 · 설정 — 호스트 API(`syntax_tokens` · `set_decorations` · `add_menu` · `on_edit`)만으로 만든다. 이 층이 나중에 WASM으로 옮길 수 있는 부분이고, 첫 플러그인으로서 API를 검증한다.
- 즉 "① in-process"의 정확한 뜻 = **코어에 표·편집 규칙을 두고, 표시·명령·메뉴를 플러그인 API 위에** 올리는 구성.

## 10. WASM 적합성 · 애드온 가능 수준(사용자 09-17 "최종적으로 WASM이 적합한지 · 어느 수준까지 가능한지")

| 수준 | 할 수 있는 것(호스트 API로) | 예 | 판정 |
|---|---|---|---|
| **1 · 텍스트·데이터 처리** | 편집기 텍스트 읽기/치환 · 선택/캐럿 · 구문 토큰 조회 · **데코레이션 배열**(색·밑줄·안내선) · 명령/메뉴/팔레트 등록 · 상태줄 문구 · 설정 읽기 · 이벤트 훅(편집·캐럿·저장 전/후·실행 전/후) · 결과 후처리(페이지 단위 행 읽기 · 값 변환) · 인텔 후보 제공(문맥 문자열 → 후보 배열) · 스니펫/문법/테마 데이터 | 레인보우 괄호 · 포매터 · 대소문자/정렬 변환 · 결과 마스킹 · 사용자 정의 완성 | ✅ 충분(Sublime Python 플러그인이 하는 일의 ~80%) |
| **2 · 제한된 외부 자원** | HTTP 요청(호스트 프록시 · 능력 `net:host` 허용 목록 · 비동기 콜백) · 파일(플러그인 폴더/승인 경로만) · 타이머(호스트 틱) · 클립보드(능력) | 사전/API 연동 · 리포트 저장 · 주기 작업 | ✅ 능력 승인 필요 · 3-OS 동일 |
| **3 · UI** | 선언적 폼/패널(JSON 스펙 → 호스트가 컨트롤 생성) · 알림/프롬프트/퀵패널 | 설정 대화상자 · 선택 목록 | △ 선언형까지만(캔버스 직접 그리기는 API 규모가 커 보류) |
| **4 · 불가·부적합** | 매 프레임 페인트 콜백 · 스레드 생성(wasmi 미지원 → 호스트 작업 큐) · 네이티브 DB 드라이버(22의 cdylib 경로) · OS 프로세스 실행(능력으로만) · 대량 결과 전체 복사(100k행 직렬화 → 페이지/스트림 API로 우회) | 커스텀 렌더러 · 드라이버 | ✗ 다른 경로 |

- **성능 현실**: `wasmi`는 인터프리터라 CPU 집약 작업이 네이티브의 5~20배 느리다. 레인보우 스캔(1MB · 2~5ms 네이티브)은 WASM에서 20~100ms + 텍스트 복사 → 편집마다 돌리기엔 부담. 그래서 §9처럼 **스캔은 코어, 색은 플러그인**으로 나누면 WASM 플러그인도 배열만 만들어 넘기므로 가볍다. JIT(`wasmtime`)로 바꾸면 2~3배 차이로 줄지만 바이너리 +10MB.
- **언어**: Rust(1급) · C/C++ · AssemblyScript · Zig · TinyGo. Python→WASM(CPython-wasm)은 20MB+·느려서 비현실적 → Python은 프로세스 플러그인(50 §3).
- **결론**: 편집 확장 · 명령 · 데코레이션 · 결과 후처리 · 인텔 데이터·후보까지는 WASM이 적합하고, 핵심 편집 규칙(자동 닫기 등)과 프레임 단위 그리기·드라이버는 코어/cdylib에 남긴다. 첫 플러그인은 **§9 구성(in-process · 코어+API 층)**으로 만들고, T-118 뒤 색/메뉴 층을 그대로 WASM 샘플로 옮겨 API를 검증한다.

## 7. 의사결정(사용자 답 대기)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| D-91 | 구현 형태 | ① in-process 플러그인(호스트 API 먼저 · 나중에 WASM으로) ② T-118 WASM 런타임 뒤에 ③ 편집기 내장 기능(플러그인 아님) | **①** |
| D-92 ✅ | 쌍 집합 | ① `()[]{}` + 인용부호 `" ' \`` · `<>` 제외(기본) ② `<>` 포함 ③ 인용부호 제외 | **①** 확정(09-17) |
| D-93 ✅ | 이동 키 | ① Ctrl+Alt+, / . (형제) · Ctrl+Alt+[ / ] (상위/하위) ② Alt+↑/↓ 계열(문장 이동과 충돌) ③ 키 없이 메뉴만 | **①** 확정(09-17) |
| D-94 ✅ | 1순위 범위 | ① 색+짝 없음+현재 쌍+이동 메뉴 ② ①+자동 닫기/감싸기 ③ ②+안내선/스코프 흐림/키워드 쌍 | **②** |
| D-95 | 팔레트 | ① 테마 기본 6색(라이트/다크 각각) ② 사용자 색 목록 필수 ③ 종류별 독립 색 풀 | **①**(②·③은 설정 옵션) |

## 11. 진행 상태(09-17 Windows → Mac 이관 · T-119)

**결정 확정**: D-91 ① 코어 + in-process 플러그인 층 · D-92 ① · D-93 ① · D-94 ② · D-95 ① (모두 09-17).

**✅ 된 것(nexa-ui · 코어 층)**
- `controls/pairs.rs` — `PairTable::build(text, hl, opts)`(구문 토큰 기반 · 문자열/주석 안 제외 · 인용부호는 `Str` 스팬으로 쌍 · plain 모드 인용부호 추적 · 짝 없음 · 깊이/부모) · 조회 `pair_at`/`enclosing`/`parent`/`first_child`/`sibling`/`marks_in` · 테스트.
- `TextBox`: `BracketOpts{rainbow, pairs(quotes/angle), unmatched, match_mode 0/1/2, colors, auto_close, max_chars}` · `set_bracket_opts` · `set_menu_extras`(우클릭 메뉴 추가 항목 → `EditCtxAction::Custom(id)`) · `goto_bracket_sibling/parent/child` · `auto_close`(건너뛰기 · 감싸기 · 알파벳 뒤 인용부호 규칙) · `backspace_pair` · 페인트(깊이 색 · 짝 없음 위험색+밑줄 · 현재 쌍 밑줄) · `Theme.rainbow` 6색(다크/라이트) · 테스트 `brackets_navigation_and_auto_close`(245 green).

**✅ 된 것(nexa-sql · 플러그인 층 · 컴파일만 · 배선 전)**
- `plugins/mod.rs` — `Plugin` 트레이트(id · settings_prefix · commands · menus · on_settings → `PluginEffect` · run) · `EditorOps`(TextBox 구현) · `Registry::builtin()`/`owner_of`/`menu_extras`/`on_settings`.
- `plugins/rainbow.rs` — `RainbowPlugin`(명령 6 · 서브메뉴 "괄호 이동" · 설정 → `BracketOpts`) · 테스트.
- 설정 `rainbow.*` 8키(enabled · quotes · angle · unmatched · match off/near/always · colors · auto_close · max_kb HIDDEN) · BOOST `rainbow.enabled=off` · i18n 24 · `EditCtxAction::Custom` 가지 3곳(메인 = `menu_action(id)`).

**☐ 남은 배선(T-119 · Mac에서)**
1. `App`에 `plugins: Registry` 두고 시작·설정 변경(`apply_setting` `rainbow.*`) 때 `on_settings` → `editors.set_bracket_opts`(전 탭 + `make_box`) · boost 뒤에도 재적용.
2. 키맵 `edit.bracket_prev/next/parent/child`(Ctrl+Alt+, / . / [ / ] · D-93) · `menu_action`에서 `plugins.owner_of(id)` → `run(id, editor)` · 팔레트 등록(`cmds.push`).
3. 편집 메뉴 "괄호 이동 ▸" 서브메뉴 + 편집기 우클릭 `set_menu_extras(plugins.menu_extras(display))`(단축키 표시는 `keymap::display`).
4. 색 목록 설정 창 = 색 창(임의 `*_color` 키 모드 재사용 · 쉼표 목록은 T-119 뒤).
5. 실기: 깊이 색 6 순환 · 짝 없음 빨강 · 현재 쌍 밑줄(near/always/off) · Ctrl+Alt+, . [ ] · 우클릭 서브메뉴 · 자동 닫기/감싸기/건너뛰기/Backspace · 향상 모드에서 색 꺼짐 · 2MB 초과 파일에서 표 없음.
6. 문서: 29(편집기) 표에 키 4 · 24 설정 표 · journal 실기 ㊵.
