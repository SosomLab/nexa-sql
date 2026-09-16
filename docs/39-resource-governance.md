# 39 · 자원 거버넌스 — 기준(표준) · 거버너(설정 체계) · 상시 점검(게이트) (사용자 요청 09-16)

> **요구**(사용자 09-16 원문 요약): ① *"DB나 네트워크 성능, GPU나 CPU를 사용해서 성능을 저하시킬 수 있는 기능, UI 등은 모두 설정으로 제한시킬 수 있는 큰 설계"* ② *"이미지 아이콘, 네트워크 과도한 시도, DB 요청이나 결과 페치, 업무 처리 과정의 메모리 누수 등 광범위한 목표를 설계하고 지속적으로 점검하면서 최종 제품이 이를 준수하는 기준이 필요"*.
> **결론 한 줄**: 자원을 쓰는 기능은 **전부 "부하원(load source)"으로 등재**되고(§3 원장), 등재된 부하원은 **예외 없이 설정 키 하나 이상으로 끄거나 상한을 둘 수 있으며**(§4 거버너), 각 도메인은 **수치 기준**(§2 표준)을 갖고 **자동 점검**(§6 게이트)이 릴리스마다 그 준수를 증명한다. 세 조각은 [30 아키텍처 패턴](30-architecture-patterns.md)의 "포트 + 레지스트리 + 설정 선택"을 그대로 따른다.
> **상위 원장**: [26 §8 네트워크 부하](26-performance-architecture.md)는 이 문서 §3의 NET 열로 흡수된다(표는 그대로 두고 이 문서가 상위). [37](37-file-picker-performance.md)의 실측 절차는 §6의 도구가 된다.

---

## 0. 원칙 — 여섯 줄

1. **등재 없는 부하 없음.** 스레드를 만들거나, 소켓을 열거나, 디스크를 읽거나, 매 프레임 다시 그리거나, 캐시를 키우는 코드는 §3 원장에 한 줄 있어야 한다. 원장에 없으면 리뷰에서 막는다(30 §1-2 체크리스트 7번).
2. **모든 부하원은 설정으로 끄거나 상한을 둘 수 있다.** 키는 `nsql-settings::REGISTRY`에만(자주 안 바꾸면 `HIDDEN`) · 값은 즉시 반영(재시작 0) · `nsql config list perf`로 한 번에 본다.
3. **기본값 = 지금 동작 그대로.** 이 설계는 동작을 바꾸지 않는다. 바꾸는 것은 "제한할 수 있는 자리"를 만드는 일이다. 절전·원격 세션 자동 감지는 **안내만**(자동 전환은 D-58).
4. **수치 기준은 측정 가능해야 한다.** "빠르게"가 아니라 "유휴 사설 메모리 ≤ 12 MB · 5주기 뒤 핸들 증가 0 · 프로브 분당 ≤ N". 못 재는 기준은 기준이 아니다.
5. **점검은 사람 손을 타지 않는다.** 단위 테스트(회귀) · `#[ignore]` 계측 테스트(수치) · 스크립트(실기 자동화) · CI 게이트(3-OS) 4층. 사람은 결과 표만 본다.
6. **부하는 사용자 행동에 비례한다.** 사용자가 아무것도 안 하면 앱은 (열려 있는 접속 창의 신호등 말고는) 네트워크·CPU를 0에 가깝게 쓴다. 실행·조회는 사용자가 시작했을 때만.

---

## 1. 경쟁 제품 — 무엇을 어떻게 제한하나

| 제품 | 마스터 스위치 | 개별 상한 | 우리가 가져올 것 |
|---|---|---|---|
| IntelliJ | **Power Save Mode**(인스펙션·인덱싱·완성 끔 · 상태줄 아이콘) | `idea.max.intellisense.filesize` · 파일 크기별 강조 끔 | 한 번에 끄는 모드 + 상태줄 표시 + "왜 꺼졌는지" 안내 |
| VS Code | 없음(개별만) | `workbench.reduceMotion`(auto = OS) · `editor.largeFileOptimizations` · `files.watcherExclude` · `editor.maxTokenizationLineLength` · `search.followSymlinks` | OS reduce-motion 존중 · 큰 파일 최적화 · 토큰화 상한 |
| DBeaver | 없음 | `resultset.maxrows`(200) · `fetch size` · `Read metadata on connect` · `Keep-alive` · `Connect timeout` · 메타데이터 캐시 | 페치 상한·크기 · 접속 시 메타 읽기 끔 · keep-alive 주기 |
| Sublime | 없음 | `index_files` · `gpu_window` · `animation_enabled` · `hot_exit` | 인덱싱 끔 · 애니메이션 끔 |
| Chrome / OS | 절전 모드(배터리 20% 이하 자동) · macOS "동작 줄이기" · Windows "애니메이션 표시" | 프레임 상한 · 배경 탭 스로틀 | 배터리·원격 세션·접근성 신호를 **읽어서 안내** |

공통 결론: **개별 상한(수십 개) 위에 모드 하나**. 우리는 여기에 **원장(전수표)과 게이트(자동 점검)**를 더한다 — 경쟁 제품 어디에도 "이 제품이 만드는 부하의 전체 목록"은 없다.

---

## 2. 기준(표준) — 도메인별 수치 · 최종 제품이 지켜야 하는 것

도메인은 여섯: **DB · NET · CPU · GFX(화면) · UI(반응) · MEM**. GPU는 우리 앱이 직접 쓰지 않는다(CPU 래스터 + softbuffer) — GPU 부하는 곧 **"다시 그리는 횟수 × 영역"**이므로 GFX 도메인으로 다룬다.

| # | 도메인 | 기준 | 측정 | 지금(09-16 실측) |
|---|---|---|---|---|
| S-1 | MEM | 유휴 사설 메모리 ≤ **12 MB**(창 1 · 접속 0) · 접속 1 ≤ 16 MB | `scripts/memcycle.ps1` 시작 행 · 26 §7 | 8.4 MB ✅ |
| S-2 | MEM | **반복 작업 누수 0**: 같은 작업 N주기(≥ 5) 뒤 사설 메모리 증가 ≤ 100 KB · 핸들·GDI·USER·스레드 증가 0(일회성 초기화는 1주기 뒤 제외) | `memcycle.ps1`(대화상자) · §6-2 시나리오 표(접속/해제 · 실행 · 탐색기 펼침 · 탭 열고 닫기 · 파일 열기/저장 · 설정 창) | 대화상자 ✅ · 나머지 미측정 |
| S-3 | MEM | 캐시는 전부 **상한이 있고 문서에 수치가 있다**(글리프 8192 · OS 아이콘 512 · 결과 행 `grid.max_rows` · 로그 줄 · 실행 취소 깊이) · 무한 성장 컨테이너 0 | 코드 리뷰 + `cargo test` 상한 테스트 | 글리프·아이콘 ✅ · 로그·undo 상한 미정(T-90d) |
| S-4 | NET | 사용자 행동 없이 나가는 패킷은 [26 §8](26-performance-architecture.md) 표의 경로뿐 · 신호등 = 접속 창 열림 + 시도한 프로필만 · 실패 = 지수 백오프(상한 32분) · 동시 ≤ `probe.max_inflight` · **자동 재접속은 사용자 실행 시 1회** | `nsql --trace-net`(T-90e: 소켓 열기마다 로그 1줄) · 로그 창 `net` 필터 | 규칙 ✅ · 추적 로그 없음 |
| S-5 | NET | 실패 서버에 대한 재시도 횟수 ≤ `probe.max_retries` · 접속 테스트·접속 동시 ≤ `connect.max_concurrent` · 초과는 FIFO | 단위 테스트(`ProbePolicy`) | ✅ |
| S-6 | DB | 결과 페치는 `grid.max_rows`(200)까지만 · 페치 왕복 1,000행 ≤ 2 · 메타 조회는 **펼친 노드만**(사전 프리페치 0) · 실행 중 폴링(Oracle 라이브 로그)은 실행 중에만 · 접속 시 추가 왕복 ≤ 2(SID · 방언 확인) | `Timeline` 단계(Fetch 왕복 수 = `--timing`) · 워커 테스트 | 200 ✅ · 왕복 수 계측 미노출 |
| S-7 | DB | 응답 없는 서버가 UI를 잡지 않는다: 실행·접속·해제·메타 조회 어느 것도 UI 스레드에서 블록 0 · Disconnect는 즉시(09-16 11차) · 문장 타임아웃 설정 가능(`db.statement_timeout` · T-90b) | 워커 교체 테스트 · 실기(VPN 끊기) | Disconnect ✅ · 타임아웃 미구현 |
| S-8 | CPU | 유휴 CPU **0.0%**(애니메이션 없을 때 `WaitUntil` = 다음 깜빡임뿐) · 애니메이션 중 프레임 간격 ≥ 33 ms(≤ 30 fps) · 프레임 1회 ≤ 8 ms(26 §5) | `typeperf`/작업 관리자 60초 평균 · 37 §5 프레임 계측 | 유휴 0% ✅(WaitUntil) · 프레임 ✅ P-1~4 |
| S-9 | CPU | 큰 입력에 상한: 구문 강조는 `editor.highlight_max_kb`(1 MB) 초과 시 평문 · Ctrl+D 전체 검색 ≤ `editor.max_occurrences` · 그리드 정렬은 상한 행 안에서만 · 파일 검색은 `search.threads`·`search.max_file_kb`(36) | 단위 테스트(상한 경계) | 미구현(T-90c) |
| S-10 | CPU/디스크 | UI 스레드는 디스크·네트워크를 만지지 않는다(nexa-ui 20 §2-2) · OS 아이콘·열거·프로브는 워커 · 워커는 **사용 뒤 회수**(대화상자 닫힘 = 스레드 0 · 유휴 30초) | `resource_tests`(nexa-fs) · 스레드 수(`memcycle`) | ✅ |
| S-11 | GFX | 다시 그리기는 **변경이 있을 때만**(입력·이벤트·애니메이션 목표 변경) · 마우스 이동만으로 전체 창 재그리기 금지(hover는 `IntentFade` 70 ms 의도 판정 뒤) · 툴팁·페이드·깜빡임 각각 끌 수 있다 · **드래그 중 프레임 ≤ 2 ms**(1116×759 · 09-16 실측 1.3 ms) | `NSQL_TRACE_FRAMES=1`(✅ 09-16 · 60프레임 평균/최대 · 구간별) | 규칙 ✅ · 계측 ✅(라운드 사각형 SDF 수정 뒤 6→1.3 ms) |
| S-12 | GFX | 이미지·아이콘은 **크기별 사전 스케일 캐시**(매 프레임 리샘플 0) · 코드 도형은 마스크 1회 래스터 · OS 아이콘은 비동기 + 상한 512 | nexa-ui 테스트(글리프 캐시 재사용) | ✅ |
| S-13 | UI | 입력 → 화면 반영 ≤ 1 프레임 · 팝업/메뉴는 동기 · 긴 작업은 진행 표시("Loading…")와 **취소/해제 가능** | 실기 · S-7 | ✅ |
| S-14 | 전체 | 기동 ≤ 300 ms · 첫 창까지 네트워크 0 · 디스크 읽기 = 설정·프로필·폰트만 | `--timing` 기동 단계 | 미계측(T-90e) |

**준수의 정의**: 릴리스 후보는 §6 게이트를 전부 통과하고, 위 표의 "지금" 열이 "✅"이거나 **미달 항목이 TODO에 있어야** 한다. 새 기능은 이 표의 어느 행에 걸리는지 PR/journal에 적는다.

---

## 3. 부하원 원장 — 지금 코드의 전수표(등재 = 설정 키 = 모드별 값)

열 설명: **키** = 있는 것은 그대로, **(신설)** = T-90에서 추가. **모드**는 §4 `perf.mode` 프리셋 값(full / balanced / low). `—` = 모드가 건드리지 않음(개별 키만).

### 3-1. DB

| 부하원 | 지금 상한 | 키 | full | balanced | low | 근거 코드 |
|---|---|---|---|---|---|---|
| 결과 페치 행 수 | 200 | `grid.max_rows` | 200 | 200 | 100 | `Runner::with_max_rows` |
| 페치 배열 크기(왕복당 행) | 드라이버 기본 | `db.fetch_size`(신설 · HIDDEN) | 500 | 500 | 200 | 드라이버 어댑터 |
| 문장 타임아웃 | 없음(드라이버 무한) | `db.statement_timeout`(신설 · 초 · 0 = 없음) | 0 | 0 | 60 | 워커 `Cmd::Run` · 드라이버 cancel 포트 |
| 접속 시 추가 왕복(Oracle SID) | 1 | `oracle.live.source`(none이면 생략) | — | — | none | 워커 `ConnectSpec` |
| 실행 중 라이브 로그 폴링(Oracle) | `oracle.live.interval_ms` · 실행 중만 | `oracle.live.interval_ms` | 1000 | 2000 | 끔 | `live_tick` |
| 탐색기 메타 세션(접속마다 세션 1) | 1 | `explorer.visible`(숨김 = 세션 안 엶 · 신설 규칙) | 켬 | 켬 | 숨김 | `Explorer::connect` |
| 탐색기 자동 새로 고침 | `explorer.refresh_secs` | `explorer.auto_refresh` | 끔 | 끔 | 끔 | `Explorer` |
| 탐색기 노드 로드 | 펼친 노드만 | (구조 · 프리페치 0) | — | — | — | `Explorer::load` |
| 인텔리전스 메타(T-57 예정) | — | `intel.prefetch`(신설 · schema/none) | schema | schema | none | T-57 |
| 실행 전 빠른 판정 | 신호등 비초록일 때 SYN 1 | `probe.enabled` | — | — | — | `Cmd::Run.preflight` |
| 자동 재접속 | 실행 시 1회 | `connect.auto_reconnect` | 켬 | 켬 | 끔 | 워커 |

### 3-2. NET ([26 §8](26-performance-architecture.md) 표 흡수)

| 부하원 | 지금 상한 | 키 | full | balanced | low | 근거 코드 |
|---|---|---|---|---|---|---|
| 신호등 주기 확인 | 60 s · 창 열림 · 시도한 프로필 | `probe.enabled` `probe.interval` | 켬 60 | 켬 120 | 끔 | `ProbePolicy` |
| 실패 재확인 백오프 | 60 s × 2ⁿ · 5회 | `probe.retry_delay` `probe.max_retries` | — | — | — | `ProbeEntry` |
| 프로브 동시 스레드 | 16 | `probe.max_inflight` | 16 | 8 | 2 | `ProbeHub` |
| 프로브 타임아웃 | `probe.timeout` | `probe.timeout` | — | — | — | `probe_once` |
| ICMP(TCP 실패 뒤) | 1 | `probe.icmp`(신설 · HIDDEN) | 켬 | 켬 | 끔 | `probe::ping` |
| 접속 테스트·접속 동시 | 4 · FIFO | `connect.max_concurrent` | 4 | 2 | 1 | `dispatch_attempts` |
| DNS 재풀이 | 프로브마다 | `probe.dns_cache_secs`(신설 · HIDDEN) | 0 | 300 | 3600 | T-63 잔여 |
| 드라이버 확장 다운로드(T-27) | 사용자 클릭 | `net.updates`(신설 · manual/notify) | notify | manual | manual | T-27 |
| 라이선스 확인(T-32) | 설계 [23](23-license-activation.md) 주기 | `net.license_check`(신설 · 주기) | 설계값 | 설계값 | 최소 | T-32 |
| 다중 인스턴스 배가 | 인스턴스당 | (수용 · 26 §8 5항) | — | — | — | — |

### 3-3. CPU(+ 디스크)

| 부하원 | 지금 상한 | 키 | full | balanced | low | 근거 코드 |
|---|---|---|---|---|---|---|
| 구문 강조(탭 전체 재분석) | 없음 | `editor.highlight`(신설 · on/off) · `editor.highlight_max_kb`(신설) | on 1024 | on 512 | on 128 | `editors::make_box` |
| Ctrl+D 전체 선택 검색 | 없음 | `editor.max_occurrences`(신설 · HIDDEN) | 10000 | 5000 | 1000 | `select_next_occurrence` |
| 결과 그리드 정렬·복사 | `grid.max_rows` 안 | (상한 공유) | — | — | — | `grid.rs` |
| 파일 대화상자 열거 | 배치 256 · 워커 | `file.list_batch`(신설 · HIDDEN) | 256 | 256 | 512 | nexa-fs `lister` |
| 빈 폴더 셰브론 프로브 | 폴더마다 첫 일치 | `file.probe_chevrons`(신설) | 켬 | 켬 | 끔 | `ListOpts.skip_probe` |
| OS 아이콘 서비스 | 워커 1 · 캐시 512 · 12 ms/확장자 | `file.os_icons`(신설) | 켬 | 켬 | 끔(코드 도형) | nexa-fs `shell` |
| 파일 검색(T-81) | 설계 [36](36-find-in-files-and-project.md) | `search.threads` `search.max_file_kb` `search.gitignore`(신설) | CPU−1 · 1024 | CPU/2 · 512 | 1 · 256 | T-81 |
| settings.json 감시 폴링 | 1 s · 열어 둔 뒤만 | `settings.watch_ms`(신설 · HIDDEN) | 1000 | 2000 | 5000 | `json_tick` |
| 접속 테스트 스레드 | 요청당 1 | `connect.max_concurrent` | — | — | — | `spawn_test` |
| 워커 패닉 격리 | `catch_unwind` | (구조) | — | — | — | 워커 |
| 로그 창 줄 수 | 없음 | `log.max_lines`(신설) | 10000 | 5000 | 1000 | `log_win` |
| 실행 취소 깊이 | 없음 | `editor.undo_max`(신설 · HIDDEN) | 1000 | 500 | 100 | nexa-ctl `EditState` |

### 3-4. GFX(화면 · "GPU")

| 부하원 | 지금 상한 | 키 | full | balanced | low | 근거 코드 |
|---|---|---|---|---|---|---|
| 애니메이션 프레임 간격 | 33 ms 고정 | `ui.max_fps`(신설 · 60/30/15) | 60 | 30 | 15 | `about_to_wait` `from_millis(33)` |
| hover 페이드(행·버튼·콤보) | `ui.fade_fast/slow` · 의도 70 ms | `ui.animations`(신설 · auto/on/off · auto = OS 동작 줄이기) | auto | auto | off | `IntentFade` |
| 스크롤바 페이드·숨김 | `HIDE_MS` | `ui.animations` | — | — | off | nexa-ctl `scroll` |
| 캐럿 깜빡임 | 500 ms | `editor.caret_blink`(신설) | 켬 | 켬 | 끔 | `next_blink` |
| 로딩 점 애니메이션 | 300 ms | `ui.animations` | — | — | off(고정 "Loading…") | `explorer` · `FilePicker` |
| 툴팁 | `ui.tooltip_delay_ms` · `tabs.tooltip` · `explorer.tooltip` | 기존 | — | — | — | — |
| 슬라이드(패널) | `ui.slide_ms` | `ui.animations` | — | — | 0 | `conn_win` |
| 다시 그리기 단위 | 창 전체 | (구조 · 더티 영역 = T-90f 후보) | — | — | — | `paint` |
| 글리프 서브픽셀(1/3 px) | 켬 | `ui.text_subpixel`(신설 · HIDDEN) | 켬 | 켬 | 끔(캐시 1/3) | nexa-gfx `GlyphKey.sub` |
| 아이콘 사전 스케일 캐시 | 크기별 | (구조) | — | — | — | `IconImage::resized` |
| HiDPI 배율 | OS | (구조) | — | — | — | `set_scale` |
| 반투명 선택(D-54) | 미정 | `editor.selection_alpha`(D-54) | — | — | 불투명 | D-54 |

### 3-5. UI(반응·입력)

| 부하원 | 지금 | 키 | 비고 |
|---|---|---|---|
| hover 의도 판정 | 70 ms | `ui.hover_intent_ms` | 마우스 이동은 목표 덮어쓰기만(큐 0) |
| 더블클릭·툴팁 지연 | `ui.dblclick_ms` `ui.tooltip_delay_ms` | 기존 | — |
| 긴 작업 표시·해제 | Loading… · 즉시 Disconnect | (S-13) | 09-16 11차 |
| IME | OS | — | — |

### 3-6. MEM(캐시 상한)

| 캐시 | 상한 | 키 | 회수 시점 |
|---|---|---|---|
| 글리프 비트맵(nexa-gfx) | 8192 | `ui.glyph_cache`(신설 · HIDDEN) | 초과 시 전체 비움 |
| OS 아이콘 RGBA(nexa-fs) | 512 | `file.icon_cache`(신설 · HIDDEN) | LRU · 프로세스 종료 |
| 파일 대화상자 목록·프로브·아이콘 사본 | 폴더 1개 분 | — | 대화상자 닫힘(Drop) |
| 결과 셋 | **편집기 탭당 그리드 1**(13차 · `grid_stash`) · 각 ≤ `grid.max_rows` | `grid.max_rows` | 다음 실행 · 탭 닫힘(즉시) |
| 로그 창 | (신설 상한) | `log.max_lines` | 초과 시 앞에서 버림 |
| 실행 취소 | (신설 상한) | `editor.undo_max` | 탭 닫힘 |
| 최근 파일·폴더 | `file.recent` 개수 | 기존(10) | — |
| 탐색기 노드 | 펼친 것만 | — | 접기/해제 |
| 폰트 mmap | 파일 기반(공유 페이지) | — | 26 §7 착시 주의 |

---

> **부하원 추가(09-16 33차)**: 상태줄 git 조회 스레드 `nsql-git`(git CLI 2회 · 폴더 바뀜/저장/15초 · 동시 1) — 끄는 키 `statusbar.git`.
> **부하원 추가(09-16 29차)**: 텍스트 보기 변환 스레드 `nsql-textview`(변환 중에만 · 500행 블록 · 취소 깃발 · 결과 복제 1회) — 끄는 키 없음(보기를 그리드로 두면 0) · 자세히 [journal 29차](journal/2026-09-16.md).

## 4. 거버너 — 설정 체계 설계(포트 + 레지스트리 + 설정 선택)

### 4-1. 세 층

```
perf.mode  (auto | full | balanced | low | custom)      ← 마스터 1개 · 상태줄 ⚡ · CLI --perf
   └ 도메인 프리셋(§3 표의 full/balanced/low 열)          ← 레지스트리가 안다(Entry.perf)
        └ 개별 키(§3 표의 키)                            ← 사용자가 직접 바꾸면 그 키만 우선 · 모드 = custom 표시
```

- **`auto`**(기본 후보 · D-58): OS 신호로 고른다 — 배터리 전원 → balanced · 원격 세션(RDP/VNC) → balanced · OS "동작 줄이기" → `ui.animations=off`만 · 그 외 full. **09-16 결정 전까지 기본은 `full`(= 지금 동작 그대로)**.
- **우선순위**: 사용자가 명시한 개별 값 > 모드 프리셋 > 레지스트리 기본. 개별 값을 지우면(설정 창 "기본값으로") 다시 모드를 따른다. 모드를 바꾸면 **개별 값은 남는다**(사용자 의도 보존 · 상태줄은 `custom`으로 알린다).

### 4-2. 레지스트리 — `Entry.perf`

```rust
// nsql-settings
pub struct PerfBinding { pub domain: Domain, pub full: &'static str, pub balanced: &'static str, pub low: &'static str }
pub enum Domain { Db, Net, Cpu, Gfx, Ui, Mem }
pub struct Entry { /* 기존 */ pub perf: Option<PerfBinding> }   // Some = 이 키가 부하원이다(= §3 등재)
```

- `perf: Some(..)`인 키가 **곧 원장**이다 — `nsql config list perf`는 이 키들만 도메인별로 찍는다(§3 표를 손으로 유지하지 않는다 · 문서 표는 "왜"만).
- `Settings::effective(key)` = 개별 값 있으면 그것 · 없으면 `perf.mode`가 custom/full/balanced/low에 따라 `PerfBinding`의 값 · auto면 §4-4 신호로 고른 모드의 값.
- 새 카테고리 `Performance`(Msg::CatPerformance · 사이드바) — 모드 라디오 + 도메인 6그룹(접힘). **기존 키는 카테고리를 옮기지 않는다**(예: `probe.*`는 Connection에 그대로 · Performance 카드에는 링크 행으로 "이 도메인의 키 n개 보기").

### 4-3. 포트 — `nsql_core::Budget`(읽기 전용 스냅샷)

```rust
// nsql-core (의존 0)
#[derive(Clone)] pub struct Budget { pub max_rows: usize, pub fetch_size: usize, pub statement_timeout: Option<Duration>,
    pub probe_inflight: usize, pub connect_concurrent: usize, pub highlight_max_bytes: usize, pub max_occurrences: usize,
    pub max_fps: u32, pub animations: bool, pub caret_blink: bool, pub log_max_lines: usize, pub undo_max: usize, /* … */ }
pub struct BudgetCell(Arc<RwLock<Arc<Budget>>>);   // 워커·스레드가 쥔다 · snapshot()은 Arc clone 1회
```

- UI는 설정이 바뀔 때 `Budget`을 다시 만들어 `BudgetCell`에 **교체**(즉시 반영 · 잠금 짧음). 워커는 **작업 단위마다** `snapshot()`을 읽는다(실행 시작 · 배치 · 프레임). 외부 crate 0.
- nexa-ui 쪽 상한(글리프 캐시 · 페이드 ms · 프레임 간격)은 이미 있는 **전역 세터**(`tokens::set_fade_ms` 류)로 흘린다 — nexa-ui는 설정을 모른다(호스트 몫 · 30 §1).

### 4-4. OS 신호(auto · 안내) — OS 차이는 `nexa-fs`에만(docs/20 규칙)

| 신호 | Windows | macOS | Linux |
|---|---|---|---|
| 배터리 전원 | `GetSystemPowerStatus` | `IOPSCopyPowerSourcesInfo`(objc2 = winit 판) | `/sys/class/power_supply/*/status` |
| 원격 세션 | `GetSystemMetrics(SM_REMOTESESSION)` | 화면 공유 감지 불가 → 없음 | `SSH_CONNECTION`/`WAYLAND_DISPLAY` 부재 |
| 동작 줄이기 | `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)` | `NSWorkspace.accessibilityDisplayShouldReduceMotion` | GNOME `gtk-enable-animations`(gsettings 읽기 · 실패 = 없음) |
| CPU 코어 수 | `available_parallelism` | 동일 | 동일 |

모듈 = `nexa-fs::sys`(파일 계층이 아니라 이름이 어긋난다 · **D-60**: `nexa-sys` 크레이트로 분리할지). 신호는 **기동 때 1회 + 60초마다**(전원 이벤트 구독은 하지 않는다 · 부하원 등재 · CPU 0에 가까움).

### 4-5. 표시·진단(사용자가 "왜 느린가/왜 꺼졌나"를 본다)

- **상태줄 ⚡ 세그먼트**: 모드 이름(`full`이면 숨김 · `custom`은 노란 점) · 클릭 = 모드 팝업(✓ 현재 · Tab Size 팝업과 같은 부품).
- **로그 창 `perf` 종류**: 상한이 실제로 작동한 순간 1줄 — "구문 강조 생략: 2.3 MB > 1 MB(`editor.highlight_max_kb`)" · "페치 200행에서 중단(`grid.max_rows`)" · "프로브 대기: 동시 상한 8". 설정 키를 함께 적어 **한 번에 고칠 수 있게**.
- **`nsql config list perf`** = 도메인별 표(키 · 현재 값 · 출처 = 개별/모드/기본) · `nsql config set perf.mode low` · `nsql --perf low`(1회성).
- **`--trace-net` · `--trace-frames`**(T-90e): 소켓 열기·프레임마다 1줄(stderr 또는 로그 창) — §6 자동 점검의 원천.

---

## 5. 시나리오 — 모드가 실제로 바꾸는 것(사용자 관점)

| 상황 | auto가 고르는 모드 | 체감 |
|---|---|---|
| 노트북 배터리 · 카페 Wi-Fi | balanced | 신호등 2분 간격 · 동시 프로브 8 · 30 fps · 강조 512 KB까지 |
| RDP로 사내 서버 접속 | balanced | 애니메이션은 OS 동작 줄이기 따라감 · 프레임 30 |
| 원격 DB가 응답 없음 | (모드 무관) | 실행은 `db.statement_timeout` · Disconnect 즉시 · 탐색기 Loading은 해제로 끊김 |
| 10만 행 조회 시도 | (모드 무관) | 200행에서 멈춤 + 상태줄 "상한" + 로그에 키 안내 |
| 2 MB SQL 스크립트 열기 | full | 1 MB 초과 → 평문 + 로그 안내 · 키 올리면 강조 |
| 저사양 VM(2코어 · 2 GB) | 사용자가 low | 15 fps · 애니메이션 0 · 캐럿 고정 · 프로브 끔 · OS 아이콘 끔 · 로그 1,000줄 |

---

## 6. 상시 점검 — 4층 게이트(사람 손 없이 준수를 증명)

| 층 | 무엇 | 언제 | 도구 |
|---|---|---|---|
| **G-1 회귀(단위)** | 상한 경계(페치 200 · 프로브 백오프 · 워커 교체 · 캐시 상한 · 강조 상한) | 매 커밋 `cargo test` | `probe::tests` · `worker::tests` · nexa-ui 캐시 테스트 · (신설) `budget::tests` |
| **G-2 계측(ignored)** | 수치 기준 S-2/S-8/S-10: 워커 5주기 핸들·GDI · 프레임 시간 · 열거 배치 시간 | 릴리스 전 · 부하원 코드 변경 시 | nexa-fs `resource_tests` · 37 §5 벤치 · (신설) `perf_tests` |
| **G-3 실기 자동화** | S-1/S-2 시나리오 표(아래 6-2) — 앱을 띄워 N주기 반복 · 표 출력 · 기준 비교 = exit code | 릴리스 전(Windows 스크립트 · mac/Linux는 T-90e) | `scripts/memcycle.ps1`(확장: 시나리오 인자) · `--trace-net` 카운트 |
| **G-4 CI** | G-1 3-OS · G-2는 Windows 러너에서 `--ignored` 별도 잡(수치는 아티팩트 표) | push · 태그 | `.github/workflows/ci.yml` + `perf.yml`(신설) |

### 6-1. 릴리스 게이트 체크리스트(태그 전에 표로 남긴다 · journal)

1. G-1 green 3-OS · G-2 표 첨부 · G-3 시나리오 6종 표 첨부(증가 0 확인).
2. §2 표의 "지금" 열 갱신 — 미달은 TODO 번호.
3. §3 원장에 이번 릴리스에서 추가된 부하원이 전부 있는가(`nsql config list perf`와 대조 — **차이 0**).
4. [26 §8](26-performance-architecture.md) 네트워크 표와 `--trace-net` 30분 유휴 실측(창 열림/닫힘 두 경우) 일치.
5. 메모리 기준선(26 §7) 재측정 · 착시(Working Set) 아닌 Private로.

### 6-2. 누수 시나리오 표(G-3 · 각 5주기 · 기준 = 2주기 이후 증가 0)

| # | 시나리오 | 조작(자동화) | 본다 |
|---|---|---|---|
| L-1 | 파일 대화상자 열기/닫기 | Ctrl+O → Esc | ✅ 09-15(12주기 평평) |
| L-2 | 접속/해제 | 접속 창 → Connect → Disconnect | 세션·탐색기 메타 스레드·SID · 워커 교체 뒤 옛 스레드 종료 |
| L-3 | 실행 반복 | F5 × N(200행) | 결과 셋 교체 · 그리드 캐시 · 타임라인 |
| L-4 | 탐색기 펼침/접기 | 스키마 3개 펼쳤다 접기 | 노드 벡터 · 아이콘 캐시 |
| L-5 | 탭 열고 닫기 | Ctrl+T → 입력 → Ctrl+W | `EditState` · undo · 강조 캐시 |
| L-6 | 설정 창·색 창·단축키 창 열고 닫기 | 메뉴 → Esc | 창 객체 · softbuffer · z_order |
| L-7 | 파일 열기/저장 | Ctrl+O 선택 → Ctrl+S | 인코딩·줄끝 버퍼 |
| L-8 | 로그 창 장시간 | 실행 100회 뒤 | `log.max_lines` 상한 작동 |

---

## 7. 구현 순서(T-90) · 결정 대기

| 단계 | 내용 | 크기 |
|---|---|---|
| **T-90a** | 레지스트리 `Entry.perf` + `perf.mode` + `Settings::effective` + `nsql config list perf` + 기존 키에 `PerfBinding` 부여(§3 "기존" 행) · 상태줄 ⚡ · 설정 창 Performance 카드(모드 + 링크 행) | 중 |
| **T-90b** | DB: `db.statement_timeout`(드라이버 cancel 포트 — Oracle `OCIBreak` · MSSQL attention · PG `pg_cancel_backend`/소켓 · SQLite `interrupt`) · `db.fetch_size` · 탐색기 숨김 = 세션 안 엶 | 중 |
| **T-90c** | CPU/GFX: `editor.highlight_max_kb` · `editor.max_occurrences` · `ui.max_fps` · `ui.animations`(OS 동작 줄이기 = nexa-fs::sys) · `editor.caret_blink` · `file.os_icons` · `file.probe_chevrons` | 중 |
| **T-90d** | MEM: `log.max_lines` · `editor.undo_max` · `ui.glyph_cache` · `file.icon_cache` 세터 + 상한 테스트 | 소 |
| **T-90e** | 진단·게이트: `--trace-net` · `--trace-frames` · 기동 `--timing` · `memcycle.ps1` 시나리오 인자(L-2~L-8) · `perf.yml` CI 잡 · 로그 창 `perf` 종류 | 중 |
| **T-90f** | (후보) 더티 영역 다시 그리기 · DNS 캐시 · auto 모드 자동 전환(D-58 뒤) | 대 |

**결정 대기(사용자)**
- **D-58** `perf.mode` 기본 = `auto`(배터리/원격 세션이면 balanced) vs `full`(지금 그대로 · 안내만). 권장 **`full` + 상태줄에 "배터리 전원 — balanced 권장" 1회 안내**.
- **D-59** 개별 키를 직접 바꿨을 때 모드 표시 = `custom`(권장) vs 모드 유지·값만 우선.
- **D-60** OS 신호 모듈 위치 — `nexa-fs::sys`(규칙 그대로) vs 새 `nexa-sys` 크레이트(이름이 맞음 · 권장).
- **D-61** `db.statement_timeout` 기본 — 0(없음 · DBeaver 동일 · 권장) vs 60초.

---

## 8. 원장 반영

- [30 §1-2](30-architecture-patterns.md) 체크리스트 **7번 추가**: *"자원(스레드·소켓·디스크·프레임·캐시)을 쓰면 [39 §3](39-resource-governance.md) 부하원으로 등재하고 설정 키(`Entry.perf`)를 붙였는가 · §2 기준 어느 행에 걸리는가."*
- [26 §8](26-performance-architecture.md) 표는 그대로 · 머리에 "상위 원장 = 39 §3-2" 한 줄.
- [24 설정 체계](24-settings-and-vscode-analysis.md): 카테고리에 Performance 추가(T-90a 때).
- CLAUDE.md §3 작업 규약에 한 줄: **부하원 등재 규칙**(네트워크 규칙과 나란히).
