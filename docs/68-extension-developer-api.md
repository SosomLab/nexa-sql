# 68 · 확장(Extension) 개발자 API — 설계 방향 · 현재 범위 · 추가/변경 대상 · 샘플 · 배포 · 동적 로딩/해제

> **요청**(사용자 09-22): *"Rainbow Pair의 실제 동작 코드를 분리해 실행 모듈도 플러그인으로 다운로드되게 하는 작업이 진행되지 않았다. 외부에서 기능을 쓰게 하려면 확장 개발자용 API가 필요 — 설계 방향 · 현재 구현 범위 · 추가/변경 대상 · 개발 샘플 · 배포 방법을 순서대로. 동적 로딩과 관리(당장 필요 없는 플러그인은 프로세스/메모리에서 내리고 해제) 검토. 방식·절차·범위는 물어볼 것."*
> **선행**: [50 확장 시스템](50-extension-system.md)(3층 · D-87~90 **미확정**) · [51 Rainbow Pairs](51-rainbow-brackets.md)(D-91 ① 코어 + in-process 층 ✅) · [22 §0 드라이버 확장](22-driver-extensions.md)(DR-29 in-process cdylib · 지연 dlopen) · [extensions/README.md](../extensions/README.md)(패키지 메타 v1) · [39 자원 거버넌스](39-resource-governance.md).

---

## 0. 결론 여섯 줄

1. **왜 진행되지 않았나**: D-91에서 "코어(쌍 표·자동 닫기)는 nexa-ctl에 두고, 표시·명령·메뉴만 in-process 확장 층에" 올리는 것까지 확정·구현했고(`extensions/rainbow_pairs.rs` · `Extension` 트레이트 · `builtin` 종류), **코드 실행 방식 D-87(WASM `wasmi` / JIT / Python / Lua)이 미확정**이라 "내려받는 실행 모듈"은 만들지 않았다. 지금 Rainbow Pairs는 앱에 컴파일된 채 `builtin` 패키지로 "설치 = 켜기"만 흉내 낸다.
2. **설계 방향 = 호스트 API 한 벌 · 실행기 셋**: 확장이 보는 표면(`ExtensionApi` — 편집기 · 결과 · UI · 설정 · 이벤트)을 **하나**로 정의하고, 그 위에 in-process(우리 코드) · **WASM**(내려받는 코드 · 샌드박스) · **네이티브 cdylib**(드라이버와 같은 길 · 신뢰된 배포자만) 세 실행기를 둔다. Rainbow Pairs를 **첫 WASM 확장**으로 옮겨 API를 검증한다.
3. **권장 실행 방식 = WASM(`wasmi`)**(D-87 ①): 샌드박스·능력 기반 권한·3-OS 한 바이너리·용량 0.6 MB · 인터프리터 속도(5~20×)는 표시·명령류에 충분. Rainbow Pairs의 무거운 부분(쌍 표 계산)은 **코어에 남고** 확장은 색·명령·메뉴만 하므로 성능 위험 없음. JIT(`wasmtime`)·Python 프로세스는 수요가 생기면.
4. **동적 로딩/해제 = 세 상태**(설치됨 → **적재됨**(모듈 인스턴스 · 메모리) → **활성**(이벤트 구독 중)): 활성화 시점 = `manifest.activation`(`onStartup` · `onCommand:<id>` · `onLanguage:sql` · `onView:<id>` — VS Code 활성화 이벤트 모델) · **유휴 해제** = 활성 구독이 없고 `extensions.unload_idle_secs` 동안 호출이 없으면 인스턴스 폐기(메모리 회수 · 다음 호출 때 재적재 · 상태는 `storage`에 자기 몫만) · 프로세스 확장은 같은 규칙으로 종료/재기동 · 관리 UI = 확장 패널 상태 열(설치·적재·활성·해제됨 · 메모리 KB · 마지막 호출) + "지금 내리기".
5. **개발자 경험**: `nexa-ext-sdk`(공개 · 형제 저장소 · Rust 크레이트 + WIT 파일) → `cargo build --target wasm32-unknown-unknown` → `extension.json`(`kind: wasm` · `capabilities` · `activation`) → 저장소 폴더/GitHub Releases에 올리면 매니저가 설치. 샘플 = `extensions/samples/hello-command`(팔레트 명령 하나) · `rainbow-pairs`(실전).
6. **범위 제안(1차 = 4주 규모)**: API v1 = 편집기 읽기/장식(색·밑줄)/캐럿 · 명령 · 메뉴 기여 · 설정 · 이벤트(편집·캐럿·설정) — Rainbow Pairs가 쓰는 것만 · 결과/네트워크/파일은 v2. **결정 D-87~90 + 아래 D-150~155에 답이 필요**(§7).

---

## 1. 현재 구현된 범위(코드 기준 · 09-22)

| 층 | 있음 | 없음 |
|---|---|---|
| 패키지·매니저(50 §10) | `index.json`/`extension.json` v1 · `Source::{Dir, Url}` · sha256 검증 · 설치본 `<설정>/extensions/<id>/…` · `installed.json` · 켬/끔 · 저장소 추가 · 팔레트 6명령 · 확장 패널 · 확장 뷰 탭 · 다운로드 추적 로그 · 관리자 켬/끔 = 모든 것의 문(50 §14) | `kind: wasm`/`process` 설치 **거부** · 서명된 인덱스(D-89) · 갱신(update) · 의존(`requires` 표시만) · 능력 승인 UI(D-90) |
| 확장 표면(`extensions/mod.rs`) | `Extension` 트레이트(id · settings_prefix · commands · menus · on_settings → `ExtensionEffect{bracket_opts}` · run) · `EditorOps`(괄호 이동 5 + 확장 선택) · `Registry::builtin()` · 메뉴 기여 → 편집 메뉴/우클릭 · 팔레트 등록 · 키맵 4 | 편집기 **읽기/장식 일반 API**(지금 효과 = `BracketOpts` 한 종류뿐) · 결과 그리드 · UI 알림/입력 · 이벤트 구독 · 저장소(상태) · 네트워크/파일 능력 |
| Rainbow Pairs(51) | 코어(nexa-ctl `pairs.rs` · `BracketOpts` · 자동 닫기 · 이동) + in-process 층(`rainbow_pairs.rs` — 설정 → 옵션 · 명령 6 · 서브메뉴) · 설정 8키 · BOOST 끔 | 색 계산·짝 없음·현재 쌍 표시가 **코어의 `BracketOpts` 렌더링**에 있어 확장 코드가 "장식을 그리는" 형태가 아님 → 분리하려면 장식 API가 먼저 |
| 드라이버 확장(22 §0) | 설계 = in-process cdylib · C ABI · 지연 dlopen(DR-29) | 구현 전(내장 드라이버 4종 정적 링크) |
| 자원 거버넌스 | 확장 없음 = 비용 0 · 관리자 꺼짐 = 전부 정지 | 확장별 메모리/시간 상한 · 유휴 해제 |

---

## 2. 설계 방향 — 호스트 API 한 벌 · 실행기 셋

```
                     ┌────────────── ExtensionApi(호스트가 구현 · WIT로도 내보냄) ──────────────┐
확장 코드 ──호출──▶ │ editor: text(range) · lines · caret/selection · set_caret · replace ·        │
                     │         syntax_tokens(range) · decorations.set(layer, spans{color,underline})│
                     │ results: active().columns/rows(page) (v2)                                     │
                     │ ui: command.register · menu.contribute(where, items) · notify · prompt ·      │
                     │     status · palette.open(items)                                              │
                     │ settings: get(own prefix) · on_change                                         │
                     │ events: on_edit(doc, version) · on_caret · on_open/close · on_run_before/after│
                     │ storage: get/set(own key) · (v2) net.fetch(allowlist) · fs(plugin dir)        │
                     └────────────────────────────────────────────────────────────────────────────┘
        ▲ 같은 표면                    ▲ 같은 표면                          ▲ 같은 표면
  ① in-process(Rust · 앱 안)   ② WASM(wasmi · 내려받음 · 샌드박스)   ③ native cdylib(C ABI · 신뢰 배포자 · 드라이버와 같은 길)
```

- **능력(capabilities)**: 매니페스트가 요구한 것만 링크(WASM import 실패 = 로드 거부 · 50 §4). Rainbow Pairs = `editor.read` · `editor.decorate` · `editor.caret` · `ui.menu` · `ui.command` · `settings`.
- **장식 모델**(새로 필요한 것): 편집기에 **레이어별 스팬 목록**(`(from, to, style{fg, underline, bold})`)을 확장이 통째로 넘기고 편집기는 그리기 때 합성 — 편집기 내부(쌍 표 · 토큰)는 노출하지 않는다. 쌍 표 계산은 코어가 하고 `editor.pairs(range)`(읽기 전용 조회)로 확장에 준다 → Rainbow Pairs WASM = "쌍 표를 받아 색 스팬을 만든다"로 얇아진다(호출 비용 = 편집당 1회 · 상한 `rainbowpair.max_kb`).
- **이벤트 배달·비용**: 편집 이벤트는 문서 버전 번호만 · 확장이 필요할 때 `text(range)`를 당긴다(복사 최소) · 호출당 연료(fuel)·시간 상한 · 초과 = 그 확장만 비활성 + 로그(50 §4).

---

## 3. 동적 로딩 · 해제 · 관리(사용자 09-22 검토 요청)

| 상태 | 뜻 | 들어가는 때 | 나오는 때 |
|---|---|---|---|
| 설치됨 | 파일이 `<설정>/extensions/<id>/…`에 있음 | 매니저 설치 | 삭제 |
| **적재됨** | 모듈 인스턴스 존재(WASM 메모리 · 프로세스 살아 있음) | 첫 활성화 이벤트 | **유휴 해제**(`extensions.unload_idle_secs` 기본 300 · 0 = 안 내림) · "지금 내리기" · 향상 모드(`perf.boost` = 활성 아닌 것 즉시 내림) · 오류/연료 초과 |
| **활성** | 이벤트 구독 중(명령 등록 · 편집 이벤트 받음) | `activation` 조건 충족(`onStartup` · `onCommand:` · `onLanguage:` · `onView:`) | 사용자가 끔 · 관리자 끔 · 조건 해제(뷰 닫힘 등 — 구독 해제 뒤 적재만 남김) |

- **적재 비용을 앱 기동에서 뺀다**: `onStartup`이 아닌 확장은 기동 때 매니페스트만 읽는다(명령 이름·메뉴 기여는 매니페스트에 **선언**되어 있어 적재 없이 팔레트/메뉴에 보인다 — VS Code `contributes` 모델). 처음 고르면 그때 적재.
- **해제 규칙**: 구독이 하나라도 살아 있으면 내리지 않는다(예 Rainbow Pairs는 SQL 탭이 열려 있는 동안 `on_edit` 구독 = 활성 → 안 내림 · 탭이 전부 닫히면 유휴 시계 시작). 내릴 때 확장의 `deactivate()`를 부르고 `storage`는 디스크에 남는다. 다시 올릴 때 `activate()`가 상태를 읽는다.
- **프로세스 확장**(③ 또는 50 §4 ③): 같은 상태 기계 · 내림 = 프로세스 종료(타임아웃 뒤 kill) · 크래시 = 재시작 안 함(사용자 재활성).
- **관리 UI**(확장 패널): 행에 상태 배지(설치·적재·활성) · 메모리(WASM 선형 메모리 페이지 수 × 64 KB · 프로세스 RSS) · 마지막 호출 시각 · 우클릭 "지금 내리기" / "항상 적재" · 설정 `extensions.unload_idle_secs` · `extensions.max_memory_mb`(확장당) · `extensions.timeout_ms`(호출당) · `extensions.max_loaded`(동시 적재 상한 — 넘으면 가장 오래 쉰 것부터 내림) — 39 §3 부하원 등재.
- **네이티브 cdylib는 내릴 수 없다고 본다**(Windows에서 `FreeLibrary` 뒤 잔존 포인터 위험 · 드라이버와 같이 프로세스 수명) → 종류 ③은 "적재 = 영구"로 표시하고 정말 내리려면 프로세스 격리(22 §0 선택지)로.

---

## 4. 추가 · 변경해야 할 대상(순서)

| # | 대상 | 내용 | 규모 |
|---|---|---|---|
| E1 | `nsql-ext-api`(새 크레이트 · 의존 0) | `ExtensionApi` 표면 정의(Rust 트레이트 + 데이터 타입) · 매니페스트 v2(`capabilities` · `activation` · `contributes{commands, menus, settings}`) · 능력 열거 · 순수 판정 `activation_due`/`unload_due`(MC/DC) | 중 |
| E2 | 편집기 장식 API | nexa-ctl `TextBox::set_decorations(layer, spans)`(색·밑줄 · 합성 그리기 · 캐시 = 줄 변경 기록 소비자 · 61 불변식) + `pairs(range)` 조회 · Rainbow Pairs의 색/짝 없음/현재 쌍 그리기를 **장식 레이어 하나**로 옮김(코어 옵션 `BracketOpts.rainbow` 제거) | 중 |
| E3 | in-process 실행기 재작성 | `Extension` 트레이트 → `ExtensionApi` 위로(`rainbow_pairs.rs`가 SDK 코드와 **같은 소스**가 되도록 — `#[cfg]`로 in-process/WASM 양쪽 빌드) | 소 |
| E4 | WASM 실행기 | `wasmi` 도입(DR-3 원장 · 39 부하원) · WIT ↔ 호스트 함수 바인딩 · 연료/시간/메모리 상한 · 능력별 import 필터 · 트랩 격리 | 대 |
| E5 | 로딩/해제 관리자 | 상태 기계(§3) · 활성화 이벤트 · 유휴 해제 · 패널 상태 열·메뉴 · 설정 4키 | 중 |
| E6 | 매니저 | `kind: wasm` 설치 허용(파일 = `.wasm` + manifest) · 서명 인덱스(D-89 · `nexa-license`의 Ed25519 재사용) · 갱신(update) · 능력 승인 화면(D-90) | 중 |
| E7 | SDK · 샘플 · 문서 | `../nexa-ext-sdk`(형제 저장소 · `nexa-ext` 크레이트 + `wit/` + `cargo generate` 틀) · `extensions/samples/hello-command` · Rainbow Pairs를 `extensions/rainbow-pairs/`에 **wasm 패키지**로 게시 · 개발 안내(50 §7 갱신) | 중 |
| E8 | CLI | `nsql ext list/install/remove/pack/verify`(개발자가 패키지 sha256·서명을 만든다) | 소 |

---

## 5. 확장 개발 샘플(목표 형태 · SDK v1)

```rust
// extensions/samples/hello-command/src/lib.rs  —  cargo build --target wasm32-unknown-unknown --release
use nexa_ext::prelude::*;

#[nexa_ext::extension]
struct Hello;

impl Extension for Hello {
    fn activate(&mut self, api: &mut dyn Api) {
        api.commands().register("hello.upper", "Hello: Uppercase Selection");
    }
    fn on_command(&mut self, api: &mut dyn Api, id: &str) {
        if id == "hello.upper" {
            let sel = api.editor().selection();
            let text = api.editor().text(sel.clone());
            api.editor().replace(sel, &text.to_uppercase());
            api.ui().status("uppercased");
        }
    }
}
```

```jsonc
// extension.json (v2 초안)
{ "format": 2, "id": "hello-command", "name": "Hello Command", "version": "0.1.0", "kind": "wasm",
  "module": "hello_command.wasm", "sha256": "…",
  "capabilities": ["editor.read", "editor.write", "ui.command", "ui.status"],
  "activation": ["onCommand:hello.upper"],
  "contributes": { "commands": [{ "id": "hello.upper", "title": "Hello: Uppercase Selection" }],
                   "menus": { "editor/context": [{ "command": "hello.upper" }] } } }
```

Rainbow Pairs(실전 샘플) = 위 틀 + `activation: ["onLanguage:sql"]` + `on_edit`에서 `api.editor().pairs(visible)` → `decorations.set("rainbow", spans)`.

---

## 6. 배포 방법

| 경로 | 개발자가 하는 일 | 앱이 하는 일 |
|---|---|---|
| 공식 저장소(`nexa-sql/extensions/`) | PR로 패키지 폴더 추가(`extension.json` + `.wasm` + `index.json` 항목) · CI가 sha256·서명·크기 상한·능력 검사 | 매니저가 `index.json` 읽어 설치 |
| 개인 저장소(GitHub) | 같은 구조의 폴더를 저장소 루트에 · Releases에 zip(선택) | "Extension Manager: Add Repository"(URL · D-89 서명 필수 여부) |
| 로컬 폴더 | 같은 구조 폴더 | "Add Repository"(폴더 · 개발 중 반복 설치) |
| 앱 동봉 | 앱 릴리스에 `.wasm`을 동봉(`builtin` 대신 `bundled` 종류) | 첫 실행에 설치 없이 적재 가능 · 갱신은 저장소에서 |

검증 도구: `nsql ext pack <dir>`(sha256 계산 · 매니페스트 검사 · 서명) · `nsql ext verify <dir|url>` · CI 워크플로 `extensions.yml`.

---

## 7. 결정이 필요한 것(사용자 답 뒤 착수)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| **D-87**(50) | 내려받는 코드의 실행 방식 | ① WASM `wasmi` ② WASM JIT `wasmtime` ③ Python 프로세스 ④ Lua ⑤ 네이티브 cdylib만 | **①**(+ ⑤는 신뢰 배포자용 2차) |
| **D-150** | 1차 범위 | ① Rainbow Pairs를 WASM으로 옮기는 데 필요한 API만(편집기 읽기·장식·캐럿·명령·메뉴·설정·이벤트) ② + 결과 그리드 읽기 ③ + 네트워크/파일 능력 | **①** — API를 실전 확장 하나로 검증한 뒤 넓힘 |
| **D-151** | Rainbow Pairs의 최종 형태 | ① 앱에서 **제거**하고 저장소 wasm 패키지로만(설치해야 보임) ② 앱 동봉(`bundled`) + 저장소 갱신 ③ 지금처럼 builtin 유지 + wasm은 샘플로만 | **②** — 첫 실행 경험 유지 · "다운로드되는 실행 모듈"도 충족 |
| **D-152** | 동적 해제 기본값 | ① 유휴 300초 뒤 내림 ② 내리지 않음(사용자가 수동) ③ 향상 모드에서만 | **①**(향상 모드는 즉시) |
| **D-153** | 활성화 모델 | ① VS Code식 활성화 이벤트(선언 기여 + 지연 적재) ② 설치되면 늘 적재 | **①** |
| **D-154** | SDK 위치·언어 | ① 형제 저장소 `nexa-ext-sdk`(Rust 크레이트 + WIT · 다른 언어는 WIT로) ② nexa-sql 안 `crates/nexa-ext` ③ Rust 외 언어 공식 지원(AssemblyScript·C) | **①**(Rust 우선 · WIT 공개로 타 언어 가능) |
| **D-155** | 서명·저장소 신뢰(D-89 구체화) | ① 공식 인덱스만 서명 · 개인 저장소는 경고 ② 모든 저장소 서명 필수 ③ 서명 없음 | **①** |
| D-88 · D-90(50) | Python 범위 · 능력 승인 시점 | 50 §6 | 권장 그대로(② · ③) |

**절차 제안**: 답 → E1·E2(API + 장식) → E3(Rainbow Pairs를 API 위로 · 동작 동일 확인 = 51 §11 실기표) → E4·E5(WASM + 로딩/해제) → E7(SDK·샘플 · Rainbow Pairs wasm 게시) → E6·E8. 각 단계마다 39 §6 게이트(편집·드래그·실행 속도 회귀 0).
