# 50 · 플러그인 시스템 도입 검토 — GitHub 배포 · 활성화 1회 · 목록/검색/설치 · 실행 방식(샌드박스·스레드·권한) · Python 대비 검토

> **상태**: 📐 검토(09-17 · 사용자 요청 "GitHub에서 다운로드 · 플러그인 매니저는 1회 활성화(네트워크 필요 인지) · 목록 조회·검색 · 설치 = 다운로드+자동 배치 · 플러그인 코딩 방식 · 인터프리터면 제약 환경·별도 스레드·권한 낮춤 · Python 가정으로 속도/성능/비용/복잡도/용량 전반 검토"). 결정 = **D-87~D-90**(§6 · 사용자 답 대기). 선행 = [DR-5](10-decision-record.md)(Sublime 패키지 구조) · [DR-16](10-decision-record.md)(플러그인 = 데이터 패키지 → WASM `wasmi` → Lua는 수요 시) · [22 드라이버 확장](22-driver-extensions.md)(GitHub Releases 다운로드 · 관리 UI · in-process cdylib) · [09 편집기·패키지](09-editor-and-packages.md) · [30](30-architecture-patterns.md) 확장점 규칙 · [26 §8](26-performance-architecture.md) 네트워크 규칙.

## 0. 결론 여섯 줄

1. **배포·관리는 22와 같은 골격**: GitHub Releases + 서명된 인덱스(JSON) → 플러그인 매니저 창(목록·검색·설치·갱신·삭제). 매니저는 **"활성화" 1회**(네트워크를 쓴다는 사실을 알리고 동의 · 설정 `plugins.enabled` + 인덱스 URL) — 그 전엔 트래픽 0(26 §8).
2. **실행 방식은 "3층"**: ① **데이터 패키지**(문법·스니펫·테마·키맵·인텔 데이터 — 코드 없음 · 대부분의 요구) ② **WASM 모듈**(`wasmi` 인터프리터 · 능력 기반 샌드박스 · 호스트 API만 · 별도 스레드 · 연료/메모리 상한 · **기본 선택**) ③ **외부 프로세스**(OS 샌드박스 · JSON-RPC · 무거운 언어/라이브러리 · 선택).
3. **Python은 ③으로만**: CPython을 프로세스 안에 넣으면 샌드박스가 **불가능**하다(언어 자체가 탈출 수단 · PEP 578 감사 훅도 "차단"이 아니라 "관찰"). 안전하게 쓰려면 **별도 프로세스 + OS 수준 격리**(Windows AppContainer/Job · macOS `sandbox-exec`/App Sandbox · Linux seccomp+namespaces/bwrap)가 필요하고, 그 비용(런타임 30~60MB · 기동 30~200ms · 3-OS 샌드박스 구현 · 배포 용량)이 크다.
4. **권한 낮춤(탈취 대응)**은 WASM에서 자연스럽다: 모듈은 **아무 권한도 없이** 시작하고 매니페스트가 요구한 능력(`editor.read` · `editor.write` · `results.read` · `net:host` · `fs:path`)만 호스트가 주입한다(deny-by-default). 프로세스 방식은 OS 기능으로 같은 효과를 내되 3-OS 구현이 각각이다.
5. **속도**: WASM 인터프리터(`wasmi`)는 네이티브의 5~20배 느리지만 플러그인이 하는 일(텍스트 변환 · 메뉴 명령 · 결과 후처리)에는 충분하고 JIT(`wasmtime`) 옵션이 있다. Python 프로세스는 계산 자체는 빠를 수 있으나 **왕복(IPC)과 기동**이 지배한다.
6. **권장**: ①+② 우선(수요의 90%) · ③은 "Python 스크립트 플러그인"이 정말 필요할 때 `python3`를 **사용자 환경에서 찾아**(내장 안 함) 격리 프로세스로 실행하는 **선택 기능**으로. Python을 **번들**(내장)하는 것은 용량·보안·유지 비용 때문에 비권장.

## 1. 배포·매니저(사용자 요구 ①~③)

| 요소 | 설계 | 근거 |
|---|---|---|
| 저장소 | GitHub `SosomLab/nexa-sql-plugins`(공식 인덱스) + 서드파티 저장소 URL 추가 가능 | 22 §2 드라이버와 같은 채널 |
| 인덱스 | `index.json`(플러그인 id · 이름 · 설명 · 종류(data/wasm/process) · 버전들 · 릴리스 자산 URL · sha256 · 서명 · 요구 능력 · 최소 앱 버전) — Releases 자산으로 게시 · **Ed25519 서명**(nexa-license 키 재사용) | 22 §6 보안 |
| 활성화 1회 | 메뉴 도구 ▸ 플러그인 매니저 → 첫 열기 때 카드 "인덱스를 GitHub에서 내려받습니다(네트워크 사용) · 활성화" → `plugins.enabled=on` · 이후 새로고침은 **사용자 버튼**(자동 폴링 없음 · 26 §8) | "네트워크 접속이 필요함을 알아야" |
| 목록·검색 | 설치됨/사용 가능 탭 · 이름·설명·태그 검색(로컬 필터 · 서버 재조회 없음) · 종류 아이콘 · 요구 능력 표시 | |
| 설치 | 다운로드(진행률 · 취소) → sha256/서명 검증 → `<config>/plugins/<id>/<version>/` 배치 → 매니페스트 읽어 등록 → (data) 즉시 · (wasm) 다음 실행부터 또는 즉시 로드 · (process) 실행 파일 존재 확인 | "설치 = 다운로드 및 자동 배치" |
| 갱신·삭제 | 버전 비교 · SxS 보관(드라이버와 동일) · 삭제 = 폴더 제거 + 등록 해제 | 22 §5 |
| 오프라인 | `.nexa-plugin`(zip) 파일에서 설치 | |

## 2. 실행 방식 비교(플러그인 "코드"를 어떻게 쓰게 할 것인가)

| 기준 | ① 데이터 패키지 | ② WASM(`wasmi`) | ②' WASM(`wasmtime` JIT) | ③ Python 프로세스(사용자 python3) | ③' Python 내장(CPython/PyO3) | Lua(`mlua`) · Rhai | JS(QuickJS/Boa) |
|---|---|---|---|---|---|---|---|
| 샌드박스 | 해당 없음 | ✅ 능력 기반 · 선형 메모리 격리 · 호스트 API만 | ✅ 동일 | △ OS 샌드박스 필요(3-OS 각각) | ❌ 불가(같은 프로세스 · 언어 탈출) | △ 라이브러리 제한으로 부분(Lua는 비교적 좋음) · 메모리 상한 가능 | △ 엔진에 따라 |
| 별도 스레드·상한 | — | ✅ 스레드 + 연료(fuel)·메모리·시간 상한 | ✅ | ✅ 프로세스 = 최고 격리 · kill 가능 | ❌ GIL·전역 상태 · 패닉 전파 | ✅ 스레드 + 인스트럭션 카운트 | △ |
| 권한 낮춤 | — | ✅ deny-by-default · 매니페스트 능력 | ✅ | ✅ OS 수준(토큰 낮춤 · 파일/네트워크 정책) | ❌ | △ | △ |
| 속도(계산) | — | 네이티브 ÷5~20 | 네이티브 ÷1.5~3 | CPython(느림) + IPC 왕복 0.1~1ms | CPython | Lua ÷10~30 · LuaJIT 빠름 | ÷10~50 |
| 기동 | 0 | 모듈 로드 1~10ms | 컴파일 10~100ms(캐시 가능) | 30~200ms/프로세스 | 20~50ms(인터프리터 초기화) | 1ms | 5ms |
| 바이너리 증가 | 0 | ~0.6MB | ~8~15MB | 0(사용자 설치) / 번들 시 **30~60MB** | 30~60MB + 3-OS 빌드 복잡 | 0.5~1MB | 1~3MB |
| 언어·생태계 | JSON/규격 | Rust·C·Go·AssemblyScript·(Python→WASM 실험) | 동일 | Python 전체 · pip | Python 전체 | Lua | JS |
| 개발 복잡도(우리) | 낮음 | 중(호스트 API 설계 · WIT 인터페이스) | 중+ | 중+(JSON-RPC · 3-OS 샌드박스 · 프로세스 수명) | 높음(빌드 · 배포 · 보안 불가) | 낮~중 | 중 |
| 사용자 복잡도 | 낮음 | 중(빌드 도구 필요) | 중 | 낮음(스크립트) · 단 python3 설치 필요 | 낮음 | 낮음 | 낮음 |
| 기존 결정 | DR-5 ✅ | DR-16 ✅ | DR-16 후속 | 22 §0 "프로세스는 격리 선택" | ✗ | DR-16 "수요 시" | — |

## 3. Python 가정 — 전반 검토(사용자 요구 ⑤)

| 항목 | 내장(CPython 번들) | 사용자 python3 + 격리 프로세스 |
|---|---|---|
| **속도** | 호출 왕복 없음 · 인터프리터 초기화 20~50ms · GIL로 병렬 불가 | 프로세스 기동 30~200ms(상주시키면 1회) · 호출마다 IPC 0.1~1ms · 대량 결과 전달은 직렬화 비용(100k행 = 수십 ms~초) |
| **성능(격리)** | 무한 루프·메모리 폭주가 앱을 잠근다 · 크래시 = 앱 크래시 | kill·타임아웃·메모리 상한(OS) 가능 · 앱 무영향 |
| **보안** | 샌드박스 **불가**(ctypes·os·importlib · 감사 훅은 관찰용) | OS 샌드박스로 가능하나 3-OS 각각: Windows AppContainer/제한 토큰+Job Object · macOS `sandbox-exec`(deprecated but works)/App Sandbox · Linux bwrap/seccomp/namespaces — 구현·검증 비용 큼 |
| **비용(개발)** | PyO3 빌드 3-OS · 인터프리터 배포 · 취약점 추적 | JSON-RPC 프로토콜(22와 공유) · 샌드박스 래퍼 · 프로세스 관리 |
| **복잡도(운영)** | Python 버전·ABI 고정 · 패키지 의존 지옥 | 사용자 환경 python 없음/버전 차이 → 안내 필요 · venv 관리 |
| **용량** | +30~60MB(3-OS 각각) · 앱 크기 2~3배 | 0 |
| **적합한 일** | — | 데이터 후처리(pandas) · 외부 API 연동 · 리포트 생성 |
| **판정** | ✗ 비권장 | △ 선택 기능(③) · 기본 아님 |

## 4. 권장 구조(3층)

```
플러그인 매니저 ── index.json(서명) ── GitHub Releases
      │ 설치 = 내려받기 → 검증 → <config>/plugins/<id>/<ver>/manifest.json
      ▼
 ① data:  Packages/<id>/*.sublime-syntax · *.sublime-snippet · *.nexa-intel · theme · keymap  → 기존 레지스트리에 등록(코드 0)
 ② wasm:  plugin.wasm + manifest(capabilities) → wasmi 인스턴스(스레드 1 · fuel · memory ≤ N MB · 시간 상한)
           호스트 API(WIT): editor.get_text/replace_selection · results.rows(page) · ui.notify/prompt · settings.get(own) · net.fetch(allowlist) · fs(plugin dir only)
           이벤트: command(id) · on_run_before/after · on_result · on_save · format_document
 ③ process: exec + args(매니페스트) · JSON-RPC over stdio(22와 같은 규약) · OS 샌드박스 프로필 · 타임아웃/kill · 능력 = 같은 목록을 정책으로
```

- **권한 낮춤 규칙**: 매니페스트 `capabilities`가 요구하지 않은 API는 링크조차 되지 않는다(WASM은 import 실패 → 로드 거부 · 프로세스는 RPC 메서드 거부). 설치 화면에 능력 목록을 보여 주고 사용자가 승인 · 나중에 설정에서 능력 단위로 끌 수 있다.
- **격리·복구**: WASM 트랩/연료 소진 → 그 플러그인만 비활성 + 로그 · 프로세스 크래시 → 재시작 안 함(사용자 재활성).
- **부하원 등재**(39 §3): 플러그인 스레드/프로세스 · 상한 설정 `plugins.max_memory_mb` · `plugins.timeout_ms` · `plugins.max_running`.

## 5. 단계(제안)

1. 매니저 골격(활성화 · 인덱스 · 목록/검색 · 설치/삭제 · data 패키지) — 22의 다운로드·검증 코드 재사용.
2. WASM 런타임(`wasmi` · 호스트 API 1차: 명령 · 편집기 텍스트 · 알림) + 샘플 플러그인(Rust → wasm) + 능력 승인 UI.
3. 결과 후처리 API(페이지 단위) · `format_document` 훅(포매터 플러그인).
4. (선택) 프로세스 플러그인 + Python 러너 템플릿 + 3-OS 샌드박스 프로필.

## 7. 플러그인 개발자는 어디서 · 어떤 언어로 · 어떻게 만드는가(사용자 09-17 질문)

"코어 + in-process"는 **우리(앱 저장소 안 · Rust)** 가 첫 플러그인을 만드는 형태이고, **외부 개발자**는 아래 경로로 만든다. 두 경로가 **같은 호스트 API**를 본다는 것이 핵심이다.

| 개발자 | 어디서 | 언어 | 어떻게 | 배포 |
|---|---|---|---|---|
| 우리(코어 팀) | `nexa-sql` 저장소 `crates/nexa-sql/src/plugins/*.rs` | Rust | `PluginHost` 트레이트에 대고 구현 · 앱과 함께 빌드 | 앱 릴리스 |
| **외부(WASM · 기본)** | 자기 저장소(템플릿 `nexa-plugin-template`에서 시작) | **Rust**(1급 · SDK crate) · C/C++ · Zig · AssemblyScript · TinyGo(같은 WIT 바인딩) | ① `nexa-plugin-sdk`(WIT에서 생성된 바인딩 + 헬퍼)로 `on_edit`/`command` 등 export 구현 ② `cargo build --target wasm32-unknown-unknown`(또는 `cargo component`) → `plugin.wasm` ③ `manifest.json`(id · 버전 · 능력 · 명령 · 메뉴 · 설정 스키마) ④ **로컬 개발 모드**: 설정 `plugins.dev_dir`에 폴더를 두면 매니저 없이 로드 · 파일이 바뀌면 다시 로드(핫 리로드) · 로그 창에 플러그인 로그 층 ⑤ 테스트: SDK의 **호스트 목(mock)** 으로 `cargo test`(텍스트 → 데코레이션/명령 결과) | GitHub Releases에 `plugin.wasm` + `manifest.json` 자산 → 공식 인덱스 PR 또는 자기 인덱스 URL(서명) |
| 외부(프로세스 · 선택) | 자기 저장소 | Python · Node · 아무거나(stdio JSON-RPC) | 러너 템플릿(`nexa-plugin-python`) · 같은 메서드 이름(`command`, `on_edit`…) · 매니페스트 `exec` | 동일(자산 = 스크립트 + 매니페스트 · python3는 사용자 환경) |

- **호스트 API 표면은 하나**: `PluginHost` 트레이트(Rust) ↔ WIT(`nexa:plugin/host`) ↔ JSON-RPC 메서드 — 이름·인자·의미가 같다. in-process 플러그인 코드는 SDK와 같은 타입을 쓰므로 **파일을 옮겨 wasm으로 빌드하면 그대로 외부 플러그인**이 된다(레인보우 괄호를 첫 샘플로 공개).
- **개발 도구**: `nsql plugin new <id>`(템플릿 생성) · `nsql plugin check <dir>`(매니페스트·능력·크기 검사) · `nsql plugin pack`(zip) · 앱 안 "플러그인 개발" 패널(dev_dir 로드 · 다시 로드 · 로그).
- **문서**: 호스트 API 레퍼런스(WIT에서 생성) · 능력 목록 · 샘플 3종(데코레이션 · 명령/포매터 · 결과 후처리).

## 6. 의사결정(사용자 답 대기)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| D-87 | 코드 플러그인의 기본 실행 방식 | ① WASM(`wasmi`) ② WASM JIT(`wasmtime`) ③ Python 프로세스 ④ Lua | **①**(DR-16 유지 · 용량 0.6MB · 샌드박스 기본) · JIT는 수요 시 |
| D-88 | Python 지원 범위 | ① 없음 ② 사용자 python3 + 격리 프로세스(선택 기능) ③ 내장 | **②** |
| D-89 | 인덱스 서명·출처 | ① 공식 인덱스만 ② 공식 + 사용자 추가 URL(서명 필수) ③ 서명 없음 | **②** |
| D-90 | 능력 승인 시점 | ① 설치 때 1회 ② 첫 사용 때 ③ 둘 다(설치 때 표시 · 위험 능력은 첫 사용 때 재확인) | **③** |

## 8. VS Code · Sublime Text 확장 모델 분석 — 우리 방식(D-91 코어 + in-process/WASM 층)과 비교(사용자 09-17 두 질문)

> 질문 ① "현재 고려중인 방식이 Sublime이나 VS Code 기준 확장성과 개발자 편의성이 어떤지" · ② "VS Code 방식과 Sublime 방식을 분석 — 개발 방식·언어·SDK 구성·편의성·확장성·기능 구현 범위".

### 8-1. 두 모델의 구조

| 항목 | **VS Code (Extension Host)** | **Sublime Text (Python 패키지)** | **Nexa SQL (제안)** |
|---|---|---|---|
| 실행 위치 | **별도 프로세스**(Extension Host · Node.js · 데스크톱은 로컬 프로세스 · 웹은 Web Worker) — UI 프로세스(Electron 렌더러)와 **IPC(JSON-RPC)** | **같은 프로세스** — 내장 **CPython 3.8** 인터프리터(3.3 호환 층도 유지) · UI 스레드와 분리된 플러그인 스레드 · 호출은 함수 호출 | 3층: **코어(Rust · 내장)** · **in-process 플러그인(Rust · 호스트 API만)** · **WASM(`wasmi` · 샌드박스)** · (선택) 프로세스(Python 등) |
| 언어 | **TypeScript/JavaScript**(1급) · WASM 모듈을 JS로 감싸 사용 가능(2023~ "wasm-wasi-core" 실험) · LSP/DAP 서버는 **어떤 언어든**(별도 프로세스 · 표준 프로토콜) | **Python**(유일) · 네이티브 확장(C) 불가에 가까움(서브프로세스로 우회) | **Rust**(1급 · in-process/WASM) · **C/C++/Zig/AssemblyScript/TinyGo**(WASM) · Python(프로세스 · 선택) |
| SDK 구성 | `@types/vscode`(API 타입) · `yo code` 생성기 · `vsce` 패키징/게시 · `package.json`의 **contribution points**(명령·메뉴·키·설정·언어·테마·뷰 · 선언형) · **activation events**(지연 활성화) · 디버깅 = F5로 Extension Development Host 실행 · 테스트 러너 `@vscode/test-electron` · 마켓플레이스(서명·검증·통계) | `sublime`/`sublime_plugin` 모듈 2개(API 문서 = 한 페이지) · **파일을 `Packages/`에 두면 끝**(패키징 없음 · `.sublime-package` = zip) · 메뉴/키/설정/문법 = **JSON/YAML 파일**(선언형 · 리소스로 오버레이) · 콘솔(`View ▸ Show Console`)로 즉시 실행 · **Package Control**(커뮤니티 · 채널 JSON · git 태그 기반) | `nsql-plugin-api`(Rust 크레이트 · WIT 인터페이스) · `nsql plugin new/check/pack` · `plugin.toml`(명령·메뉴·키·설정 · 선언형) · `plugins.dev_dir` 핫 리로드 · 매니저 = GitHub Releases + 서명 인덱스(§1) |
| 편의성(개발자) | **최고**: 타입 완성 · 디버거 붙음 · 방대한 예제(80k+ 확장) · 웹뷰로 임의 UI · 다만 **빌드 도구 체인 필수**(npm · tsc · esbuild) · 활성화/IPC 비동기 모델 학습 곡선 | **가장 낮은 진입 장벽**: 파일 하나 저장 → 즉시 반영(리로드 자동) · REPL · 동기 API(단순) · 반면 타입/디버거 없음 · API 좁음 · UI는 퀵패널/입력패널/팬텀(HTML 미니)뿐 | **중간**: Rust 컴파일 필요(진입 장벽 ↑) · 타입 안전 · 핫 리로드로 반복은 빠름 · 디버거 = 네이티브(in-process는 `cargo run` 그대로 · WASM은 로그/트레이스) |
| 확장성(어디까지) | **가장 넓음**: 언어 서버 · 디버거 · 터미널 · SCM · 웹뷰(임의 HTML UI) · 트리뷰 · 커스텀 에디터 · 노트북 · 원격/컨테이너 · 인증 · 테마/아이콘 · 태스크 · 설정 UI 자동 | **편집기 중심**: 텍스트 명령 · 이벤트 리스너 · 뷰/영역/팬텀 · 퀵패널 · 문법/테마/스니펫/빌드 시스템 · 설정 · 하위 프로세스 · **커스텀 UI 없음** | **편집·결과·메타 중심**(51 §10 수준 1~3): 텍스트/선택/토큰/데코레이션 · 명령/메뉴/키/팔레트 · 결과 후처리 · 인텔 후보 · 선언형 폼 · HTTP(능력) · 문법/테마 데이터 · **캔버스 UI·드라이버는 불가**(cdylib 경로 22) |
| 성능·격리 | 프로세스 분리 → **UI가 멈추지 않음**(느린 확장은 격리) · IPC 왕복 비용(문서 전체 동기화는 증분 이벤트로) · 확장 하나가 Host 전체를 죽일 수 있음(같은 프로세스 안의 확장끼리는 격리 없음) · 권한 모델 **없음**(확장 = 사용자 권한 전체 · 워크스페이스 트러스트만) | 같은 프로세스 → **빠르고 단순**, 그러나 잘못된 플러그인이 편집기를 느리게/불안정하게 함 · GIL · 권한 모델 없음 | in-process = 0 비용/무격리(우리 코드만) · WASM = 5~20× 느림(인터프리터)/**능력 기반 샌드박스**(파일·네트워크·클립보드 승인) · 스레드 없음(호스트 작업 큐) |
| 배포·갱신 | 마켓플레이스(중앙 · 심사 최소 · 서명 · 자동 갱신 · 통계) · VSIX 수동 | Package Control(중앙 채널 JSON · git 저장소 직접 · 갱신 = 태그) · 수동 폴더 복사 | GitHub Releases + 서명 인덱스(공식 + 사용자 URL) · 자동 갱신 · 능력 승인 |
| 기능 구현 범위(같은 "레인보우 괄호"로) | 내장(2021~ `editor.bracketPairColorization` · 확장 "Bracket Pair Colorizer"가 원조 · 데코레이션 API로 구현 가능 · 100k줄에서 느려 내장으로 흡수) | 패키지 "RainbowBrackets"(Python · `view.add_regions` 데코레이션 · 뷰 변경 이벤트 · 큰 파일에서 느림 → 상한 설정) | 51 §9: **스캔·자동 닫기는 코어**, 색·메뉴는 플러그인 층 — VS Code가 내장으로 흡수한 이유(성능)를 처음부터 반영 |

### 8-2. 우리 방식의 위치(질문 ①)

- **확장성**: VS Code(웹뷰·언어 서버·원격) > **Nexa(수준 1~3 + 드라이버는 cdylib)** ≈ Sublime(편집기 API) — SQL 클라이언트에 필요한 범위(편집·결과·메타·포매터·완성 후보·리포트)는 Sublime 수준으로 충분하고, "임의 UI"는 선언형 폼까지만 둔다(웹뷰 없음 = DR-1 · 바이너리 크기). 언어 서버는 필요 시 **프로세스 플러그인 + LSP 어댑터**로 같은 형태를 재사용할 수 있다(50 §2 ③).
- **개발자 편의성**: Sublime(파일 저장 즉시) > VS Code(빌드 체인 + F5 디버그 · 타입 완성) > **Nexa(Rust 컴파일)** — 진입 장벽은 셋 중 가장 높다. 보완: ① `nsql plugin new` 템플릿 + `plugins.dev_dir` 핫 리로드(저장 → 재빌드 → 자동 재적재 · Sublime의 "즉시 반영" 감각) ② **데이터 패키지(0층 · 문법/테마/스니펫/키맵 = 파일만)** 는 Sublime과 같은 무컴파일 경로 ③ 선언형 `plugin.toml`(VS Code contribution points 방식 · 활성화 없이 메뉴/키/설정 등록) ④ 나중에 프로세스 플러그인으로 Python(Sublime 사용자층 흡수).
- **격리·안전**: VS Code·Sublime 둘 다 권한 모델이 없다(확장 = 사용자 권한). WASM 능력 승인은 두 모델보다 **앞서는 지점**이고, DB 클라이언트(자격 증명·데이터)에는 필요하다. 비용은 인터프리터 속도 — 편집 경로의 무거운 계산은 코어에 두는 51 §9 분할로 상쇄한다.
- **API 표면 크기**: VS Code API는 수천 항목(유지 비용 큼) · Sublime은 ~150 함수(작고 안정 · 10년 호환). 우리는 **Sublime 쪽**(작은 표면 · WIT 한 파일 · 버전 1개)을 따르고, contribution은 선언형(VS Code 쪽)으로 둔다 — 두 모델의 장점 조합.
- **결론**: 확장성은 목적(SQL 클라이언트)에 맞는 범위에서 Sublime과 동급, 편의성은 컴파일 언어라 한 단계 낮되 핫 리로드·템플릿·데이터 패키지로 메우고, 격리는 두 모델보다 낫다. 첫 플러그인(51)으로 in-process API를 검증한 뒤 같은 표면을 WASM export로 옮겨 "두 층이 같은 API"임을 실증하는 것이 다음 단계(T-118 ②).

### 8-3. 개발 워크플로 대비(같은 일을 세 곳에서)

| 단계 | VS Code | Sublime | Nexa |
|---|---|---|---|
| 시작 | `npx yo code` → 폴더 · `package.json` · `extension.ts` | `Packages/My/my.py` 파일 하나 | `nsql plugin new my` → `plugin.toml` · `src/lib.rs` |
| 명령 등록 | `contributes.commands` + `registerCommand` | `class MyCommand(sublime_plugin.TextCommand)` (클래스 이름 = 명령) | `plugin.toml [[commands]]` + `#[command]` 함수(export) |
| 메뉴/키 | `contributes.menus/keybindings` | `Main.sublime-menu` · `Default.sublime-keymap` | `plugin.toml [[menus]]/[[keys]]` |
| 설정 | `contributes.configuration`(스키마 → 설정 UI 자동) | `My.sublime-settings`(JSON · UI 없음 · 파일 편집) | `plugin.toml [[settings]]` → REGISTRY 등재(설정 창 자동 · i18n 라벨) |
| 실행·확인 | F5 → 새 창 · 디버거 | 저장 → 자동 리로드 · 콘솔 | `cargo build --target wasm32-wasip1` → `plugins.dev_dir` 감시 재적재 · 로그 창 `⟨plugin⟩` 층 |
| 배포 | `vsce publish` | Package Control PR(채널 JSON) | `nsql plugin pack` → GitHub Release + 인덱스 PR(서명) |
