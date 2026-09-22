# 30. 아키텍처 패턴 원장 — 확장점(IoC/주입) · 재사용 부품 · 관리 범위 (2026-09-15)

> **상태**: ✅ 원칙 확정(사용자 09-14~15) · 🚧 적용 로드맵(T-65~T-68). 상위 = [01 아키텍처](01-architecture.md)(4계층 · DR-7) · [22 드라이버 확장](22-driver-extensions.md)(DR-23·24) · [09 편집기·패키지](09-editor-and-packages.md)(DR-5·14). 이 문서는 **"어떤 기법을 어디에 쓰는가"의 원장**이다 — 새 기능은 여기 등재된 확장점·부품을 먼저 찾고, 없으면 여기에 추가한 뒤 만든다.

## 0. 사용자 요구(원문 요약 · 09-14~15)

1. 디자인 패턴의 효용을 최대화 — **일관성·효율성·관리성·재사용성**을 올리고, **관리되는 범위 안에서** 개발한다.
2. 플러그인·DB 드라이버처럼 뒤에 바뀌거나 버전이 갈리는 기능은 **IoC/주입(Injection)** 으로 **동적 배치** — 호환성·확장성·안정성.
3. 새 기법·기술·서비스가 나와도 **간단한 설정 + I/O 규약 준수**만으로 확장되게.
4. 구현 값은 하드코딩하지 말고 설정으로 — 자주 바꾸지 않는 값은 **비노출 설정**.
5. 네트워크를 만드는 코드는 상한·큐가 있고 원장([26 §8](26-performance-architecture.md))으로 관리.

## 1. 확장점 모델 — "포트 + 레지스트리 + 설정 선택"

모든 확장점은 같은 세 조각으로 만든다. 조각이 하나라도 빠지면 확장점이 아니라 하드코딩이다.

| 조각 | 뜻 | 규칙 |
|---|---|---|
| **포트(trait)** | 허브(`nsql-core`)가 정의하는 I/O 규약 · 구현체는 규약만 안다 | 포트는 허브에만 · 구현 crate는 포트만 의존(DR-7) · 규약 버전을 갖는다 |
| **레지스트리** | 구현체를 **이름/키로 찾는 표** · 부팅 시 또는 요청 시 조립 | 한 확장점에 레지스트리 하나 · 조회 실패는 명시 오류(패닉 금지) |
| **설정 선택** | 어떤 구현을 쓸지는 **설정/프로필/파일**이 고른다 · 코드는 고르지 않는다 | 키는 `nsql-settings::REGISTRY`에만 · 기본값은 설정에 · 비노출 가능 |

주입 방식은 Rust답게 **생성자 주입**(`Box<dyn Port>` · `Opener` 클로저)과 **레지스트리 조회**로 한다 — 전역 컨테이너·리플렉션은 쓰지 않는다. 동적 배치가 필요한 곳(드라이버·플러그인)은 **프로세스/WASM 경계**로 격리해 버전을 나란히 둔다(SxS · [22 §4-2](22-driver-extensions.md)).

### 1-1. 확장점 현황

| 확장점 | 포트(규약) | 레지스트리 | 설정 선택 | 동적 배치 | 상태 |
|---|---|---|---|---|---|
| **DB 드라이버** | `nsql_core::Session`·`Catalog` · `Opener` 클로저 | `nsql-drivers::open`(방언 → 어댑터 · feature) | 접속 문자열 `scheme://` · 프로필 `dialect` · `NSQL_ORACLE_CLIENT_DIR` | 내장 3종 · **RPC 드라이버**(stdio JSON-RPC 프로세스 · GitHub Releases · SxS) = T-27~30 | 내장 ✅ · RPC 📐 |
| **편집기 구문/패키지** | `.nexa-syntax`(Sublime 형식) · `Highlighter` trait | `syntax.rs` 레지스트리(확장자 → 규격 · `Packages/*/` 스캔·덮어쓰기) | `Set Syntax` 팔레트 · 확장자 기본 | 파일 드롭인(재시작 없이 스캔) · WASM 플러그인(DR-14) 📐 | 파일 ✅ · WASM 📐 |
| **로그 표현** | `nsql_log::LogFormat` trait | `nsql_log::formatter(name)` | `log.format` | 내장 3종 | ✅ |
| **접속 프로필 저장소** | `Vault`(봉투 형식 · 기기 키) | 단일(사용자 폴더) | `NSQL_HOME` | — | ✅ · 원격/팀 저장소는 [25](25-license-tiers-and-server.md) |
| **UI 컨트롤·토큰** | nexa-ctl `Control/Widget` trait · `tokens::*` 전역 | 컨트롤은 타입 · 토큰은 setter | `ui.*`(hover 색·페이드·스크롤바 지연) | 라이브러리(path 의존) | ✅ |
| **라이선스** | `nexa-license::check(Feature)` 순수 함수 | 기능 게이트 1곳/진입점 | 라이선스 파일 | 서명 알고리즘 포트(ed25519/p256 · D-40) | 📐 T-32 |
| **명령/단축키** | 팔레트 명령 id 문자열 · 메뉴 정의 | `open_palette` 목록 · 메뉴 트리 | 단축키 설정(T-53) | — | 명령 ✅ · 키맵 📐 |
| **설정** | `SettingKind` · `Entry`(키·범위·기본·i18n) | `nsql-settings::REGISTRY`(단일 원천) · `HIDDEN` | `settings.conf` · `nsql config` · 설정 화면(T-39) | — | ✅ |

### 1-2. 새 확장점을 만들 때의 체크리스트
1. 포트가 허브에 있고, 구현 crate가 허브만 의존하는가(역방향 의존 금지).
2. 레지스트리 하나 · 이름/키가 문서화됐는가 · 조회 실패 경로가 있는가.
3. 선택은 설정/프로필이 하는가 · 기본값이 레지스트리에 있는가 · 비노출이 맞는가.
4. 버전이 갈릴 수 있는가 → SxS 보관 · 규약 버전 필드 · 구버전 공존.
5. 실패가 앱 전체로 번지지 않는가 → 프로세스/WASM/스레드 경계 · `catch_unwind` · 오류 비확산([28](28-object-explorer.md)).
6. 네트워크를 만들면 [26 §8](26-performance-architecture.md) 표에 상한을 등재했는가.
7. 자원(스레드·소켓·디스크·프레임·캐시)을 쓰면 [39 §3](39-resource-governance.md) 부하원으로 등재하고 설정 키(`Entry.perf`)를 붙였는가 · 39 §2 기준 어느 행에 걸리는가.

## 2. 재사용 부품 원장 — 한 번 만든 기법은 여기 등재하고 다시 쓴다

| 부품 | 위치 | 문제 → 기법 | 쓰는 곳 |
|---|---|---|---|
| **TypeAhead + hangul::Composer** | nexa-ctl `typeahead` · `hangul` | 목록/트리 앞글자 점프(nexa-beep 이식): 버퍼+조합+타임아웃은 부품 · 매칭은 라벨 함수 · 필터·HUD 위치 설정 · Windows 한/영은 호스트가 토글해 `jamo_from_qwerty` | 오브젝트 탐색기(09-19) · (후보) 파일 대화상자 트리 · 설정 창 트리 · 팔레트 |
| **contrast_order** | nexa-ctl `theme` | 순환 팔레트를 이웃끼리 가장 잘 구별되게 배열(보색·색 온도·밝기 · 첫 색 고정) — 깊이 색·차트 계열색처럼 "차례로 쓰는 색 목록"에 재사용 · `Theme.rainbow`는 이미 이 순서 | Rainbow Pairs(`rainbowpair.contrast_order`) |
| **merge3 + replace_all_undoable** | nexa-ctl `merge3` · `TextBox` | 줄 단위 3-way 병합(의존 0 · 겹침 수·가져온 줄 범위) + 본문 전체를 되돌리기 한 단계로 바꾸기(캐럿 유지) — 외부 변경 반영 · 포매터 결과 적용 같은 "밖에서 온 새 본문"에 재사용 | 외부 파일 변경(58) |
| **StatWatch / FileSig** | nexa-fs `watch` | OS 와처 없는 파일 변경 감지(서명 비교 → 안정 대기 → 내용 해시) · 전용 스레드 · 요청 합침 — 언제 확인할지는 호스트가 정한다 | 외부 파일 변경(58) |
| **memtrim** | nexa-sql `memtrim.rs` | 힙 → OS 정리(3-OS) + 사용량 읽기 — "큰 것을 놓은 뒤 1회 + 유휴 주기" 정책은 호스트 · 워킹셋 트림은 하지 않는다 | 메모리 회수(59 §2) |
| **PreparedText + set_prepared** | nexa-ctl `TextBox` | 큰 본문을 **UI 스레드 밖에서** 준비(글자 버퍼 + 문자열 + 줄 표 · `Send`)해 복사 없이 옮겨 넣기 — 적재·재로드·생성된 큰 글 | 큰 파일 적재(`fileload.rs`) |
| **TextBuf**(UTF-8 갭 버퍼 + 줄 표 + 변경 기록) | nexa-ctl `edit::TextBuf` · `CharSeq` | 글자 인덱스로 말하는 큰 본문 버퍼 — 글자당 1~3 B · 편집 O(편집) · 줄 ↔ 글자 O(log) · `LineChange`로 줄별 캐시를 바뀐 줄만 갱신 · `find`/`eq_str`/`content_hash`는 본문을 문자열로 만들지 않는다 → [59 §6](59-large-file-handling.md) | 편집기 · (후보) 로그 창 본문 |
| **line_edits**(최소 줄 편집) | nexa-ctl `merge3` | 두 본문 → 바뀐 줄들만의 편집 목록(글자 좌표 · 오름차순) — 재로드·서식 정리를 "바뀐 줄만" 기록 | 외부 변경 반영 |
| **되돌리기 기록 파일**(`export_history`/`import_history` + `undofile`) | nexa-ctl `EditState` · nexa-sql `undofile.rs` | 저장할 때 쓰고 · 열 때 본문 길이·해시가 같으면 복원 · 어긋나면 통째로 버림 · 폴더는 인자로(테스트가 실제 설정을 안 건드린다) | 파일 탭 |
| **연산 기록 되돌리기**(`replace_many` · 묶음 · 저장 지점 · 예산 · 읽기 전용) | nexa-ctl `EditState` | 되돌리기 = 역연산 맞바꾸기(저장 = 지운 글자만) · 여러 곳 = 한 단계·한 번 훑기 · 더러움 O(1) · 바이트 예산 · 단일 변경 통로 → [60](60-undo-redo-redesign.md) | 찾아 바꾸기 · 외부 변경 반영 · 큰 파일 탭 |
| **fileload**(자리 탭 · 진행 막 · 취소) | nexa-sql `fileload.rs` + `Editors::begin_load_tab`/`fill_loaded` | "오래 걸리는 일을 **탭 하나에 가두고** 나머지는 그대로" — 진행 상태 = 원자값 · 지연 표시 · 100% 프레임 뒤 교체 · 명령 문지기는 순수 함수 | 큰 파일 열기 · (후보) 큰 결과 내보내기 미리보기 |
| **세대 + 줄 해시 캐시** | nexa-ctl `TextBox`(`MlTextCache`·`RowWidthCache`·`HlStateCache`) | 본문에서 파생되는 줄 단위 산출물은 (세대, 행 해시)로 묶어 **바뀐 행만** 다시 계산 — 새 줄 단위 기능(진단 표시 · 폴딩)을 넣을 때 같은 틀로 | 큰 파일 성능(59 §1) |
| **ellipsize_middle + show_full**(09-22) | nexa-ctl `draw` | 긴 경로·라벨의 가운데 `…` 축약(접두사 폭 표 · 앞 ≈ 뒤) + 전역 "전체 보기" 스위치(Alt 동안) — 풀다운·우클릭 메뉴·팔레트·검색 결과 공통 · 새로 경로를 보이는 곳은 이 부품을 쓴다 | journal 09-22 §38 |
| **IntentFade / HoverFade / FadeSpeed** | nexa-ctl `tokens` | 지나가는 대상의 hover 비용 0 · 마지막 의도만 · 속도 속성 2단 | 그리드 행 · 목록 행 · 콤보 항목 · 버튼 · 텍스트박스 |
| **hover/눌림 색 · 페이드 ms · 스크롤바 지연 전역 setter** | nexa-ctl `tokens` · `scroll` | "설정 한 번 = 전 컨트롤 즉시"(핫스왑) | `ui.*` 설정 |
| **ScrollBars(축별)** | nexa-ctl `scroll` | 오버레이 · 축 독립 · 상태 상수 크기 | 편집기 · 그리드 · 로그 · 목록 · 상태 메시지 |
| **ContextMenu(id만) · EditMenu** | nexa-ctl | 실행은 호스트 몫 · 컨트롤은 무엇을 할지 모른다 | 목록 · 콤보 · 입력란 |
| **TimeoutButton(경고 톤 · 두 줄 · 잔여 표시 옵션)** | nexa-ctl | 파괴적 2단 확인 · 시간 주입(결정적 테스트) | Delete |
| **ColorPanel** | nexa-ctl | HSV+A · 프리셋 · 최근 · 실시간 보고 | 색 설정 창 |
| **Control::set_enabled · clear_transient · own_focus 패턴** | nexa-ctl · `connect.rs` | 포커스 ≤1 · 가려진 컨트롤 일시 상태 비움 | 모든 창 |
| **ProbePolicy · ProbeHub(요청당 스레드 · 상한)** | `probe.rs` | 주기·지수 백오프·즉시 확인·병렬·격리 | 신호등 |
| **Attempt 큐(상한 + FIFO)** | `main.rs` | Test/Connect 동시 상한 · 순서 보장 | 접속 창 |
| **Runner + Opener + catch_unwind 워커** | `nsql-run` · `worker.rs` | 세션 소유 스레드 · 패닉 격리 · 빠른 판정 | 실행 |
| **owned_by(소유 창) · rehover · wheel_event** | `winfocus.rs` · `input.rs` | 작업표시줄 1 · 모달 닫힘 뒤 hover · 스크롤 방향 한 곳 | 3창 |
| **Timeline(Stage)** | `nsql-core` | 각 층이 자기 단계만 · 표시 시점 포맷 | `--timing` · 상태줄 |
| **ConnTuning + HIDDEN 설정** | `conn_win.rs` · `nsql-settings` | 구현 상수 → 설정(비노출) 주입 | 접속 창 |
| **RowSource 포트 · ResultData(Arc 세그먼트) · View(인덱스 투영)** | `nsql-core` · `nsql-io` | 결과 **한 세트** · 형식별 사본 0 · 렌더러 하나(`render_block`/`generate_src`) · NULL 글자 설정 1 | 그리드 · 텍스트 보기 7종 · 복사 · SQL 복사 · CLI(DR-33) |
| **ToolDock · ToolGroup · DockLayout · ToolItem::separator + ToolFloatWin** | nexa-ctl `tooldock` · `toolfloat.rs` | 목적별 그룹(아이콘·구분자 계층) · 그립 드래그 순서 · 떼어 내기 = 액션만(창은 호스트) · 배치 문자열 1키 · 툴바 소유 한 곳 | 상단 툴바(09-17) · 결과 도구줄 후보 |

규칙: 같은 문제를 두 번째 만나면 **부품으로 올린다**(nexa-ui면 공용). 창마다 복사한 코드는 이 표의 후보다.

## 3. 관리 범위 — 지금 코드에서 벗어나 있는 것(개선 로드맵)

| # | 증상 | 기법 | 작업 |
|---|---|---|---|
| A | 창 3개(접속·로그·색)가 softbuffer/RasterCtx/이벤트 변환(`to_input`)/틱을 **복사**해 갖고 있다(≈300줄×3) | **WindowHost** 부품(nexa-clip 창 골격 이식): 창 생성·표면·배율·입력 변환·틱·페인트 프레임을 한 타입으로, 창은 `layout/route/paint`만 구현 | **T-65** |
| B | `conn_win.rs` 2.5k줄 — 목록 모델(정렬·필터·열)·행 버튼·폼·프로브 표시·메뉴가 한 파일 | 목록을 **nexa-grid `ConnectionGrid`**(U-3 · dir2 rows 이식)로 · 프로브 표시는 `RowSource` 장식 · 창은 조립만 | T-31b·U-3 |
| C | 확장점 중 드라이버 RPC·WASM 플러그인이 설계만 있음 | [22](22-driver-extensions.md) T-27~30 · 규약 버전 필드부터 | T-27 |
| D | 접속 창 조정값은 설정화됐지만 편집기·그리드·로그 창 상수는 아직 코드 | `EditorTuning/GridTuning`으로 같은 방식(비노출) | **T-66** |
| E | 명령 id·단축키·메뉴가 문자열로 흩어져 있음 | **Command 레지스트리**(id · 라벨 Msg · 기본 키 · 핸들러) 하나 → 메뉴·팔레트·툴바·키맵이 같은 표를 읽는다 | **T-67**(T-53 선행) |
| F | 워커 `Cmd/ConnOutcome/RunEvent` 셋이 각각 자람 | 실행 계층 **메시지 규약 문서화**(버전 · 큐/스레드 소유권 표) — RPC 드라이버 규약과 맞춘다 | **T-68** |

우선순위: A → E → D(작은 것) → B(nexa-grid와 함께) → C.

## 4. 결정(관례 · 사용자 확인 불요)
- IoC = **생성자 주입 + 레지스트리** · 전역 컨테이너/리플렉션 없음(DR-3 외부 crate 0 · 단일 바이너리 DR-1과 조화).
- 동적 배치 경계 = **프로세스(stdio RPC)·WASM·스레드** 셋만 — 네이티브 dylib 플러그인은 쓰지 않는다(ABI·안정성).
- 규약에는 **버전 필드**를 둔다(드라이버 매니페스트 · `.nexa-syntax` · 라이선스 파일 · 설정 키 rename 표).
- 구현 상수는 **설정 레지스트리**로 · 자주 바꾸지 않으면 `HIDDEN`(`nsql config list all`).
