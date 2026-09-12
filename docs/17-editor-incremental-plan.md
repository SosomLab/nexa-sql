# 17 · 편집기 점증 개발 계획 — Sublime Text 기준 기능 순서 · 성능 설계 원칙

> 사용자(09-12): *"Sublime Text를 참고해 텍스트 편집 기능을 만들 때 점증적으로 개발해야 할 기능들을 정리하고 순서를 정의. GUI는 최소화한 상태에서 기능을 일정 부분 개발하고 다시 GUI를 우선해서 정리."* + 성능 원칙(요청 큐 병합·인덱스·미니맵 정확 이동).
> 현재: 편집기 = `nexa-ctl::TextBox`(단일/다중행 · IME preedit · 1,657 LOC) — **임시**. `nexa-edit`(nexa-ui)가 이를 대체한다([09 §3](09-editor-and-packages.md)).

## 1. 원칙

- **로직 먼저, 화면 나중**: 각 단계는 화면 없이 `cargo test`로 검증되는 순수 크레이트 코드. GUI는 최소(현재 창)를 유지하다가 E5 이후 다시 GUI 우선.
- **Sublime `Default` 패키지의 명령 이름·키맵을 명세로**(`move`, `insert`, `left_delete`, `find_under_expand`, `soft_undo`, `expand_selection`, `split_selection_into_lines`…).
- ★ **최소 처리 원칙**(clip DR-41 계승): 같은 결과를 내는 요청이 몰리면 큐가 아니라 **마지막 것만**(coalesce) 실행하거나 주기 실행. 대상: 재하이라이트 · 레이아웃 · 미니맵 갱신 · 자동완성 조회 · 세션 저장 · 파일 워처 이벤트. 세대(generation) 번호로 낡은 결과는 버린다.
- ★ **한 번에 찾아간다**: 줄 시작 오프셋 인덱스(로프 노드 요약 = 줄 수·바이트 수 누적 → `O(log n)` 이진 탐색), 접기(fold) 구간 트리, 화면 행↔버퍼 위치 맵(DisplayMap). 검색은 토큰/트라이그램 인덱스로 후보를 좁힌 뒤 확인.
- **좌표는 Anchor로**: 편집 후에도 유효한 위치 표기(버퍼 오프셋 + 세대)로 커서·북마크·진단·폴딩 위치를 보관(Zed 방식).

## 2. 단계 (E0 → E9)

| 단계 | 내용 | Sublime 대응 | 검증 |
|---|---|---|---|
| **E0** ✅ | 임시 편집기 = `TextBox` 다중행 · IME · 선택 실행 | — | GUI 최소 창 |
| **E1 Buffer** | Rope(`crop`/`ropey` 중 택1 · D-9) · 줄 인덱스 · UTF-8/UTF-16/**CP949** 감지 · BOM · EOL(LF/CRLF) 보존 · `Transaction{changes}` + `invert` · `History`(선형 undo · **selection 스냅샷 = soft undo**) | `undo/redo/soft_undo` · `reindent` 기반 | 단위 테스트(랜덤 편집 왕복 · 대용량 10MB 삽입 < 10ms) |
| **E2 Selection** | `Selection{anchor, head, xpos}` 정렬 벡터 · 병합 · 커서 이동(문자/단어/줄/문서 · grapheme 경계 · CJK 폭) · 컬럼 선택 | `move`, `move_to`, `select_all`, `select_lines`, `add_cursor_above/below`, `single_selection` | 테스트 |
| **E3 Commands+Keymap** | `Command{name,args}` 레지스트리 · `.sublime-keymap` 파서 · context 평가(`selection_empty`·`num_selections`·`preceding_text`·`setting.*`·`match_all`) · **Default 키맵 데이터** | `Default (OSX/Windows/Linux).sublime-keymap` | 키 시퀀스 → 명령 테스트 |
| **E4 Display** | DisplayMap(Fold→Tab→Wrap) · 줄 높이 고정(고정폭 전제) · **줄 캐시**(가시 범위만 셰이핑) · gutter/줄번호 · 룰러 20/80/120 · 공백 표시(선택 내) · 들여쓰기 가이드 · 스크롤 좌표 ↔ 버퍼 위치 `O(log n)` | `rulers`, `draw_white_space` | 레이아웃 테스트(가짜 DrawCtx) |
| **E5 Multi-cursor 편집** | `Ctrl+D`(find_under_expand) · `Alt+F3` · `Ctrl+Shift+L` · `Ctrl+U` · 줄 연산(swap/dup/join/sort) · 대소문자 · 들여쓰기/내어쓰기 · 주석 토글(`.tmPreferences` `TM_COMMENT_START`) | 동일 명령 이름 | 테스트 |
| ★ **GUI 재정비 ①** | E1~E5를 `nexa-edit::View`(Widget)로 창에 장착 · IME 오버레이 · 클립보드 · 컨텍스트 메뉴 · 상태줄(인코딩·EOL·커서 위치) | — | 3-OS 실기표 |
| **E6 Find** | 증분 검색 · regex · 선택 내 · 대소문자 보존 치환 · 하이라이트 · Goto line(`:`) · **인덱스**(줄·토큰) | `show_panel find/replace`, `goto_line` | 테스트 |
| **E7 Syntax** | `syntect` `.sublime-syntax` · scope 스택 캐시(줄 단위 · 변경 줄부터 재파싱 · 상태 동일하면 중단) · `.sublime-color-scheme` · 괄호 매칭 | `SQL.sublime-syntax` + PL/SQL·T-SQL | 하이라이트 스냅샷 테스트 |
| **E8 Minimap · Scroll** | 미니맵 = 줄 단위 축소 렌더(토큰 색 · 문자 폭 1px) · **클릭 = 정확한 버퍼 줄로**(미니맵 y → 줄 번호 역매핑은 DisplayMap의 누적 높이 이진 탐색 · 드래그 스크롤 동기) · 미니맵 갱신은 coalesce(프레임당 1회) | `minimap` | 좌표 왕복 테스트 |
| **E9 Completion · Snippets · Fold** | `.sublime-completions` · 스키마 인덱스 provider · `.sublime-snippet` 필드/미러 · 폴딩(들여쓰기 + `meta.block`) · Goto Anything `@`/`#` | `auto_complete`, `insert_snippet`, `fold` | 테스트 |
| ★ **GUI 재정비 ②** | 탭·분할(Origami식 키보드) · Command Palette · 설정 화면 · 패키지 로더 | — | 실기 |

## 3. 성능 설계 세부

| 관심사 | 설계 |
|---|---|
| 재하이라이트 | 변경된 줄부터 아래로, 줄 끝 파서 상태가 이전과 같아지면 중단. 요청은 세대 번호로 coalesce · 프레임당 최대 N줄, 나머지는 다음 유휴 시간 |
| 레이아웃/셰이핑 | 가시 범위 ± 1화면만 셰이핑 · 줄 캐시 키 = (줄 내용 해시, 폰트, 폭). 고정폭 전제라 x → 컬럼은 `O(1)`(CJK는 2셀) |
| 미니맵 | 축소 비트맵을 별도 버퍼에 유지 · 변경 줄만 다시 그림 · 클릭/드래그 y → 줄 = 누적 높이 배열 이진 탐색 → `scroll_to(line, center)` |
| 검색 | 소문자 정규화 텍스트 + 줄 오프셋 배열 · 정규식은 줄 단위 실행 · 결과는 Anchor 목록 |
| 세션 저장 | 변경 후 2초 유휴 또는 30초 주기 · 버퍼별 dirty 플래그 · **원자적 쓰기**(temp+rename) · 종료/닫기 시 즉시([18](18-session-and-projects.md)) |
| 파일 워처 | 150ms debounce · 해시 확인 · 자기 저장 무시([15](15-external-file-changes.md)) |
| 대용량 | 10만 줄 스크립트·덤프: 전부 인메모리 로프 · 하이라이트는 가시 범위 우선 · 미니맵은 샘플링(줄 수 > 20k면 n줄당 1픽셀) |
