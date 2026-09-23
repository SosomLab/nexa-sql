# 75 · 확장 SDK · 동적 로딩 — 조사 보고서 + 구현(사용자 09-23)

> 요청: "Extension에서 필요한 **SDK 구성 방법과 샘플 프로젝트** · 그 위에 **Rainbow Pairs를 개발한 소스와 컴파일되어 동적 로딩될 파일의 별도 생성·업로드** · 이를 **내려받아 적용하는 동적 로딩 기술**을 상세히 조사해 보고서로 · 미개발분은 개발까지".
> 결과: **전부 개발했다** — SDK 크레이트 `nexa-ext-sdk` · 샘플 2종 · WASM 런타임(`wasmi`) · 매니저 `kind = wasm` 설치 · 공식 패키지 `rainbow-pairs 1.1.0`(76 KB `.wasm`) · 빌드/배치 스크립트. 남은 것은 §9(설정 스키마 동적 등록 · 동적 명령 라벨 · 개발 모드 핫 리로드 · 서명 · 능력 승인).
> 선행 문서 = [50 확장 시스템](50-extension-system.md)(§2 실행 방식 비교 · §4 3층 · §7 개발자 경로 · §12 SDK 최소 표면 · §13 테스트) · [68 확장 개발자 API](68-extension-developer-api.md) · [22 §0 DR-29](22-driver-extensions.md)(드라이버는 cdylib) · nexa-dir2 ADR-0005(wasmi 미리보기 플러그인 선례).

## 0. 결론 여섯 줄

1. **동적 로딩 기술 = WASM(`wasmi` 인터프리터)** — D-87 ①을 구현으로 확정. 한 `.wasm`이 3-OS·모든 CPU에서 같은 파일(OS별 빌드·서명·로더 차이 0) · 샌드박스 기본(연료·메모리·시간 상한 · 호스트 import 외 접근 불가) · 바이너리 +0.6 MB(실측 §8) · 정책 층은 계산이 거의 없어 인터프리터 속도로 충분.
2. **cdylib(C ABI · dlopen)는 UI 확장에 쓰지 않는다** — OS별 3벌 산출물 · 격리 0(패닉·메모리 오염이 앱을 죽임) · Rust ABI 불안정 → C ABI 코덱 필요. 드라이버(DR-29)처럼 "네이티브 라이브러리를 붙여야만 하는 것"에만 남긴다.
3. **SDK = 게스트 크레이트 하나 `nexa-ext-sdk`**(의존 0 · `extensions/sdk/`) — 앱의 `Extension` 트레이트 5메서드를 그대로 `trait Extension { meta · on_settings · disabled · run }`로 옮기고 `export_extension!(MyExt)` 한 줄이 wasm export 5개를 만든다. 버퍼 = 4바이트 LE 길이 + UTF-8 · 메타/설정/효과 = JSON(ABI v1).
4. **Rainbow Pairs = 첫 WASM 확장**(`extensions/sdk/samples/rainbow-pairs` → `extensions/rainbow-pairs/rainbow_pairs.wasm` · kind `wasm` · 1.1.0). 설치하면 **내장판을 대체**하고(같은 id) 로드에 실패하면 내장이 폴백. 쌍 표·색칠·이동·자동 닫기는 여전히 코어(nexa-ctl) — 확장은 색 층만(51 §13).
5. **업로드 = 저장소 폴더**(`extensions/<id>/` + `index.json` 한 줄 → `main`에 병합되면 raw URL로 즉시 배포 · 지금 방식) 또는 **GitHub Releases 자산**(`files[].url` 절대 URL · 이번에 추가). 둘 다 sha256 필수.
6. **내려받아 적용** = 매니저가 `extension.json` → 파일 GET(curl) → sha256 검증 → `<설정>/extensions/<id>/<ver>/` 보관 → `installed.json` 기록 → 호스트 `Registry::sync_wasm`이 모듈 로드(검증·컴파일 · 메타 export 1회 호출) → 설정 적용 · 명령/메뉴 등록. 삭제 = 내림 + 내장 복귀. 앱 재시작 불필요.

## 1. 현황 조사(구현 전)

| 층 | 있었던 것 | 없었던 것 |
|---|---|---|
| 호스트 API | `Extension` 트레이트 5메서드 · `EditorOps` 5 · `ExtensionEffect{bracket_opts}` · `Registry`(내장만) | 문자열 id/라벨(전부 `&'static str`·`Msg`) · WASM 인스턴스 |
| 매니저 | `index.json`/`extension.json` v1 · GET(curl)+sha256+보관+배치+`installed.json` · 켬/끔/삭제 · 패널·뷰 탭 · 다운로드 추적 | `kind = wasm`/`process` **설치 거부** |
| 런타임 | nexa-dir2 `preview/wasm.rs`(wasmi 1.1 · 연료·메모리·시간·브레이커) 선례 | nexa-sql 안에는 없음 |
| SDK | 50 §7 표(`nexa-plugin-sdk` · WIT · 템플릿)는 **설계만** | 크레이트 · 샘플 · 빌드 스크립트 |
| 결정 | DR-16(WASM) · D-87 ①(권장) · D-88~90(대기) | — |

## 2. 실행 방식 재확인(이번 요구 기준)

| 기준 | WASM(`wasmi`) ✅ | cdylib(C ABI) | 프로세스(JSON-RPC) |
|---|---|---|---|
| 산출물 | **1개**(3-OS · x64/arm64 공통) | OS × CPU **6개**(win/mac/linux × x64/arm64) | 스크립트/실행 파일 OS별 |
| 격리 | 연료·메모리·시간 상한 · import 외 접근 0 · 트랩 = 그 확장만 | 없음(패닉 = 앱 종료 · `catch_unwind` 한계) | 완전(프로세스) |
| 호출 비용 | 인스턴스 생성 ~0.1 ms + 인터프리트(정책 층은 µs) | 0 | 왕복 0.1~1 ms |
| ABI | 버퍼+JSON(안정 · 언어 무관 · Rust/C/Zig/AssemblyScript) | C vtable 코덱 설계·버전 관리 | JSON |
| 앱 크기 | +0.6 MB(wasmi) | 0 | 0 |
| 배포·서명 | 파일 1 + sha256 | OS별 코드 서명(macOS notarize) | OS별 |
| 결론 | **UI 확장 기본**(DR-16) | 드라이버(DR-29)만 | 격리 선택 모드(T-118 ④) |

## 3. ABI v1(호스트 ↔ 게스트 계약)

```
게스트 export                        호스트 import(env)
  memory                               nx_editor_op(op: i32, flag: i32) -> i32   op = 1 goto_bracket · 2 expand · 3 sibling_prev ·
  nx_alloc(len) -> ptr                                                             4 sibling_next · 5 parent · 6 child
  nx_ext_meta() -> ptr                 nx_log(ptr)                                 로그 한 줄(호출당 32줄 · 512자)
  nx_ext_settings(ptr) -> ptr
  nx_ext_disabled() -> ptr
  nx_ext_run(ptr) -> i32
버퍼 = [len: u32 LE][UTF-8 bytes]   (nexa-dir2 ADR-0005와 같은 규약)
```

| 호출 | 입력 | 출력 |
|---|---|---|
| `nx_ext_meta` | — | `{"abi":1,"id","name","settings_prefix","commands":[{id,label:{en,ko?}}],"menus":[{id,label,items:[cmd id]}]}` |
| `nx_ext_settings` | `{"<prefix>key":"원문",…}`(레지스트리가 아는 접두 키만) | `{"bracket":{"rainbow","unmatched","colors":["#RRGGBB"],"max_chars"}}` — 없는 키 = 앱 기본 |
| `nx_ext_disabled` | — | 같은 효과 JSON(끈 상태) |
| `nx_ext_run` | 명령 id | `1` 처리 / `0` 아님 · 편집기 조작은 `nx_editor_op`로 **요청**(호스트가 큐에 모아 호출 뒤 적용) |

- **호출마다 새 인스턴스**(게스트는 상태를 두지 않는다) · 연료 5천만 · 메모리 16 MB · 벽시계 200 ms · 반환 1 MB · 모듈 8 MB · 연속 실패 3회 = 세션 동안 정지.
- 편집기 조작을 **큐**로 한 이유: 호스트 상태(`&mut dyn EditorOps`)를 게스트 호출 안에 빌려주지 않는다(수명·재진입·unsafe 0). 정책 층 명령은 "이동 하나"라 큐로 충분하다.
- 확장이 정하는 것은 **색 층뿐**(`rainbow`·`unmatched`·`colors`·`max_chars`). 쌍 종류·문자열 안·현재 쌍·자동 닫기는 편집 코어 설정(`editor.pair_*` · 51 §13) — 호스트가 덮어쓴다.

## 4. SDK 구성(`extensions/sdk/`)

```
extensions/sdk/
  Cargo.toml                 [workspace] nexa-ext-sdk · samples/* · release = opt "z" + lto + panic abort + strip
  .cargo/config.toml         [build] target = "wasm32-unknown-unknown"
  nexa-ext-sdk/              SDK(의존 0): json.rs(값·파서·직렬화) · lib.rs(Meta/Command/Menu/Label · Effect/BracketEffect ·
                             Settings(get/flag/int) · Editor(op) · log · trait Extension · export_extension! · buf)
  samples/hello-ext/         최소 샘플(명령 1 · 로그)
  samples/rainbow-pairs/     Rainbow Pairs 정책 층(앱 rainbow_pairs.rs와 같은 동작 · 시험 1)
```

**새 확장 만들기**(외부 개발자 순서):
1. `rustup target add wasm32-unknown-unknown` · 이 폴더를 복사하거나 `samples/hello-ext`를 복제해 `Cargo.toml`의 이름·버전을 바꾼다(`crate-type = ["cdylib"]` · `nexa-ext-sdk` 의존).
2. `impl Extension for MyExt { meta · on_settings · run }` + `nexa_ext_sdk::export_extension!(MyExt);`.
3. `cargo build --release` → `target/wasm32-unknown-unknown/release/<crate>.wasm`(호스트 테스트는 `cargo test --target <host triple>` — SDK의 호스트 목 import).
4. 패키지 폴더 `<id>/`에 `.wasm` + `extension.json`(`kind: "wasm"` · `files[] = {path, sha256}` · `settings_prefix`) · 저장소 `index.json`에 한 줄.
5. 앱: Extension Manager ▸ Add Repository(폴더 또는 URL) → Install → 즉시 로드(재시작 없음).

**우리 저장소 빌드/배치**: `scripts/ext-build.ps1` / `scripts/ext-build.sh`(`cargo build --release` → `extensions/<pkg>/<file>.wasm` 복사 → sha256 계산 → `extension.json` 갱신).

## 5. Rainbow Pairs를 SDK 위에 — 소스 · 산출물 · 업로드

- 소스: [`extensions/sdk/samples/rainbow-pairs/src/lib.rs`](../extensions/sdk/samples/rainbow-pairs/src/lib.rs) — 앱의 [`rainbow_pairs.rs`](../crates/nexa-sql/src/extensions/rainbow_pairs.rs)와 1:1(메타 6명령 + 메뉴 · `on_settings` = `rainbowpair.enabled/unmatched/colors/max_kb` → 색 층 · `disabled` = 색 없음 · `run` = `Editor` op). 차이 하나 = 사용자 색 목록의 `contrast_order` 재배열은 앱(내장)만 한다(WASM 판은 적은 순서 · 필요하면 SDK에 옮긴다).
- 산출물: `extensions/rainbow-pairs/rainbow_pairs.wasm` **76,228 B**(hello 64,244 B — std `fmt`가 대부분 · `wasm-opt -Oz`로 20~30 % 더 줄일 수 있다 · 후속).
- 업로드(배포) 두 길:
  1. **저장소 폴더**(지금): `extensions/rainbow-pairs/extension.json`(kind `wasm` · 1.1.0 · sha256) + `index.json` → `main` 병합 = raw URL 배포. 앱이 소스 트리에서 돌면 그 폴더를 직접 읽는다(네트워크 0).
  2. **GitHub Releases 자산**: `files[].url`에 절대 URL(`https://github.com/SosomLab/nexa-sql/releases/download/ext-rainbow-pairs-1.1.0/rainbow_pairs.wasm`)을 적으면 매니저가 그 URL을 GET한다(`path`는 보관 이름) — 큰 파일·버전별 자산에 적합. 서명(D-89)은 후속.
- 내장판과의 관계: 같은 id `rainbow-pairs` → WASM이 로드되면 레지스트리가 **내장을 가린다**(`Registry::active_indices`) · 로드 실패(손상·ABI 불일치·연료 초과)면 로그에 남기고 내장이 그대로 산다(폴백) · 삭제하면 내장 복귀(설치 기록이 없으면 내장도 꺼진 상태 = 종전 규칙).

## 6. 내려받아 적용하는 흐름(런타임 상세)

```
Install(팔레트/패널/뷰 탭)
  → manager::install_traced: extension.json GET → kind 검사(wasm = .wasm 파일 필수) → 파일마다 GET → sha256 비교(불일치 = 거부)
    → <설정>/extensions/<id>/<ver>/<file> 보관(+ extension.json 사본) → installed.json 기록
  → App::apply_extensions
    → installed() 중 kind = wasm → wasm_module_path(root,id,ver) → Registry::sync_wasm(want)
      · 새 것: WasmExtension::load_file — 8 MB 상한 · wasmi Module::new(검증·컴파일) · nx_ext_meta 1회(abi == 1 · id == 설치 id) · 명령/메뉴 표
      · 사라진 것: 레지스트리에서 내림 · 같은 것: 그대로(경로·id 비교)
    → on_settings(접두 키 JSON) → 효과 → 편집기 전 탭 · menu_extras · 팔레트/키맵은 id 문자열로 연결
Run(명령) → Registry::run → owner(WASM 우선) → nx_ext_run → op 큐 → EditorOps 적용
```

- 로그(개발자 모드 `ext` 층 + 일반 로그): `wasm extension loaded: rainbow-pairs Rainbow Pairs (6 commands · N ms · <경로>)` · 실패 = `load failed — <이유> (builtin fallback if any)` · 게스트 `nx_log` 줄 = `[id] …`.
- 실패 격리: 트랩·연료·시간 초과 = 그 호출만 실패(효과 = 기본값 · 명령 = 미처리) · 연속 3회 = 정지(`notes`에 남김) · 다시 설치/앱 재시작으로 초기화.
- 네트워크 규칙(26 §8): 설치 때만 GET(파일마다 1회) · 자동 갱신 없음 · 실패 재시도 없음 — 종전 매니저 그대로.

## 7. 호스트 쪽 변경(코드 지도)

| 파일 | 변경 |
|---|---|
| `extensions/mod.rs` | `Label { Msg · Text(en, ko) }` · `Command.id: String` · 트레이트 `id/name/settings_prefix -> &str` · `is_wasm`/`as_wasm` · `Registry::sync_wasm/ids/wasm_ids/active_indices/take_wasm_notes` · `wasm_module_path` · 시험 `install_load_replace_and_unload_wasm_package` |
| `extensions/wasm.rs`(신설) | `WasmExtension`(load · call_buf/call · 연료/메모리/시간/브레이커 · op 큐) · `effect_from_json` · `settings_json` · 시험 2(실제 `.wasm` 로드) |
| `extensions/manager.rs` | `kind = wasm` 설치 허용(`.wasm` 파일 필수 · `process`는 여전히 거부) · `installed_meta_in` · `files[].url` |
| `main.rs` | `apply_extensions` 첫머리에서 설치 목록 → `sync_wasm` → 로그 · `ids()` |
| `Cargo.toml` | `wasmi = "1.1"`(DR-3 원장 · 10 §3) · `exclude = ["extensions/sdk"]` |
| `nsql-settings` | `Settings::from_text` 공개(시험) |

## 8. 실측(Windows · 09-23)

| 항목 | 값 |
|---|---|
| `.wasm` 크기 | rainbow-pairs 76,228 B · hello 64,244 B |
| 게스트 빌드 | 2.3 s(증분 · SDK 워크스페이스 · opt "z" + lto) |
| 로드(검증·컴파일·메타 1회) | 시험에서 ms 단위(로그 줄에 실측 기록 · §6) |
| 앱 크기 증가(wasmi + 런타임) | GUI 10.41 MiB(10,915,840 · 96차) → **12.08 MiB(12,663,296)** = **+1.67 MiB**(예상 0.6 MB의 약 3배 — wasmi 1.1 인터프리터 + 검증기 + `wasmi_ir` · LTO fat에서도 남는 몫 · 예산 30 MB 안 · 26 §7 인벤토리 다음 측정에 반영) · 이어 IntelliSense/아웃라인(76) +163 KB → 12.24 MiB(12,830,208) |
| 호출 | 인스턴스 생성 + JSON ≈ 0.1~0.3 ms(정책 층) — 페인트 핫 패스에는 부르지 않는다(50 §11 P-1) |

## 9. 남은 것(후속 · T-118 갱신)

1. **설정 스키마 동적 등록** — 지금 `rainbowpair.*`는 앱 레지스트리에 있어 되지만, 새 확장의 `foo.*` 키는 설정 창에 안 보인다(값은 `settings.conf`에 쓰면 게스트가 받는다). 메타 `settings:[{key,kind,default,label,desc}]` → 런타임 레지스트리 층 + 설정 창 분류.
2. **동적 명령 라벨·키맵** — 앱 키맵 표(`COMMANDS`)에 없는 새 id는 팔레트에 라벨이 없다(우클릭 메뉴는 메타 라벨로 보임). 팔레트/키맵을 레지스트리 조회로.
3. **개발 모드** — 설정 `extensions.dev_dir` 폴더의 `.wasm`을 매니저 없이 로드 · 파일 변경 감시 핫 리로드 · `nsql ext check <dir>`(메타·sha256·ABI 검사) · `nsql ext pack`.
4. **서명(D-89) · 능력 승인(D-90)** — 인덱스 서명 · 매니페스트 `capabilities`(지금 표면은 편집기 이동뿐이라 승인 대상이 없다).
5. **API 확장(50 §12 규칙대로)** — 문서 읽기/데코레이션/결과 후처리는 필요해질 때 `EditorOps`+import 한 쌍씩 · 페인트 때 게스트를 부르지 않는다.
6. **프로세스 종류**(T-118 ④ · 격리 선택) · `wasm-opt` 크기 축소 · WIT/컴포넌트 모델은 수요 시.

## 10. 결정

| # | 결정 | 상태 |
|---|---|---|
| D-87 | 코드 확장 실행 방식 = **① WASM(`wasmi`)** | ✅ 구현(09-23 · 사용자 요구로 확정) |
| D-197 | WASM 확장이 내장 확장과 같은 id면 **WASM이 대체 · 내장은 폴백** | ✅ |
| D-198 | ABI v1 = 버퍼+JSON(WIT 없음) · 게스트 상태 없음 · 편집기 조작은 op 큐 | ✅ |
| D-199 | 배포 채널 = 저장소 폴더(기본) + Releases 자산(`files[].url`) · 둘 다 sha256 필수 | ✅ |
