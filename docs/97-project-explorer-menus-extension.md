# 97. 확장 "Project Explorer Menus" — 개발 기록 · 작업 기록 (2026-09-30 시작)

> 사용자 09-30: "열린 파일의 우클릭 메뉴를 확장하는 Extension을 개발하고 싶어 — SDK API로 메뉴를 추가하고 기능을 구현하는 소스의 **기본 틀만** 잡아 줘.
> 이름은 **Project Explorer Menus**. 개발 기록은 **해야 할 일의 순서**로, 작업 기록은 **시간 순서**로 남겨 줘. 본 프로그램에 함수가 만들어지고 →
> 확장에서 그 함수를 호출하고 → wasm으로 빌드하고 → 확장 프로그램에 등록하는 것까지 순서대로."
>
> 관련: [75 확장 SDK · WASM 동적 로딩](75-extension-sdk-and-dynamic-loading.md)(ABI · 매니저 · 버전 규칙) · [50 플러그인 시스템](50-extension-system.md) ·
> [94 §6-4 확장 설정 키 규칙](94-settings-key-naming-and-location.md) · [67 프로젝트 작업 환경](67-project-workspace.md)(OPEN FILES) · 위키 [Extensions](wiki/Home.md).

## 0. 한눈에

| 항목 | 값 |
|---|---|
| 확장 id · 이름 | `project-explorer-menus` · **Project Explorer Menus** |
| 종류 | `wasm`(SDK `nexa-ext-sdk` · ABI v1 + 호스트 상황 조회 `nx_host_get`) |
| 소스 | `extensions/sdk/samples/project-explorer-menus/`(`Cargo.toml` · `src/lib.rs`) |
| 패키지 | `extensions/project-explorer-menus/`(`extension.json` · `project_explorer_menus.wasm`) · `extensions/index.json` 항목 |
| 빌드 | `pwsh scripts/ext-build.ps1 -Only project-explorer-menus`(wasm 재빌드 + sha256) |
| 설치 | 확장 패널 ▸ 설치 가능 ▸ **Project Explorer Menus ▸ 설치**(또는 `scripts/ext-sync-installed.ps1` — 이 PC 설치본 갱신) |
| 지금(0.1.0) 하는 일 | 편집기 우클릭 ▸ **Project Explorer Menus ▸ Show file path** → 활성 문서의 파일 경로를 호스트에서 받아 앱 로그에 남긴다 |
| 설정 키 접두 | `ext.project_explorer_menus.`(아직 키 없음 · 94 §6-4) |

## 1. 개발 기록 — 해야 할 일의 **순서**(한 기능 = 이 여섯 걸음)

새 기능(메뉴 항목 하나)을 넣을 때 늘 이 순서로 간다. 0.1.0의 "Show file path"가 그대로 이 길을 밟았다(각 걸음의 소스 위치는 §2).

1. **호스트 함수를 만든다**(nexa-sql · `crates/nexa-sql/src/extensions/`)
   - 확장이 필요로 하는 **값**(예: 활성 문서 경로)이나 **동작**(예: 파일 열기)을 호스트 포트에 더한다.
   - 값 = `HostCtx`에 필드 + import `nx_host_get(kind, ptr, cap)`의 `kind` 번호 하나(`wasm.rs` `HOST_GET_*`).
   - 동작 = `EditorOps`에 메서드 + `nx_editor_op`의 op 번호(`wasm.rs` `run` 안 번호표 · SDK `op::*`와 같은 표).
   - 호출 때 넘길 상황은 `Extension::run_with(id, ed, doc_path)`(`mod.rs`) ← `App::run_extension_cmd`(`app/extensions.rs`)가 채운다.
2. **SDK에 임포트·래퍼를 만든다**(`extensions/sdk/nexa-ext-sdk/src/lib.rs`)
   - `host` 모듈 `extern "C"`에 import 선언 + wasm/비wasm 두 판의 안전한 래퍼(`host::get` · `host::editor_op`).
   - 번호표 `query::*` / `op::*` 상수 + `Editor` 메서드(`Editor::doc_path()`처럼 **의미 있는 이름**) — 확장 코드는 번호를 모른다.
   - SDK 단위 시험은 호스트 타깃으로: `cargo test -p nexa-ext-sdk --target x86_64-pc-windows-msvc`(작업 공간 기본 타깃이 wasm32).
3. **확장에서 호출한다**(`extensions/sdk/samples/project-explorer-menus/src/lib.rs`)
   - `meta()`: `commands`(id = `ext.project_explorer_menus.<이름>`) + `menus`(편집기 우클릭 서브메뉴 · `items` = 명령 id).
   - `run(cmd, ed)`: 명령 id로 분기 → `ed.doc_path()` 등 SDK 메서드 호출 → `nexa_ext_sdk::log(..)`로 결과.
   - 시험(호스트 밖): 메타 맞물림(메뉴 항목 = 등록 명령 · id 접두) · `run`이 자기 명령만 처리.
4. **wasm으로 빌드한다**
   - `extensions/sdk`에서 `cargo build --release`(기본 타깃 wasm32) — 또는 5의 스크립트가 함께 한다.
5. **패키지에 배치하고 등록한다**
   - `pwsh scripts/ext-build.ps1 -Only project-explorer-menus` → `extensions/project-explorer-menus/project_explorer_menus.wasm` + `extension.json`의 `files[].sha256`.
   - **버전**: wasm이 바뀌는 커밋마다 `version`을 올린다(세 곳 = 샘플 `Cargo.toml` · `extension.json` · `index.json` · [75 §3-1](75-extension-sdk-and-dynamic-loading.md)).
   - 설정 키를 새로 쓰면 호스트 레지스트리(`nsql-settings` `ext.project_explorer_menus.*` + i18n 라벨 + `EXTENSION_CATEGORIES`)에도 등록한다(75 §9 ① 동적 등록 전까지).
6. **설치·확인한다**
   - 앱 확장 패널 ▸ 설치 가능 ▸ 설치(또는 이미 설치했으면 **업데이트** 버튼 · 09-30) · 이 PC 설치본은 `scripts/ext-sync-installed.ps1`.
   - 편집기 우클릭 ▸ 서브메뉴 ▸ 명령 → 로그 창(개발자 모드 `ext` 층)에서 확인. 호스트 wasm 시험 = `cargo test -p nexa-sql --bin nexa-sql extensions::`.

## 2. 소스 지도 — 무엇이 어디에, 무엇과 연결되는가(0.1.0 기준)

### 2-1. 호스트(nexa-sql)

| 파일 | 추가/변경 | 목적 | 연결 |
|---|---|---|---|
| `crates/nexa-sql/src/extensions/mod.rs` | 변경 · trait `Extension::run_with(id, ed, doc_path)`(기본 = `run`) · `Registry::run_with(cmd, disabled, ed, doc_path)` | 명령 실행에 **호스트 상황**(활성 문서 경로)을 함께 넘기는 통로 | `App::run_extension_cmd` → 여기 → WASM `run_with` · 내장 확장은 기본 구현(상황 무시) |
| `crates/nexa-sql/src/extensions/wasm.rs` | 변경 · `HostCtx.doc_path` · `HOST_GET_DOC_PATH = 1` · import **`nx_host_get(kind, ptr, cap) -> i32`**(게스트 버퍼에 길이 접두 + 본문 · 상한 넘거나 모르는 종류 = -1 · 연료 20k) · `WasmExtension.doc_path: RefCell<String>` · `run_with` = 값을 넣고 `run` 뒤 비움 | 호스트 함수의 본체 — 게스트가 부르는 import | SDK `host::get` ↔ 이 import · `call_inner`가 `HostCtx`에 복사 |
| `crates/nexa-sql/src/app/extensions.rs` | 변경 · `run_extension_cmd` = `editors.active_path()` → `run_with(.., &doc)` | 상황값의 **원천**(활성 탭의 파일 경로 · 스크립트 탭 = 빈 글) | `Editors::active_path`(editors.rs) |

### 2-2. SDK(`extensions/sdk/nexa-ext-sdk/src/lib.rs`)

| 항목 | 추가 | 목적 | 연결 |
|---|---|---|---|
| `host` 모듈 `extern "C" fn nx_host_get(kind, ptr, cap) -> i32` | 추가 | import 선언(wasm32만) | 호스트 `linker()`의 같은 이름 |
| `host::get(kind) -> Option<String>` | 추가 | 안전한 래퍼: `buf::alloc(4096)` → import → `buf::read` → `buf::free` · 비wasm = `None` | `Editor::doc_path` |
| `query::DOC_PATH = 1` | 추가 | 종류 번호표(호스트 `HOST_GET_DOC_PATH`와 같은 값) | — |
| `Editor::doc_path(&self) -> Option<String>` | 추가 | 확장이 부르는 **의미 있는 API**(빈 글 = None) | 확장 `run` |

### 2-3. 확장(`extensions/sdk/samples/project-explorer-menus/`)

| 파일 | 내용 | 연결 |
|---|---|---|
| `Cargo.toml` | crate `project-explorer-menus` 0.1.0 · `cdylib` · 의존 `nexa-ext-sdk`만 | 작업 공간 `extensions/sdk/Cargo.toml` members |
| `src/lib.rs` | `CMD_SHOW_PATH = "ext.project_explorer_menus.show_path"` · `meta()`(id·이름·접두·명령 1·메뉴 1) · `on_settings` = 효과 없음 · `run` = 명령이면 `ed.doc_path()` → `log` · 시험 2 | 호스트가 `nx_ext_meta`로 메타를 읽어 팔레트·우클릭 메뉴에 올린다(75 §6) |

### 2-4. 패키지·등록

| 파일 | 내용 |
|---|---|
| `extensions/project-explorer-menus/extension.json` | `format 1` · id·이름·**version 0.1.0** · `kind wasm` · summary/description · `settings_prefix` · `files[]` = `project_explorer_menus.wasm` + sha256 · `messages.install` |
| `extensions/project-explorer-menus/project_explorer_menus.wasm` | 빌드 산출물(66 KB · sha256 `8564ea08…`) |
| `extensions/index.json` | 카탈로그 한 줄(id · dir · name · version · kind · summary) — 패널 "설치 가능"에 뜬다 |
| `scripts/ext-build.ps1` | `$map`에 `project_explorer_menus` → `project-explorer-menus` 배치 줄 |
| `extensions/sdk/Cargo.toml` | members에 `samples/project-explorer-menus` |

## 3. 작업 기록 — 시간 순서(요청 → 추가/변경/삭제)

| 일시 | 요청(사용자) | 추가 | 변경 | 삭제 | 비고 |
|---|---|---|---|---|---|
| 09-30 | "Project Explorer Menus 기본 틀 · 호스트 함수 → 확장 호출 → wasm 빌드 → 등록 순서" | `extensions/sdk/samples/project-explorer-menus/{Cargo.toml, src/lib.rs}` · `extensions/project-explorer-menus/{extension.json, project_explorer_menus.wasm}` · 이 문서 | `extensions/mod.rs`(`run_with`) · `extensions/wasm.rs`(`nx_host_get` · `HostCtx.doc_path`) · `app/extensions.rs`(경로 전달) · SDK `lib.rs`(`host::get` · `query` · `Editor::doc_path`) · `extensions/sdk/Cargo.toml` · `scripts/ext-build.ps1` · `extensions/index.json` · [75 §3](75-extension-sdk-and-dynamic-loading.md) ABI 표 | — | 호스트 wasm 시험 11 · SDK 5 · 확장 2 통과 · 0.1.0 |

## 4. 지금 쓸 수 있는 SDK 표면(0.1.0 시점)

| 호스트 함수(import) | SDK API | 뜻 |
|---|---|---|
| `nx_editor_op(op, flag)` | `Editor::goto_bracket` · `expand_to_brackets` · `goto_bracket_sibling` · `goto_bracket_parent` · `goto_bracket_child` | 편집기 이동(호출 뒤 호스트가 큐를 적용) |
| `nx_log(ptr)` | `nexa_ext_sdk::log(msg)` | 앱 로그 한 줄(호출당 32줄 · 512자) |
| **`nx_host_get(kind, ptr, cap)`**(09-30) | `Editor::doc_path()` | 활성 문서 경로 |
| — | `Settings::get/flag/int` | `settings_prefix` 아래 설정값(`on_settings`) |
| — | `Meta { commands, menus, formatter }` | 팔레트 명령 · 편집기 우클릭 서브메뉴 · (포맷터는 [95](95-sql-formatter.md)) |

## 5. 다음 단계(사용자가 소스를 보며 하나씩 · 각 단계 = §1의 여섯 걸음)

1. **메뉴를 프로젝트 탐색기 OPEN FILES 행에도** — 메타 `menus[].target: "editor" | "project_explorer"`(ABI 추가분) → 호스트 `project_panel.rs` 우클릭 메뉴가 확장 항목을 붙인다 · 대상 파일 경로는 `run_with(doc_path = 그 행의 경로)`.
2. **파일 동작 op** — `EditorOps`/새 포트 `FileOps`(예: `open_in_new_tab(path)` · `reveal_in_project(path)` · `copy_path`) + `nx_editor_op` 번호 또는 새 import.
3. **설정** — `ext.project_explorer_menus.*` 키(예: 항목 표시 여부) → 호스트 레지스트리 등록(94 §6-4) → `on_settings`로 반영.
4. **클립보드** — 확장이 글을 클립보드에 넣는 op(호스트 `clipboard::write_text`)는 사용자 클립보드를 건드리므로 **명시적 명령에서만**.
5. 버전을 올리고(0.2.0 …) 패키지·인덱스 갱신 → 확장 패널 **업데이트** 버튼으로 교체 확인.
