# 07 · 편집기 조사 — 기능 인벤토리 · Sublime Text 패키지 구조 · Rust 구현 참고

> 작성 2026-09-12 · 웹 조사(에이전트). 사용자 방침: *"기본 편집 기능은 Sublime Text를 최대한 차용, 확장은 Sublime의 기술구조(패키지)를 차용"* · *"편집기 구현이 최고 난이도 중 하나"*.
> 조사 대상: Sublime Text 4(1차) · VS Code · IntelliJ + 추가 검토(Zed · Lapce · Helix · Neovim · Notepad++ · EmEditor · BBEdit · CudaText · Kate · Emacs). 설계 결정은 [09](09-editor-and-packages.md).

## Part A — 기능 인벤토리

### A-1. Sublime Text 4 (1차 레퍼런스)
- 최신 안정판 Build 4200 (2025-05-21), dev 4211 (2026-09-11). 4200에서 멀티커서 성능 대폭 개선("100,000+ cursors"), Python 3.3 host 제거 예정, 3.8→3.13 계획 ([4200](https://www.sublimetext.com/blog/articles/sublime-text-4200), [dev](https://www.sublimetext.com/dev)).
- ST4 핵심 변화: GPU compositing, Tab Multi-Select, 프로젝트 인덱스 기반 auto complete, non-deterministic grammar(`branch`/`fail`)·lazy `embed`·`extends`, Python 3.8 API ([ST4](https://www.sublimetext.com/blog/articles/sublime-text-4)).
- 핵심 편집 기능([docs.sublimetext.io](https://docs.sublimetext.io/guide/usage/editing.html)):
  - **Multiple selections**: `Ctrl+D`(다음 occurrence), `Ctrl+K, Ctrl+D`(skip), `Ctrl+U`(soft undo — 선택 복원), `Alt+F3`(모든 occurrence), `Ctrl+Shift+L`(Split into Lines), `Ctrl+Alt+Up/Down` 컬럼 선택, middle-drag / `Shift+RightDrag` 컬럼 선택, `Ctrl+Click` 커서 추가.
  - **Goto Anything** `Ctrl+P`: 파일 fuzzy, `@`심볼, `#`파일 내, `:`행, 조합(`tp@rf`). fuzzy 알고리즘 역공학 분석 존재([ForrestTheWoods](https://www.forrestthewoods.com/blog/reverse_engineering_sublime_texts_fuzzy_match/)).
  - **Command Palette** `Ctrl+Shift+P`: 모든 command + `.sublime-commands`.
  - **Selection 확장**: `Ctrl+L`(line), `Ctrl+Shift+Space`(scope), `Ctrl+Shift+M`(brackets), `Ctrl+Shift+J`(indentation), `Ctrl+M`(bracket jump).
  - **Line ops**: swap(`Ctrl+Shift+Up/Down`), duplicate(`Ctrl+Shift+D`), join(`Ctrl+J`), sort/permute, case, wrap(`Alt+Q`), reindent, `Ctrl+Shift+V` paste-and-indent, `Ctrl+T` transpose, `Ctrl+/` comment(`.tmPreferences` `TM_COMMENT_START`).
  - **Find**: `Ctrl+F`/`Ctrl+H`/`Ctrl+I` incremental/`Ctrl+Shift+F` find-in-files → Find Results buffer, regex·case·whole word·in-selection·preserve-case.
  - 탭·레이아웃: `Alt+Shift+1..8` columns/rows/grid, Tab Multi-Select([문서](https://www.sublimetext.com/docs/tab_multi-select.html)), Distraction Free, minimap, rulers, `draw_white_space`, indent guides, word wrap, bookmarks, folding(indent + `meta.block`), bracket matching·auto-pair, macros, snippets(tab trigger·fields·mirrors), Vintage, Jump Back/Forward(`Alt+-`), encoding/BOM/EOL 자동 감지, git gutter(3.2+), 드래그-드롭.
- **Large file**: GB 파일을 "여는" 것은 사실이나 버퍼 구조 비공개. 4180에서 메모리 감소. 500MB~2GB 로딩이 수 분~시간 보고([포럼](https://forum.sublimetext.com/t/very-large-files-loading/43673)). mmap 글은 Sublime Merge용([mmap](https://www.sublimetext.com/blog/articles/use-mmap-with-care)) → 인메모리 + 지연 하이라이팅 추정.
- ★ **IME**: Windows 한국어 IME 조합 깨짐([#3626](https://github.com/sublimehq/sublime_text/issues/3626)), 한자 변환 부분 지원([#6333](https://github.com/sublimehq/sublime_text/issues/6333)) — **ST를 그대로 모방하면 안 되는 영역**.

### A-2. VS Code (editor core)
[Basic Editing](https://code.visualstudio.com/docs/editing/codebasics): `Alt+Click`, `Ctrl+Alt+Up/Down`, `Ctrl+D`, `Ctrl+Shift+L`, `Shift+Alt+I`, `Shift+Alt+Drag` 컬럼 선택, `Shift+Alt+Left/Right` shrink/expand(AST), `Alt+L` find-in-selection, Search Editor, 폴딩, auto-detect indentation, line move/copy, sort, transform case, `Alt+Z` wrap, sticky scroll, minimap, breadcrumbs, bracket pair colorization, editor groups, zen. TextMate + LSP semantic tokens. Undo 선형(soft undo 없음). 대용량은 tokenization·확장 비활성.

### A-3. IntelliJ IDEA (editor core)
[Multiple cursors](https://www.jetbrains.com/help/idea/multicursor.html): `Alt+Shift+Click`, `Ctrl` 두 번+화살표, `Alt+Shift+Insert` Column Mode, `Alt+J`/`Ctrl+Alt+Shift+J`, `Ctrl+W`/`Ctrl+Shift+W` extend/shrink(PSI), **caret 상한 1000**. Lexer→PSI + TextMate fallback. Live Templates, sticky lines, folding.

### A-4. 추가 후보 평가

| 에디터 | 포함 가치 | 근거 |
|---|---|---|
| **Zed** | **포함(높음)** | Rust·tree-sitter·멀티커서·Vim/Helix 모드·WASM 확장 — 기술 스택 가장 유사([Vim docs](https://zed.dev/docs/vim)) |
| **Lapce** | 포함(중) | Rust, Floem, xi-rope 계열, WASI 플러그인, 0.4.6(2026-01-21) |
| **Helix** | 포함(중) | `helix-core`의 Selection/Transaction 설계가 소규모·함수형 → <20k LOC 목표에 참고가치 큼([architecture](https://github.com/helix-editor/helix/blob/master/docs/architecture.md)) |
| **Neovim** | 참고만 | 0.12 내장 `vim.pack`, tree-sitter. Lua 플러그인 모델 비교용 |
| **Notepad++** | 참고만 | 8.9.7(Scintilla 5.6.4). **컬럼 모드 + Column Editor(번호 삽입)** 는 SQL 결과 가공에 유용([커뮤니티](https://community.notepad-plus-plus.org/topic/24839/3-things-on-column-edit-and-multi-edit-modes)) |
| **EmEditor** | 참고만 | 16TB/1조 행, Large File Controller(부분 열기), CSV 모드 — 대용량 결과셋 아이디어([EmEditor](https://www.emeditor.com/text-editor-features/large-file-support/)) |
| BBEdit | 제외 | macOS 전용 |
| CudaText | 참고(낮음) | Sublime 영향, Python 플러그인 |
| Kate | 참고(낮음) | KTextEditor 멀티커서, vi 모드 |
| Emacs | 제외 | Lisp 확장 모델 참고용 |

### A-5. 통합 체크리스트 (SQL 클라이언트 편집기 우선순위)
○ 내장 · △ 부분/플러그인 · × 없음. P0 필수 · P1 1차 출시 후 · P2 선택.

| 기능 | ST4 | VS Code | IntelliJ | Zed | 기타 | 우선순위 |
|---|---|---|---|---|---|---|
| 멀티커서 Ctrl+D / Ctrl+Shift+L / Alt+F3 | ○ | ○ | ○(Alt+J) | ○ | Kate○ N++○ | **P0** |
| 컬럼(block) 선택 | ○ | ○ | ○(mode) | ○ | N++ 강력 | **P0** |
| Soft undo (Ctrl+U) | ○ | × | × | × | — | P1 (ST 고유) |
| Goto Anything (`@` `:` `#`) | ○ | ○ | ○ | ○ | — | P1 (`@`=SQL 오브젝트) |
| Command Palette | ○ | ○ | ○ | ○ | ○ | **P0** |
| Find/Replace regex·in-selection·preserve case | ○ | ○ | ○ | ○ | ○ | **P0** |
| Incremental find | ○ | △ | △ | △ | — | P2 |
| Find-in-files + Results buffer | ○ | ○ | ○ | ○ | — | P1 |
| 스니펫(tab trigger, fields, mirrors) | ○ | ○ | ○ | ○ | ○ | **P0** |
| 자동완성 | ○(index) | ○(LSP) | ○(PSI) | ○ | — | **P0** (스키마 인덱스) |
| Bracket match + auto-pair | ○ | ○ | ○ | ○ | ○ | **P0** |
| Auto-indent / reindent | ○ | ○ | ○ | ○ | ○ | **P0** |
| Line ops | ○ | △ | ○ | ○ | ○ | P1 |
| Expand selection (line/bracket/indent/scope) | ○ | ○ | ○ | ○ | — | P1 |
| Split into Lines | ○ | △ | △ | ○ | — | P1 |
| Transpose / paste-and-indent | ○ | △ | △ | ○ | — | P2 |
| Word wrap + rulers | ○ | ○ | ○ | ○ | ○ | **P0** |
| Whitespace / indent guides | ○ | ○ | ○ | ○ | ○ | P1 |
| 폴딩 | ○ | ○ | ○ | ○ | ○ | P1 |
| 북마크 | ○ | △ | ○ | × | N++○ | P2 |
| Minimap | ○ | ○ | △ | ○ | — | P2 |
| Split panes / 탭 그룹 | ○ | ○ | ○ | ○ | ○ | P1 |
| Distraction-free | ○ | ○ | ○ | ○ | — | P2 |
| Undo tree | × | × | × | × | Vim/Emacs○ | P2 |
| 매크로 | ○ | △ | ○ | × | N++○ | P2 |
| Keymap(JSON+context) | ○ | ○(`when`) | ○(XML) | ○ | — | **P0** |
| Vim 모드 | △ | △ | △ | ○ | Kate○ | P2 |
| 인코딩/BOM/EOL 감지 | ○ | ○ | ○ | ○ | ○ | **P0** |
| 대용량 파일 | △ | △ | △ | △ | EmEditor◎ | P1 |
| **IME(CJK 인라인 조합)** | △(이슈 다수) | ○ | ○ | ○ | — | **P0** (한국어 필수) |
| Bidi | △ | ○ | ○ | △ | — | P2 |
| Ligatures | ○ | ○ | ○ | ○ | — | P2 |
| Syntax engine | `.sublime-syntax` | TextMate+semantic | Lexer/PSI | tree-sitter | Scintilla | **P0** |
| Color scheme(scope) | ○ | ○ | ○ | ○ | — | **P0** |
| Git gutter | ○ | ○ | ○ | ○ | — | P2 |
| Drag-drop 텍스트 | ○ | ○ | ○ | ○ | ○ | P1 |
| Jump back/forward | ○ | ○ | ○ | ○ | — | P1 |
| Sticky scroll | × | ○ | ○ | ○ | — | P2 |

## Part B — Sublime Text 패키지/확장 아키텍처

### B-1. 패키지 구조와 로드 순서
- 위치: `Data/Packages/<Name>/`(loose), `Data/Installed Packages/<Name>.sublime-package`(zip), 앱 번들 내 shipped(읽기 전용). zip 내 파일은 같은 이름의 `Packages/<Name>/` 동일 경로 파일로 **override**.
- 로드 순서: `Default` → shipped(알파벳) → Installed(알파벳) → 사용자 패키지(알파벳) → `User` 마지막. 동명 리소스는 나중 것 우선([packages](https://docs.sublimetext.io/guide/extensibility/packages.html)).
- ★ `Default` 패키지 = 모든 기본 keymap·menu·command·Python 명령 구현을 담은 **레퍼런스 구현** → 새 에디터에서 이 패키지의 keymap/commands가 "기본 기능 명세서".
- Package Control 4.x: PEP 440, Python 3.8([package_control](https://github.com/sublimehq/package_control)).

### B-2. 파일 타입과 역할

| 파일 | 형식 | 역할 |
|---|---|---|
| `.sublime-syntax` | YAML | `name`, `file_extensions`, `first_line_match`, `scope`, `version`, `extends`, `hidden`, `variables`(`{{var}}`), `contexts`. 액션: `match`/`scope`/`captures`/`push`/`pop`/`set`/`embed`+`escape`/`branch`+`fail`/`include`/`with_prototype`/`meta_scope`/`meta_content_scope`/`clear_scopes`/`meta_prepend`/`meta_append`. Oniguruma, 한 줄 단위, branch 되감기 128행([syntax](https://www.sublimetext.com/docs/syntax.html)) |
| `.tmPreferences` | XML plist | `scope` + `settings`: `increaseIndentPattern`, `decreaseIndentPattern`, `bracketIndentNextLinePattern`, `cancelCompletion`, `shellVariables`(`TM_COMMENT_START/END`), `showInSymbolList`, `symbolTransformation`([metadata](https://docs.sublimetext.io/reference/metadata.html)) |
| `.sublime-completions` | JSON | `scope` + `completions[]{trigger, contents, annotation, kind, details}`([completions](https://www.sublimetext.com/docs/completions.html)) |
| `.sublime-snippet` | XML | `<content>`, `<tabTrigger>`, `<scope>`; `$1`, `${1:placeholder}`, mirror, `$0`, `${1/regex/repl/}`([snippets](https://docs.sublimetext.io/guide/extensibility/snippets.html)) |
| `.sublime-keymap` | JSON | `keys[]`(chord), `command`, `args`, `context[]{key, operator, operand, match_all}`([key bindings](https://www.sublimetext.com/docs/key_bindings.html)) |
| `.sublime-commands` | JSON | Command Palette `{caption, command, args}` |
| `.sublime-menu` | JSON | Main/Context/Side Bar/Tab 메뉴 트리 |
| `.sublime-settings` | JSON | 계층: Default → 플랫폼 → 패키지 → User → project → syntax-specific(`SQL.sublime-settings`) → session([settings](https://www.sublimetext.com/docs/settings.html)) |
| `.sublime-color-scheme` | JSON | `variables`, `globals`, `rules[]{scope, foreground, background, font_style}`([color schemes](https://www.sublimetext.com/docs/color_schemes.html)) |
| `.sublime-theme` | JSON | UI 스타일 |
| `.sublime-build` | JSON | `cmd`, `selector`, `variants` |
| `.sublime-project` | JSON | `folders`, `settings`, `build_systems` |

### B-3. Python 플러그인 API
- 모듈 `sublime`, `sublime_plugin`. 명령: `ApplicationCommand`, `WindowCommand(window)`, `TextCommand(view, edit)`; 리스너: `EventListener`(`on_new/on_load/on_modified/on_selection_modified/on_query_completions/on_query_context/on_hover/on_text_command/on_post_text_command/on_pre_save/on_post_save/on_close` + `_async`), `ViewEventListener`(`is_applicable(settings)`), `TextChangeListener`; 입력 `TextInputHandler`/`ListInputHandler`([API](https://www.sublimetext.com/docs/api_reference.html)).
- 핵심 객체: `View`, `Window`, `Sheet`, `Edit`(TextCommand.run 동안만 유효 → 하나의 undo 단위), `Region(a,b,xpos)`, `Selection`(정렬·비중첩), `Phantom/PhantomSet`, `add_regions(key, regions, scope, icon, flags, annotations)`, `CompletionItem/CompletionList`, `set_timeout(_async)`, `score_selector`.
- ★ **명령 이름 규칙**: 클래스명에서 `Command` 제거 후 snake_case(`InsertSnippetCommand` → `insert_snippet`). 키맵·메뉴·팔레트·`view.run_command()`가 모두 이 문자열 + JSON args로 연결 → **텍스트 파일만으로 UI를 조립**.
- ★ **Context가 동작을 결정**: 키 입력 시 keymap을 역순(User 우선)으로 스캔, `context` 전부 참인 첫 바인딩 실행. 키: `selector`, `text`, `preceding_text`, `following_text`, `selection_empty`, `num_selections`, `has_next_field`, `auto_complete_visible`, `panel`, `overlay_visible`, `last_command`, `setting.*`, `eol_selector`; 연산자 `equal/not_equal/regex_match/regex_contains/...`; `match_all`. 플러그인은 `on_query_context`로 키 추가. Tab이 snippet 이동/자동완성 확정/들여쓰기 중 무엇인지가 전부 이 context로 분기.
- ★ **Scope selector가 모든 것을 묶는다**: syntax가 `source.sql keyword.other.DML.sql` scope stack 부여 → color scheme `rules.scope`, snippet/completions `scope`, keymap `selector`, `.tmPreferences` `scope`, `SQL.sublime-settings`가 같은 selector 언어(`source.sql - string - comment`)로 매칭. `score_selector`로 특이도.
- 프로세스 모델: 플러그인은 별도 `plugin_host-3.3/3.8` 프로세스 → 크래시 격리([API environments](https://www.sublimetext.com/docs/api_environments.html)).

### B-4. 관련 Rust 크레이트

| 크레이트 | 상태 (2026-09) | 비고 |
|---|---|---|
| `syntect` | 5.3.0 (2025-09-27), MIT | `.sublime-syntax` + `.tmTheme`. **branch/fail은 PR #614로 2026-03-27 master 병합**(`parse_line` breaking), 5.3.0 릴리스 미포함([PR #614](https://github.com/trishume/syntect/pull/614)). `.sublime-color-scheme` JSON 미지원 → 자체 파서. `fancy-regex` 백엔드는 onig 대비 약 절반 속도 |
| `tree-sitter` + SQL | DerekStride/tree-sitter-sql 활발([GitHub](https://github.com/derekstride/tree-sitter-sql)); crates.io `tree-sitter-sql` 0.0.2는 낡음 |
| `ropey` | 1.6.1 안정, 2.0.0-beta.1 | char 인덱스, Helix 채택 |
| `crop` | 활발 | byte 인덱스, LF/CRLF만, ropey 대비 3~4배([crop](https://github.com/noib3/crop)) |
| `xi-rope` | 종료 | Levien 회고: CRDT·멀티프로세스 과설계([retrospective](https://raphlinus.github.io/xi/2020/06/27/xi-retrospective.html)) |
| `cosmic-text` | 0.19.0 (2026-07-10), MIT/Apache | HarfRust shaping, swash 래스터, bidi, 폰트 폴백. **preedit API 없음**(확인 필요)([docs](https://docs.rs/cosmic-text/latest/cosmic_text/trait.Edit.html)) |
| `unicode-segmentation`, `unicode-width` | 안정 | grapheme 커서, CJK 전각 폭 |
| `regex` / `fancy-regex` / `onig` | 안정 | Sublime 문법은 lookbehind·backreference → `fancy-regex`(순수 Rust) 또는 `onig`(C) |
| `wasmtime` / `wasmi` / `extism` | Extism은 wasmtime 위([lib.rs](https://lib.rs/crates/extism)) | wasmtime JIT·바이너리 큼, wasmi 인터프리터 경량 |
| `mlua` | 안정 | Lua 5.4/LuaJIT/Luau, 화이트리스트 샌드박스 |
| `pyo3` / `rustpython` | pyo3 안정, RustPython "not production-ready" | pyo3는 CPython 동반 배포 |

### B-5. Zed 확장 API와 런타임 비교
- Zed: `zed_extension_api` 0.7.0 (2026-07-24), `wasm32-wasip2`, `extension.toml`, WIT. 제공: languages(tree-sitter .wasm + queries), language servers, themes, snippets, slash commands, context servers(MCP), debug adapters. 호스트 함수 `http_client`, `process`, `settings`, `download_file`([docs.rs](https://docs.rs/zed_extension_api/latest/zed_extension_api/), [blog](https://zed.dev/blog/zed-decoded-extensions)). ★ **편집 명령·UI·키맵을 확장이 추가하는 경로는 없음** — Sublime 모델과의 결정적 차이.

| 기준 | WASM (Zed/Lapce) | Lua (Neovim) | Python (Sublime) |
|---|---|---|---|
| 샌드박스 | 강함 | 화이트리스트로 가능 | 없음(프로세스 격리만) |
| 시작 시간 | 컴파일 캐시 필요 | 수 ms | 수백 ms |
| 배포 | 단일 `.wasm` | 런타임 내장(~300KB) | CPython 동반 |
| 작성자 접근성 | 높은 장벽 | 매우 낮음 | 낮음, ST 플러그인 이식 가능 |
| 에디터 상태 접근 | WIT 노출분만 | 자유 | 전체 |
| 사유 앱 적합성 | 서명·검증 용이 | 좋음 | 크기 부담 |

에이전트 권고: 1차 **데이터 패키지(문법·스니펫·완성·키맵·컬러스킴)만으로 확장**(런타임 없음) → 2차 `mlua`(Luau)로 Sublime API 형태 노출 → WASM은 3rd-party 배포 시. (우리 결정은 [09 §4](09-editor-and-packages.md) — dir2에 이미 `wasmi` 플러그인 선례가 있어 재검토.)

## Part C — 권고 및 단계별 개발 계획

### C-1. 참고할 오픈소스 구현
- **Helix `helix-core`** (MPL-2.0): `Rope`(ropey) + `Selection{Range{anchor, head}}` + `Transaction/ChangeSet`(OT식, invert로 undo, `Selection::map`) — 소규모·순수함수형. `helix-view`의 `Document`/`View` 분리 차용 가능.
- **Zed `editor`** (GPL/AGPL — 코드 복사 불가, 설계만): `DisplayMap` = `InlayMap → FoldMap → TabMap → WrapMap → BlockMap` 파이프라인, 좌표를 `Anchor`로 보관. `SumTree` Rope([Rope & SumTree](https://zed.dev/blog/zed-decoded-rope-sumtree)).
- **Lapce** (Apache-2.0): floem 위 편집기, WASI 플러그인.
- **cosmic-edit / cosmic-text** (MIT/Apache): CPU 래스터(`swash`)로 텍스트를 그리는 실전 예시 — ★ 우리 래스터라이저와 가장 가까운 파이프라인.
- **xi-editor**: CRDT는 채택하지 말 것. "Rope Science"의 라인 캐시·증분 하이라이팅만 참고.

### C-2. 단계별 계획 (제안)
1. **Buffer (1~2주)**: `crop`(byte 인덱스, CRLF) 또는 `ropey 2.0-beta`. 인코딩/BOM/EOL 감지, 인메모리 + 지연 하이라이팅. Helix식 `Transaction` + `History`(선형 undo, ST soft undo용 selection 스냅샷).
2. **Layout/Shaping (2주)**: `cosmic-text`로 shaping·bidi·폴백; 행 단위 `LayoutLine` 캐시. CJK 폭·grapheme은 `unicode-width`/`unicode-segmentation`. 렌더는 `swash` 글리프 → 자체 래스터라이저 blit.
3. **View/Rendering (2주)**: DisplayMap 축소판(`Fold → Tab → Wrap`), gutter, 룰러, 공백, indent guide. buffer offset ↔ display row 분리.
4. **Selections/Multi-cursor (2주)**: `Selection{anchor, head, xpos}` 정렬 벡터, 병합, 컬럼 선택 가상 위치. ST 명령 이름 그대로: `find_under_expand`, `split_selection_into_lines`, `select_all_under`, `soft_undo`, `expand_selection{to}`.
5. **Commands/Keymap (1~2주)**: `Command{name, args}` 레지스트리 + `.sublime-keymap` 파서 + context 평가기. Command Palette·`.sublime-commands`·설정 계층.
6. **Syntax (2~3주)**: 1안 `syntect`(master, branch)로 `.sublime-syntax` + 자체 `.sublime-color-scheme`/`.tmPreferences` 파서. 2안 tree-sitter. **권고 1안**(ST 패키지 생태계 재사용). tree-sitter는 스키마 인식 자동완성용으로 P1 병행.
7. **Completion/Snippets (2주)**: `.sublime-snippet` + `.sublime-completions` + 스키마 인덱스 provider.
8. **Packages (2주)**: `Packages/` + zip 로더, override, 로드 순서, `Default` 패키지 배포.

### C-3. 제약별 주의점
- ★ **Korean IME 필수**: OS 조합 이벤트(Windows `WM_IME_COMPOSITION`, macOS `NSTextInputClient`, Wayland `zwp_text_input_v3`)를 받아 preedit 구간을 버퍼에 임시 삽입하지 않고 **뷰 오버레이**로 그리고, 커서 사각형을 IME에 보고. ST Windows 이슈(#3626)는 이 부분 실패 사례. cosmic-text에 preedit API가 없으므로 layout 단계에 preedit 스타일 주입 지점 별도 설계.
- **CPU 래스터라이저**: 글리프 아틀라스 캐시(폰트·크기·서브픽셀 키) 필수. 행 단위 dirty-rect.
- **<20k LOC**: Helix 구조를 뼈대로. CRDT·멀티프로세스·LSP 범위 밖.
- **라이선스**: Helix MPL-2.0(파일 단위 카피레프트), Zed GPL/AGPL(복사 금지), Lapce Apache-2.0, syntect MIT, cosmic-text MIT/Apache — 사유 앱은 MIT/Apache 계열만 코드 차용.
