# 93 · 코드 건강 점검 · 리팩터링 절차 — 미사용 정리 · 공통화 · 구조 (사용자 09-27)

> **요청**(사용자 09-27, 한 흐름으로 이어진 지시): *"커버리지 검사해서 실제 사용하지 않는 코드에 대한 정리 작업을 리팩토링과 병행 · 기능 공통화, 모듈화, 재사용성 및 코드 최적화 등 전체 소스"* · *"임시 테스트 목적으로 만들어진 코드, 화면, 파일 청소"* · *"성능 테스트, 기능 테스트, 회귀 테스트 등 목적이 있는 경우는 제외"* · *"코드의 가시성 향상과 코드 간 호출 구조도 디자인 패턴을 고려해서 효율적이고 직관적인 구조로"* · *"main 브랜치가 원복에 사용할 수 있도록 잘 관리"* · *"필요시 동일 기준으로 재실행 가능하도록 문서 및 도구 정리"* · *"좋은 체크리스트 도구가 있으면 차용하고 같이 문서에 정리"*.
>
> **이 문서의 자리**: [71 성능 점검](71-performance-review-process.md)이 *실행 성능*의 절차서라면, 이 문서는 *소스 자체*의 절차서다. [30 아키텍처 패턴 원장](30-architecture-patterns.md)은 확장점·부품 목록, [61](61-core-design-and-working-rules.md)은 불변식이다. "코드 정리해줘 / 리팩터링해줘 / 안 쓰는 코드 지워줘"가 오면 §2 순서를 그대로 돈다.

---

## 0. 원칙 여섯 줄

1. **원복 기준점 먼저** — 세 저장소(nexa-sql · nexa-ui · nexa-license)의 `main`에 같은 이름의 주석 태그를 단다(§1). 작업은 전부 브랜치에서.
2. **동작을 바꾸지 않는다** — 구조 이관·중복 제거는 행동 보존(behaviour-preserving)이 원칙. 매 단계 `clippy -D warnings` + 전체 시험 + 기능 점검(`win-func-check.ps1` 등)으로 확인.
3. **컴파일러가 1차 판정관** — "안 쓰는가"는 사람 눈이 아니라 허용 표시를 뗀 뒤의 `dead_code` 경고로 가른다. 컴파일러가 못 보는 것(참조 0 `pub` · 번역 문구 · 설정 키 · 의존)만 도구(§3)로.
4. **지움 · 시험 전용 · 유지의 세 갈래**(§4-1) — 이유 없는 `allow(dead_code)`는 남기지 않는다.
5. **목적 있는 시험 자산은 청소 대상이 아니다** — 성능·기능·회귀 시험 코드, 자체 시험 기동 명령(`grid.dump`·`ime.fake` …), `scripts/`의 탐침·E2E, 벤치는 남긴다(사용자 09-27).
6. **수치로 끝낸다** — `code-health.py`의 요약표를 전/후로 남기고(§6), 기준선 JSON을 저장해 다음 실행과 비교한다.

---

## 1. 원복 기준점(사용자 "main이 원복에 쓸 수 있게")

```bash
T=baseline/pre-refactor-YYYY-MM-DD
for r in nexa-sql nexa-ui nexa-license; do
  (cd ../$r && git tag -a "$T" main -m "원복 기준점: <무엇> 착수 직전 main(<해시> · CI 초록 · 시험 N)")
done
# 원복: git switch -c restore/<날짜> "$T"   (main을 강제로 되돌리지 않는다 — 새 브랜치에서 비교·선별)
```

- 태그는 **주석 태그**(`-a`) — 누가 · 왜 · 그때의 검증 수치를 메시지에 남긴다.
- 세 저장소가 path 의존으로 묶여 있으므로 **같은 이름**으로 단다(한쪽만 돌리면 빌드가 안 맞는다).
- push는 사용자 요청 때 `git push origin <태그>`(브랜치와 별도).
- 이번 기준점: `baseline/pre-refactor-2026-09-27` = nexa-sql 54d7fcd · nexa-ui 3bd0f4e · nexa-license 4f02524.

---

## 2. 순서(매번 이대로)

| 단계 | 할 일 | 도구 | 끝 조건 |
|---|---|---|---|
| **P0** | 원복 태그 · 작업 브랜치 | git | 세 저장소 태그 |
| **P1** | 기준선 측정 | `python scripts/code-health.py --save-baseline` · `cargo llvm-cov --workspace --summary-only` | 보고서·기준선 JSON |
| **P2** | 허용 표시 걷어내기 → 컴파일러 판정 | 무조건 `#[allow(dead_code)]`을 떼고 `cargo check --workspace` / `--tests` | 경고 0 |
| **P3** | 컴파일러 밖 미사용 | 도구 §3 B~E | 후보 전부 판정(§4-1) |
| **P4** | 임시 코드·화면·파일 | `임시`·`추후 제거`·`HACK`·`FIXME`·`dev.` 설정 grep · 추적 파일 목록 | 목적 있는 시험 자산 제외 |
| **P5** | 구조: 거대 객체 분할 | `scripts/split-app-impl.py`(한 번) → `narrow-app-visibility.py`(멱등) | 3000줄 넘는 파일 감소 |
| **P6** | 중복 → 공용 부품 | 도구 §3 G 목록 위에서부터 | 같은 모양 3곳 이상 = 부품 |
| **P7** | 검증 | fmt · `clippy --all-targets -D warnings` · 전체 시험 · 기능 점검(Release) | 전부 초록 |
| **P8** | 기록 | 이 문서 §6 표 · journal · STATUS · 예외 표(§3) | 커밋 · main `--ff-only` |

---

## 3. 도구 — `scripts/code-health.py`(재실행용 · 외부 패키지 0)

```bash
python scripts/code-health.py                  # nexa-sql + ../nexa-ui + ../nexa-license
python scripts/code-health.py --cov            # + cargo llvm-cov 요약(수 분)
python scripts/code-health.py --save-baseline  # 이번 결과 = 기준선
python scripts/code-health.py --baseline target/code-health/baseline.json   # 표에 기준선 열
# 산출: target/code-health/report.md · result.json (추적 안 됨)
```

| 기호 | 점검 | 판정 방식 | 흔한 오탐과 처리 |
|---|---|---|---|
| A | 무조건 `allow(dead_code/unused)` | 줄 grep(`cfg_attr` 조건부는 따로 셈 — OS별 코드는 정상) | 이유가 있으면 조건부(`cfg_attr`)로 좁힌다 |
| B | 참조 0인 `pub` 항목 | 세 저장소 전체 낱말 빈도 1 | 공용 라이브러리 API(빌더 `with_*` · getter/setter 짝 · 토큰 · SDK · 프로토콜 상수) → `ALLOW_PUB`에 이유와 함께 |
| C | 안 쓰는 `Msg` | i18n 밖 등장 + i18n 안에서 **번역 표 줄·`Msg::ALL`·시험 밖** 참조 | 없음(표·목록에만 있으면 화면·CLI에 안 나온다) |
| D | 안 쓰는 설정 키 | `"키"` 문자열 참조 없음 · 또는 **nsql-settings 안에서만** 등장 | 조립 키(`key.` + 명령 id) → `ALLOW_SETTING_PREFIX` |
| E | 안 쓰는 의존 | 크레이트 소스에 의존 이름(`-`→`_`)이 없음 | feature 활성·링크 전용 → `ALLOW_DEP` |
| F | 크기 | 3000줄 넘는 파일 · 150줄 넘는 함수 수 · 상위 목록 | 번역 표(`row`)·생성 표는 예외로 읽는다 |
| G | 중복 블록 | 주석·빈 줄·괄호 줄을 뺀 10줄 창 해시(350자 이상) | 시험 코드 제외 · 같은 파일 안 반복도 본다 |
| H | 커버리지 | `cargo llvm-cov` 크레이트별 줄 % | GUI 크레이트는 낮은 것이 정상(화면은 기능 점검이 덮는다) |

보조 스크립트(구조 이관 · §5):

- `scripts/split-app-impl.py` — main.rs의 `impl App`을 메서드 단위로 `app/<기능>.rs`에 배정(이름 규칙표 = 파일 머리의 `MODULES`). **1회성**(멱등 아님) — 이후 새 메서드는 사람이 해당 기능 파일에 둔다.
- `scripts/narrow-app-visibility.py` — `app/*.rs`의 `pub(crate) fn` 중 자기 파일 밖에서 안 부르는 것을 비공개로(멱등 · 언제든 다시 돌려도 됨).

---

## 4. 판정 규칙

### 4-1. 지움 · 시험 전용 · 유지

| 상황 | 처리 |
|---|---|
| 운영 경로·시험 어디에서도 안 부름 | **지운다**(문서·주석의 언급도 함께) |
| 시험에서만 부름 | `#[cfg(test)]`로 옮긴다(운영 바이너리에서 빠짐 · 시험은 유지) |
| OS별 경로에서만 부름 | `#[cfg_attr(not(<그 OS>), allow(dead_code))]` + 한 줄 이유([61 §3](61-core-design-and-working-rules.md)) |
| 공용 라이브러리의 API 짝 | 유지 + 도구 예외 표에 이유 |
| 설정 창에 보이는데 앱이 안 읽는 설정 | **지운다**(성능 프리셋 표 · 비노출 목록 · `Msg` 라벨/설명까지) — 기능이 생기면 그때 다시 등록 |
| "임시 · 추후 제거"라고 적힌 기능 | 지운다 — 단 **목적 있는 시험 자산은 제외**(§0-5) |

### 4-2. 공통화 기준

- **같은 모양 3곳 이상 = 부품**([30 §1](30-architecture-patterns.md) "두 번째 만나면 부품")을 기본으로, 이번처럼 이미 5~11벌이면 즉시.
- 부품의 집: 여러 앱이 쓸 만하면 **nexa-ui**(예: `FontPrefs::with_base`), 이 앱의 창 공통이면 nexa-sql 공용 모듈(예: `input::text_key_event` · `Presenter::frame`).
- 판정 본체는 **순수 함수**로 떼어 시험한다(winit 사건처럼 만들기 어려운 입력은 논리 값만 받는 `*_of` 판본 — `shortcut_letter_of`와 같은 모양).

### 4-3. 구조(디자인 패턴 관점)

| 문제(냄새) | 패턴·기법 | 이번 적용 |
|---|---|---|
| 거대 객체(God Object · 2만 줄 파일) | **기능별 모듈 분할**(Extract Module) — 상태는 한 곳(`App` 구조체), 동작은 기능 파일 | `app/` 26 파일 |
| 모든 메서드가 서로 보임 | **최소 가시성**(Encapsulate) — 파일 밖에서 안 부르면 비공개 | `narrow-app-visibility.py` |
| 창마다 복사한 입력 변환 | **Adapter** 한 벌 | `input::text_key_event` |
| 창마다 복사한 표면 준비 | **Template Method**의 공통 단계를 `Presenter`에 | `Presenter::frame` |
| 손으로 조립한 설정 값 | **Factory(연관 함수)** | `FontPrefs::with_base(_status)` · `SlotFont::plain` |
| 명령 문자열 분배(`menu_action` · `startup_cmd`) | **Command 분배표** — 지금은 `match` 한 곳(명령 id = 키맵·메뉴·팔레트·기동 명령 공용) | 유지(분배는 한 파일 · §7 후보) |
| 백엔드 선택(softbuffer/IOSurface) | **Strategy** | 기존 `Presenter` 그대로 |
| DB 접근 통제 | **Gatekeeper/Facade**(`gate_open`) | 기존 그대로(DR-34 불변식) |

---

## 5. 구조 지도 — `crates/nexa-sql/src/app/`(09-27 분할 뒤)

`App` 구조체(상태)는 main.rs에 그대로 있고, 동작은 기능 파일에 있다. **새 동작은 해당 기능 파일에 둔다.** 두 기능에 걸치면 호출하는 쪽(상위 흐름) 파일에.

| 파일 | 담당 | 대표 메서드 |
|---|---|---|
| `event_loop.rs` | winit 사건 처리기(`ApplicationHandler`) | `window_event` · `about_to_wait` |
| `input.rs` | 키·마우스 라우팅 · 포커스 · IME | `route` · `route_inner` · `set_focus` |
| `paint.rs` | 프레임 합성 | `paint` |
| `events.rs` | 백그라운드 사건 소화(워커·접속·가져오기) | `drain_events` · `drain_conn` |
| `menus.rs` | 풀다운·우클릭·팔레트 · 명령 분배 | `menu_action` · `key_command` · `build_menus` |
| `startup_cmd.rs` | 기동 명령(자체 시험·자동 점검 경로) | `startup_cmd` |
| `settings.rs` | 설정 키 → 화면 반영 · 테마·언어 | `apply_setting` · `apply_*` |
| `session.rs` | 세션 컨텍스트 · 접속/해제 · 문지기 | `gate_open` · `sync_gate` · `disconnect_*` |
| `run.rs` | SQL 실행·설명 계획·입력 응답 | `run_sql` · `run_explain` |
| `grid_results.rs` | 결과 탭·페치·그리드 편집 적용 | `new_result_tab` · `grid_edit_apply` |
| `tx.rs` | 트랜잭션 UX · 잠금 방지 | `tx_*` · `set_autocommit` |
| `files.rs` | 파일 열기/저장·인코딩·종료 흐름 | `open_file` · `save_to` · `request_exit` |
| `project.rs` | 프로젝트·작업 환경·다중 열기 | `project_*` · `multi_*` |
| `find.rs` | 찾기·바꾸기·파일 검색 | `find_*` · `start_search` |
| `completion.rs` | 코드 완성·시그니처·아웃라인 | `intel_*` · `outline_*` |
| `meta.rs` | 메타·객체 상세·탐색기 동작 | `meta_*` · `detail_actions` |
| `bookmarks.rs` | 북마크 | `bm_*` |
| `extensions.rs` | 확장 관리자·패널·명령 | `ext_*` |
| `toolbar.rs` | 툴바 그룹 도크·플로팅 | `build_tool_dock` · `dock_group` |
| `windows.rs` | 보조 창 열기·모달·창 목록 | `modal_open` · `all_windows` |
| `connwin.rs` | 접속 창·프로필 관리 | `open_conn_window` · `login_profile` |
| `vars.rs` | 변수(내장·DEFINE·변수 창) | `run_defines` · `vars_apply` |
| `license.rs` | 라이선스 게이트·창·배지 | `lic_gate` · `license_*` |
| `memory.rs` | 메모리 회수·메모리 창 | `mem_*` |
| `live.rs` | 실시간 재조회 | `live_*` |
| `demo.rs` | 데모 프로필·샘플 데이터 | `start_demo_create` |

main.rs에 남은 것: `App` 구조체와 생성 · `layout` · `redraw`/`tmark` · 자유 함수 · `fn main` · 시험 모듈.

---

## 6. 이번 실행 기록(09-27 · Windows 102차)

| 항목 | 착수 전(태그) | 뒤 |
|---|---|---|
| 무조건 `allow(dead_code)` | 16 | 0 |
| 참조 0 `pub`(세 저장소) | 51 | 45(전부 공용 API · 예외 표) |
| 안 쓰는 `Msg` | 62 | 0 |
| 효과 없는 설정 키 | 4 | 0 |
| 임시 기능 | `dev.start_demo` | 0 |
| main.rs 줄 수 | 20,721 | 2,759 |
| 3000줄 넘는 파일 | 10 | 9 |
| 중복 블록 묶음(10줄 창) | 35 | (§6 끝 수치 참조) |
| 시험 | 620 | 622(+ 키 변환 2) |
| 커버리지(nexa-sql 줄) | 46.5 % | (같은 시험 · 코드 감소분만큼 변동) |

세부 = [journal 2026-09-27 §23](journal/2026-09-27.md).

---

## 7. 차용한 체크리스트(원천 · 쓰는 자리)

| 원천 | 라이선스 | 무엇을 빌렸나 | 이 저장소에서 |
|---|---|---|---|
| **Rust API Guidelines — Checklist**(rust-lang) | MIT/Apache-2.0 | 이름 규칙(C-CASE · C-CONV `as_/to_/into_`) · 생성자(C-CTOR) · 공통 트레이트(C-COMMON-TRAITS) · `#[must_use]` | nexa-ui·nexa-license 공개 API 추가 때 §7-1 1~5 |
| **Clippy 린트 묶음**(rust-lang/rust-clippy) | MIT/Apache-2.0 | `clippy::all`(CI 게이트) + 가끔 `pedantic` 훑기(`cognitive_complexity` · `too_many_lines` · `needless_pass_by_value`) | P7 게이트 · 정기 훑기는 경고만 보고 선별 |
| **Refactoring 카탈로그 · 코드 냄새**(M. Fowler) | 개념 인용 | Long Method · Large Class · Duplicated Code · Shotgun Surgery · Feature Envy · Dead Code | §4-3 표의 "문제" 열 |
| **Google Engineering Practices — What to look for in a code review** | CC-BY 3.0 | 설계 · 기능 · 복잡도 · 시험 · 이름 · 주석 · 스타일 · 문서 8항 | §7-1 6~12 |
| **cargo-llvm-cov**(taiki-e) | MIT/Apache-2.0 | 줄·함수·영역 커버리지 | `--cov` · P1 |
| (참고) cargo-udeps · cargo-machete · cargo-deny | MIT/Apache-2.0 | 안 쓰는 의존 · 라이선스·중복 판본 감사 | 도구 E가 기본 대체 — 설치돼 있으면 교차 확인 |

### 7-1. 변경 체크리스트(리팩터링·정리 PR마다)

1. [ ] 새 공개 항목에 `#[must_use]`(값 반환) · 문서 주석 · 이름이 C-CONV를 따른다.
2. [ ] 생성은 연관 함수(`new`/`with_*`)로 · 기본값은 `Default`.
3. [ ] `pub`는 필요한 만큼만 — 크레이트 안이면 `pub(crate)`, 파일 안이면 비공개.
4. [ ] `allow(dead_code)`가 없다(OS별은 `cfg_attr` + 이유).
5. [ ] 같은 모양 3곳 이상이 남지 않았다(도구 G).
6. [ ] **설계** — 새 코드가 올바른 모듈(§5 지도)에 있다 · 상태는 `Sess`/`App` 규칙([CLAUDE.md §3](../CLAUDE.md))대로.
7. [ ] **기능** — 동작이 바뀌지 않았다(구조 이관) 또는 바뀐 동작이 시험으로 덮였다.
8. [ ] **복잡도** — 150줄 넘는 새 함수를 만들지 않았다 · 분기 2개 이상 판정은 순수 함수 + 시험(MC/DC · [52 §12](52-session-modes.md)).
9. [ ] **시험** — 지운 코드의 시험도 함께 정리 · 시험 전용은 `cfg(test)`.
10. [ ] **이름** — 기능 파일 이름이 담당을 말한다.
11. [ ] **주석** — "왜"를 남긴다 · 지운 기능을 가리키는 주석이 없다.
12. [ ] **문서** — 설정 키·`Msg`를 지웠으면 위키·설계 문서의 언급도 지웠다.
13. [ ] **불변식** — 편집기 불변식([61 §1](61-core-design-and-working-rules.md)) · 실행 통제(`gate_open`) · 데이터 보호(87 §14)를 건드리지 않았다.
14. [ ] **검증** — fmt · clippy `-D warnings` · 전체 시험 · 기능 점검(Release) · 도구 요약표 전/후.

---

## 8. 남은 후보(다음 실행)

- 긴 함수: `window_event` · `route_inner` · `worker::spawn` · `intel::request` · `main` · `paint` — 한 함수 = 한 사건 분기표가 되도록 사건별 도움 함수로(행동 보존 · 기능 점검 필수).
- 큰 파일: `explorer.rs` · `grid.rs` · `conn_win.rs` — `app/`와 같은 방식(상태 한 곳 · 동작 기능별)으로.
- 남은 중복(도구 G 상위): 보조 창 `paint` 머리(배경·글꼴·안내 줄) · 설정/키/파일 창 목록 그리기 · 로그/메모리/세션 창 머리.
- `menu_action`·`startup_cmd`의 문자열 `match`를 명령 표(Command 레지스트리)로 — 키맵·팔레트·메뉴가 같은 표를 쓰게(효과 = 명령 추가가 한 자리).
- 번역 표 `nsql-i18n::row`(3천 줄 `match`)는 생성 표(정적 배열)로 바꿀지 측정 뒤 판단(컴파일 시간·바이너리 크기).
