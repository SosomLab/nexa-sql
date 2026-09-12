# 09 · 편집기 · 패키지 확장 설계 (`nexa-edit`) — 최고 난이도 구간

> 사용자 방침(09-12): ① 편집기 구현이 최고 난이도 중 하나 ② **기본 편집 기능은 Sublime Text를 최대한 차용** ③ **확장은 Sublime Text의 기술구조(패키지)를 차용**해 package를 개발하면 기능이 추가되게 ④ VS Code·Sublime·IntelliJ + 추가 앱의 기능 목록 정리와 개발 방향 검토.
> 조사 결과 = [07](07-editor-research.md). 이 문서는 **결정과 구조**만 적는다.

## 1. 개발 방향 결정

| 항목 | 결정 | 왜 |
|---|---|---|
| 편집 모델·키맵 | **Sublime Text 4의 `Default` 패키지를 명세서로** 채택 — 명령 이름(`find_under_expand` · `split_selection_into_lines` · `soft_undo` · `expand_selection{to}`)과 키 바인딩 JSON을 그대로 | ST 사용자의 근육 기억을 사고 나머지 앱(VS Code·IntelliJ)의 키맵은 **패키지로 제공**(같은 명령에 다른 키) |
| 프레임워크 | 새 프레임워크 없음 — **`nexa-ui`(자체 래스터) 위에 `nexa-edit` 크레이트** | [06 Part B](06-rust-ecosystem.md) 결론: egui가 얻는 점수는 nexa-ui도 얻고, 자체 에디터 구현 비용은 어느 프레임워크든 동일 |
| 구조 참고 | **Helix `helix-core`**(Rope + Selection{anchor,head} + Transaction/ChangeSet) 를 뼈대로, Zed `DisplayMap`(Fold→Tab→Wrap) 개념 차용. **코드 복사는 MIT/Apache만**(Helix MPL·Zed GPL은 설계만) | <20k LOC 목표 |
| 구문 강조 | **`.sublime-syntax`를 그대로 읽는다** — `syntect`(MIT · branch 지원은 master) 채택. `.sublime-color-scheme`(JSON)·`.tmPreferences` 파서는 자체 | ST 패키지 생태계(SQL·PL/SQL·T-SQL 문법)를 재사용 |
| 텍스트 셰이핑 | 크레이트 도입 대상(**D-3**) — 후보 `rustybuzz`(MIT · HarfBuzz 포팅) 단독 vs `cosmic-text`(shaping+폴백+bidi 일괄). 현 `nexa-gfx`는 글리프 단위라 **한글 조합·CJK 폭·합자**가 안 된다 | 편집기·그리드 모두 요구 |
| **IME** | P0. preedit는 버퍼에 넣지 않고 **뷰 오버레이**로 그리며 커서 사각형을 OS에 보고(Windows `WM_IME_COMPOSITION` · macOS `NSTextInputClient` · Wayland `text_input_v3`). ST의 Windows 한글 이슈(#3626)를 반면교사 | 한국어 필수 |
| 대용량 | 인메모리 + 지연 하이라이팅. EmEditor식 부분 열기는 P1 후반 | SQL 덤프·결과 |

## 2. 기능 범위 (P0 → P2) — [07 A-5](07-editor-research.md) 체크리스트 요약

- **P0**: 멀티커서(`Ctrl+D`·`Ctrl+Shift+L`·`Alt+F3`·컬럼 선택) · Command Palette · Find/Replace(regex·선택 내·대소문자 보존) · 스니펫(필드·미러) · 자동완성(스키마 인덱스) · 괄호 매칭/자동 쌍 · 자동 들여쓰기 · 워드랩·룰러 · 키맵(JSON+context) · 인코딩/BOM/EOL · **IME** · `.sublime-syntax` 엔진 · scope 기반 컬러스킴.
- **P1**: soft undo · Goto Anything(`@`=DB 오브젝트 · `:`줄) · Find-in-files 결과 버퍼 · 줄 연산(swap/dup/join/sort/case) · expand selection(line/bracket/indent/scope) · 공백 표시·들여쓰기 가이드 · 폴딩 · split panes/탭 그룹 · 드래그드롭 · Jump back/forward · 대용량.
- **P2**: incremental find · 북마크 · minimap · distraction-free · undo tree · 매크로 · Vim 모드(패키지) · bidi · 합자 · git gutter · sticky scroll.
- ★ SQL 전용: 커서 문장 실행(`Ctrl+Enter`) · 선택 실행 · 스크립트 실행(`F5`) · 바인드 변수 패널(PL/SQL Developer Test Window식 — 소스에서 `:v` 스캔) · `/*csv*/` 인라인 포맷 힌트(SQLcl) · Notepad++식 컬럼 편집기(결과 가공).

## 3. 크레이트 구조 (`nexa-ui/crates/nexa-edit` — 공용, nexa-sql이 첫 소비자)

```text
nexa-edit
├ buffer/      Rope(crop 또는 ropey) · 인코딩/EOL 감지 · Transaction/ChangeSet · History(선형 + selection 스냅샷 = soft undo)
├ selection/   Selection{anchor, head, xpos} 정렬 벡터 · 병합 · 컬럼 선택(가상 위치)
├ display/     Fold → Tab → Wrap 맵 · gutter · 룰러 · 공백/가이드 · LayoutLine 캐시(가시 범위)
├ syntax/      syntect 어댑터 · .sublime-color-scheme JSON · .tmPreferences · scope selector 점수
├ commands/    Command{name, args} 레지스트리 · .sublime-keymap 파서 · context 평가기(selector · preceding_text · has_next_field · setting.*)
├ complete/    CompletionProvider 트레이트 · .sublime-completions · .sublime-snippet(필드·미러·변환)
├ ime/         preedit 오버레이 · 커서 사각형 보고
└ view.rs      nexa-ctl::Widget 구현 — DrawCtx로 그린다(래스터 호출 0)
```

수직 슬라이스 순서([07 C-2](07-editor-research.md)): Buffer → Layout/Shaping → View → Selections → Commands/Keymap → Syntax → Completion/Snippets → Packages. 각 단계는 `cargo test`로 검증 가능한 순수 로직 우선, 화면은 마지막.

## 4. 패키지 시스템 — Sublime 구조 차용

### 4-1. 데이터 패키지 (1차 · 런타임 없음)
`Packages/<Name>/` 폴더 또는 `<Name>.nexa-package`(zip). 로드 순서 `Default → 동봉 → 설치 → 사용자 → User`(나중 것 우선, 동일 경로 override). **파일 형식은 Sublime 것을 그대로 채택**해 기존 ST 패키지를 거의 복사해 쓸 수 있게 한다:

| 파일 | 역할 | 파서 |
|---|---|---|
| `.sublime-syntax` | 문법 | syntect |
| `.sublime-color-scheme` | 색 | 자체(JSON) |
| `.tmPreferences` | 들여쓰기·주석·심볼 | 자체(plist) |
| `.sublime-snippet` · `.sublime-completions` | 스니펫·완성 | 자체 |
| `.sublime-keymap` · `.sublime-commands` · `.sublime-menu` | 키·팔레트·메뉴 | 자체(JSON + context) |
| `.sublime-settings` | 계층 설정(Default→User→문법별) | `nexa-conf` 확장 |
| ★ `nexa-dialect.json`(신설) | DBMS 방언 정의 — 키워드·바인드 문법·배치 구분·시스템 카탈로그 질의(오브젝트 브라우저)·DDL 템플릿 | 자체 |
| ★ `nexa-driver.json`(신설) | 드라이버 플러그인 매니페스트(4-3) | 자체 |

→ `Default` 패키지 = 우리 기본 키맵·명령·SQL/PL/SQL/T-SQL 문법·컬러스킴·방언 정의. **ST가 그렇듯 텍스트 파일만으로 UI를 조립**한다.

### 4-2. 코드 플러그인 (2차) — `TextCommand` / `WindowCommand` / `EventListener`
Sublime의 Python API **모양**(`TextCommand.run(edit, args)` · `EventListener.on_query_completions/on_query_context/on_modified` · `View/Region/Selection`)을 노출하되 런타임은:
- **1순위 WASM(`wasmi`)** — `nexa-dir2`가 이미 `wasmi 1.1` 플러그인을 실증했고(재발명 금지), 샌드박스·서명·단일 `.wasm` 배포가 사유 앱에 맞다. 호스트 API는 WIT 대신 **단순 함수 표(JSON in/out)** 로 시작.
- 2순위 Lua(`mlua` · Luau) — 작성자 접근성이 필요해지면 같은 API를 Lua에도 바인딩.
- Python(pyo3)은 배포 크기 때문에 제외. 플러그인 크래시는 ST처럼 **본체와 격리**(WASM 트랩 = 플러그인 비활성).
- ★ 방언·드라이버도 플러그인의 한 종류다: `nexa-dialect.json`(데이터) + 선택적 `.wasm`(이름 정규화·계획 파서 등 코드).

### 4-3. 드라이버 플러그인
Tabularis의 **stdio JSON-RPC 프로세스 플러그인**([03 §3](03-competitive-landscape.md))을 채택 후보로 둔다 — 언어 무관(Java JDBC 래퍼도 가능), 크래시 격리, 라이선스 격리(GPL 드라이버도 별 프로세스면 본체 오염 없음). 내장 드라이버(순수 Rust)와 같은 `Session` 포트를 구현한다.

## 5. 위험과 대응

| 위험 | 대응 |
|---|---|
| 편집기 코어가 2만 줄을 넘어 늪이 됨 | Helix 구조 고정 · 단계별 테스트 · P0 외 기능은 패키지로 미룸 |
| 한글 IME 3-OS 편차 | 각 OS 실기 점검표(clip `21-manual-test` 방식) · preedit 오버레이 원칙 고수 |
| syntect branch 미릴리스 | git 의존(master 커밋 고정) 또는 branch 없는 문법으로 시작(SQL 문법은 branch 불필요) |
| 셰이핑 크레이트가 외부 의존 0 규율과 충돌 | D-3에서 명시 예외 원장 등재(rustybuzz는 의존 트리 작음) |
