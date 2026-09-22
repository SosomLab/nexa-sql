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
| S-9 | CPU | 큰 입력에 상한(★ 규칙 09-22: 원장의 키는 **읽는 코드 위치**를 함께 적는다 — 키만 만들고 안 읽던 `highlight_max_kb`·`max_occurrences` 재발 방지 · 크기 상한의 원장 = [72](72-size-limits-and-large-file-constraints.md)): 구문 강조는 큰 파일 단계 L2(`file.large_syntax_level` · [72](72-size-limits-and-large-file-constraints.md))에서 평문(`editor.highlight_max_kb`는 09-22 폐기 — 미배선이었다) · Ctrl+D/Alt+F3 다중 선택 ≤ `editor.max_occurrences`(09-22 배선) · 그리드 정렬은 상한 행 안에서만 · 파일 검색은 `search.threads`·`search.max_file_kb`(36) | 단위 테스트(상한 경계) | 미구현(T-90c) |
| S-10 | CPU/디스크 | UI 스레드는 디스크·네트워크를 만지지 않는다(nexa-ui 20 §2-2) · OS 아이콘·열거·프로브는 워커 · 워커는 **사용 뒤 회수**(대화상자 닫힘 = 스레드 0 · 유휴 30초) | `resource_tests`(nexa-fs) · 스레드 수(`memcycle`) | ✅ |
| S-11 | GFX | 다시 그리기는 **변경이 있을 때만**(입력·이벤트·애니메이션 목표 변경) · 마우스 이동만으로 전체 창 재그리기 금지(hover는 `IntentFade` 70 ms 의도 판정 뒤) · 툴팁·페이드·깜빡임 각각 끌 수 있다 · **드래그 중 프레임 ≤ 2 ms**(1116×759 · 09-16 실측 1.3 ms) | `NSQL_TRACE_FRAMES=1`(✅ 09-16 · 60프레임 평균/최대 · 구간별 · ✅ 09-19 `input→present` 한 줄 = 입력→key→route→editor→request_redraw→RedrawRequested→paint→present 지점별 ms · `scripts/win-latency-probe.ps1`) | 규칙 ✅ · 계측 ✅(라운드 사각형 SDF 수정 뒤 6→1.3 ms) |
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
| 탐색기 메타 세션 **서버당 1**(DR-34 · 52 §2-2 · 그 서버에 붙은 세션 ≥1이면 유지 · 0이면 접속만 닫고 트리 유지) | 서버 수 · 유휴 회수 | `session.idle_secs`(메타 세션 유휴 닫기 · 다음 요청 때 재개) · `explorer.visible` | — | — | — | `ExplorerSet::sync_refs` · `suspend_if_idle` |
| 동작 직전 생존 판정(SYN 1 · 53) | 마지막 성공 뒤 60s | `probe.stale_secs`(0 = 신호등 조건만) · `probe.timeout` | — | — | — | `worker::ensure_alive` |
| TCP keepalive 빈 세그먼트(PG·MSSQL · 53) | 60s | `net.keepalive_secs`(0 = 끔) | — | — | — | 드라이버 `set_keepalive_secs` |
| 메모리 회수(힙 → OS · 보이지 않는 탭의 그리기 캐시 해제 · 59 §2) | 큰 것을 놓은 1초 뒤 1회 + 유휴 300s(수 ms · UI 스레드) | `mem.trim_on_release` · `mem.trim_secs`(0 = 끔) · `mem.release_results_on_disconnect` | 300 | 300 | 300 | `App::mem_tick` · `memtrim.rs` |
| 변수 표 보존 파일(`<설정 폴더>/vars/<경로 해시>.sql` · 실행이 그 탭의 변수를 바꿨을 때만 · 수백 바이트 · 63 V4) | 실행당 ≤ 1회 쓰기 · 파일 열 때 1회 읽기(≤ 4 MB) | `vars.persist`(off = 끔) · `vars.persist_days` | on | on | on | `varsfile.rs` |
| macOS 화면 내보내기 표면 풀(IOSurface ≤ 3장 × 창 픽셀 × 4 B + 폭이 16px 배수가 아니면 중간 버퍼 1장 · 62 §2-1) | 창마다 ≤ 4벌(레티나 메인 창 ≈ 21 MB/장) | `gfx.mac_present`(`softbuffer` = 끔 = 기본) | softbuffer | softbuffer | softbuffer | `present.rs` · nexa-sys `layer_present` |
| 편집기 그리기 캐시(본문 UTF-8 사본 + 행당 28 B · 세대 열쇠) | 탭당 ≈ 파일 크기 + 1 MB/4만 줄 · 보이지 않는 탭은 회수 때 해제 | (회수 설정과 같음) | — | — | — | nexa-ctl `TextBox::release_caches` |
| 외부 파일 변경 감시 스레드 `file-watch`(58 · stat 서명 + 달라졌을 때만 읽기) | 활성 창의 보이는 탭 2s · 비활성 0 | `file.external_change`(off) · `file.external_check`(focus) · `file.external_poll_ms`(0 · 향상 모드 0) | 2000 | 2000 | 0 | `App::ext_tick` · `nexa_fs::watch` |
| IME 안내 조회(journal 09-22 §34 · 가린 칸에 포커스가 있는 동안 150 ms마다 `GetKeyboardLayout`+`WM_IME_CONTROL` 둘 · 아니면 0) | 포커스 동안만 · 새 타이머 없음(창 tick) | `ui.ime_hint`(끄면 0) | on | on | off | `imehint.rs` · `conn_win`/`input_win` `tick` |
| ★ 다중 열기 스레드 `multi-open`(journal 09-22 §30 · 열기 창에서 여러 파일 → **스레드 하나**가 고른 순서대로 읽어 자리 탭마다 결과) | 요청당 1개 · 파일 수 ≤ `file.open_max` · 끝나면 종료 · 동시 N개 없음 | `file.open_max`(1~50 · 기본 10) | 10 | 10 | 10 | `main.rs multi_open_start/multi_load_poll` |
| 파일 적재 스레드 `file-load`(59 §5-2 · 8 MB 이상 파일마다 1개 · 읽기 1 MB 덩어리 → 풀이 → 본문 준비) | 파일당 1회 · 끝나면 종료 · 피크 ≈ 파일 × (1 + 글자당 4 B) · 막이 보이는 동안만 프레임 생성 | `file.async_load_mb`(기준) · `file.load_progress_ms` · 취소 = Esc/탭 닫기 | — | — | — | `fileload.rs` · `main.rs load_file` |
| 되돌리기 히스토리(60 · 탭당) | 글자 = 지운 것만 + 연산 48 B · 묶음 96 B · 예산 넘으면 오래된 것부터 | `editor.undo_budget_mb`(64 · 0 = 무제한) · `editor.undo_max` | — | — | — | nexa-ctl `EditState::evict` |
| 되돌리기 기록 파일(60 §7 D-129 · `<설정 폴더>/undo/*.nsqu`) | 저장할 때 1회 쓰기(≤ 4 MB · UI 스레드 · tmp+rename) · 열 때 1회 읽기 · 실행당 첫 저장 때 오래된 파일 치우기 · 큰 파일 탭은 안 씀 | `editor.undo_persist`(off = 쓰기·읽기 0) · `editor.undo_persist_mb` · `editor.undo_persist_days` | — | — | — | `undofile.rs` |
| 호출 서명 조회(63 · T-151 — `EXEC 루틴(…)` 앞에서 카탈로그 질의: Oracle `ALL_ARGUMENTS` · PostgreSQL `pg_proc` · SQL Server `sys.parameters`) | **루틴당 1회**(세션 캐시 · 재접속 때 비움) · 타입이 필요한 바인드·`OUTPUT` 빠진 바인드·PG 바인드 인자가 있을 때만 · 실패는 조용히(종전 동작) | **`vars.signature_lookup`**(off = 질의 0 · 09-21 신설) | on | on | on | 러너 `infer_call_bind_types` · `nsql-catalog::routine_args` |
| PostgreSQL 커서 이름 풀기(T-150) | 결과의 글자 값이 커서 이름 꼴일 때만(앞 32행 · 63자 이하) `pg_cursors` 확인 **왕복 1** + 열린 커서마다 `FETCH`·`CLOSE` · 사용자가 실행한 문장 안에서만(스스로 만드는 트래픽 0) | **`pg.refcursor_expand`**(off = 추가 왕복 0 · 이름이 글자로 나온다 · 09-21 신설) | on | on | on | nsql-driver-pg `expand_refcursors` |
| 편집 버퍼 `TextBuf`(59 §6 · 탭당) | 본문 UTF-8 + 갭(본문의 1/16 · 4 KB~4 MB) + 줄당 16 B · 줄별 폭·구문 상태 8 B/줄 · 보이지 않는 탭은 회수 때 갭을 접는다 | (회수 설정과 같음) | — | — | — | nexa-ctl `edit/textbuf.rs` |
| 막힘 감지 폴링(56 L3 · 메타 세션 1문장) | 미커밋 세션당 30s | `tx.block_poll_secs`(0 = 끔 · 향상 모드 0) | 30 | 30 | 0 | `App::tx_block_tick` |
| 탐색기 유휴 워터마크(57 T2 · 스키마당 1행) | 300s · 유휴일 때만 | `meta.refresh_secs`(0 = 끔 · 향상 모드 0) · `meta.refresh_scope` | 300 | 600 | 0 | `App::meta_refresh_tick` |
| 탐색기 DDL 뒤 갱신(57 T1·T4 · 폴더당 1질의) | 사용자 실행에 딸림 | `meta.refresh_on_ddl` · `meta.refresh_on_missing` | 켬 | 켬 | 켬 | `App::meta_flush` |
| 확장 저장소 읽기 스레드 `ext-index`(curl 자식 프로세스 · 50 §14) | 진행 중 1 · 사용자 동작(패널 열기·⟳)으로만 | `extensions.enabled`(끄면 패널·아이콘·읽기 전부 없음) | — | — | — | `App::ext_fetch_start` |
| 공유 연결(서버당 DB 세션 1 + 워커 스레드 1 · DR-34) | 8 | `session.max_shared` | — | — | — | `App::new_shared` |
| 전용/개별 탭 세션(DB 세션 1 + 워커 스레드 1) | 8 | `session.max_private` · `session.private_connect` | — | — | — | `App::new_private` |
| 유휴 세션 닫기(서버 자원 회수 · 점검 30s 로컬) | 1800 s · 전용만 | `session.idle_secs`(0 = 끔) · `session.idle_shared` | — | — | — | `App::idle_tick` |
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
| 구문 강조(탭 전체 재분석) | 없음 | `editor.highlight`(신설 · on/off) · ~~`editor.highlight_max_kb`~~(09-22 폐기 · 미배선 — 컷오프 = `file.large_syntax_level` L2 · [72](72-size-limits-and-large-file-constraints.md)) | on 1024 | on 512 | on 128 | `editors::make_box` |
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
| 드래그 자동 스크롤(T-158 · 09-21) | 50 ms · 걸음당 1~12줄 | `ui.max_fps`(프레임 상한) | 켬 | 켬 | 켬 — **드래그 선택 중 + 포인터가 편집기 위/아래 밖일 때만** 돈다(새 타이머·스레드 0 · 놓으면 0) | nexa-ctl `TextBox::tick` · `drag_autoscroll_active` |
| 로딩 점 애니메이션 | 300 ms | `ui.animations` | — | — | off(고정 "Loading…") | `explorer` · `FilePicker` |
| 툴팁 | `ui.tooltip_delay_ms` · `tabs.tooltip` · `explorer.tooltip` | 기존 | — | — | — | — |
| 슬라이드(패널) | `ui.slide_ms` | `ui.animations` | — | — | 0 | `conn_win` |
| 토스트 남은 시간 표시(왼쪽 상태 막대가 위에서부터 옅어짐 + 카드 진척 페이드 · 09-22) | 카드가 떠 있는 동안 30ms 틱(실행 카드는 끝난 뒤 카운트다운 동안만 · 새 타이머 0) | `ui.toast_progress`(신설) · `ui.toast_fade_to`·`ui.toast_bar_spent`(HIDDEN) | 켬 | 켬 | **끔**(향상 모드 = 종전 마지막 300ms 페이드만) | `toast` · `runtoast` |
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
| GDI face(HDC·HFONT·32bpp DIB ≈ em×3 × em×2 px · 스레드 로컬 · nexa-gfx `gdi.rs`) + 전진 폭 캐시 | face × 크기 × 굵게(≤ 수십) · 전진 폭 face×em×문자×굵게 · 글리프 비트맵은 rgb 3바이트/px | `ui.text_gdi`(끄면 0) | 프로세스 종료(44·46차) |
| OS 아이콘 RGBA(nexa-fs) | 512 | `file.icon_cache`(신설 · HIDDEN) | LRU · 프로세스 종료 |
| 파일 대화상자 목록·프로브·아이콘 사본 | 폴더 1개 분 | — | 대화상자 닫힘(Drop) |
| 결과 셋 | **편집기 탭당 그리드 1**(13차 · `grid_stash`) · 각 ≤ `grid.max_rows` | `grid.max_rows` | 다음 실행 · 탭 닫힘(즉시) |
| 로그 창 | (신설 상한) | `log.max_lines` | 초과 시 앞에서 버림 |
| 실행 취소 | (신설 상한) | `editor.undo_max` | 탭 닫힘 |
| 최근 파일·폴더 | `file.recent` 개수 | 기존(10) | — |
| 탐색기 노드 | 펼친 것만 | — | 접기/해제 |
| 폰트 mmap | 파일 기반(공유 페이지) | — | 26 §7 착시 주의 |

### 3-7. ★ 기능 성능 프로파일 — 기능 하나가 무엇을 쓰는가(사용자 09-22)

> **왜 표가 하나 더 필요한가**: §3-1~3-6은 *부하원*(스레드·소켓·캐시)을 도메인별로 모은 표다. 사용자가 요구한 점검 차원은 그보다 넓다 — *"프로세스 & 쓰레드 적용 · 기능이 메인 프로세스에 영향을 미치는지 · 로딩된 상태에 대한 데이터 구조 및 재사용성 · 파일 및 메모리 사용량 · I/O"*. 그래서 **기능 단위**로 그 답을 한 줄에 적는다. 새 기능은 [71 §5](71-performance-review-process.md)의 체크리스트를 채우고 여기 한 줄을 남긴다.
>
> 열 뜻: **스레드/프로세스** = 이름 있는 작업 스레드나 자식 프로세스(없으면 UI 스레드) · **메인 영향** = UI 스레드를 잡는 시간과 범위(창 전체 / 그 탭 / 없음) · **적재 구조·재사용** = 켜져 있는 동안 무엇을 몇 벌 들고 있나(복사인가 공유인가) · **회수** = 언제 놓나.

| 기능(들어온 차수) | 스레드/프로세스 | 메인 영향 | 적재 구조 · 재사용 | 파일·I/O | 회수 | 끄는 키 | 향상 모드 |
|---|---|---|---|---|---|---|---|
| **결과 데이터**(DR-33 · 52차) | 워커 스레드가 받아 채널로 | 수신 0(소유권 이동) · 그리기는 보이는 행만 | `ResultData` = `Arc` 세그먼트 **한 벌** · 그리드·복사·텍스트 보기·CLI 렌더러는 `RowSource` **인덱스 뷰**(복사 0) · 셀당 ≈ 61 B | — | 다음 실행 · 탭 닫힘(즉시) | `grid.max_rows` · `grid.result_tabs` | `grid.result_tabs=off`(들고 있는 결과 수 억제) |
| **세션 컨텍스트**(DR-34 · 56~62차) | 세션마다 `nsql-worker` 1(+ SQLite는 `nsql-sqlite` 1) | 없음(문지기 `gate_open`만 UI에서) | 세션별 상태는 `Sess`에(App에 두지 않는다) · 서버별 메타는 `ExplorerSet`이 **공유** | — | 유휴 닫기 · Disconnect | `session.max_shared/max_private/idle_secs` | — |
| **편집 버퍼 `TextBuf`**(T-142 · 84차) | 없음(UI 스레드) | 입력당 ≤ 3~8 ms(70만 줄) | UTF-8 본문 **한 벌** + 갭(1/16) + 줄 표 16 B/줄 · 줄별 폭·구문 상태 8 B/줄 · **세대(rev)로 캐시 무효화** · 본문 전체를 `Vec<char>`로 뜨지 않는다 | — | 보이지 않는 탭은 갭을 접는다 | (회수 설정) | — |
| **되돌리기 = 연산 기록**(83차) | 없음 | 묶음당 0.07 ms | `Op{pos, remove_n, insert}` — **지운 글자만** 저장 · 스냅숏 0 · 예산 넘으면 오래된 것부터 | 기록 파일 `undo/*.nsqu`(저장 때 1회 · 열 때 1회) | 탭 닫힘 · 예산 축출 | `editor.undo_budget_mb` · `editor.undo_persist` | `editor.undo_persist=off` |
| **큰 파일 모드 · 탭 격리 적재**(82~84차) | 파일당 `file-load` 1(8 MB 이상) | **0 ms**(옮겨 넣기) · 진행 막은 **그 탭에만** · 다른 탭은 그대로 | 읽은 버퍼가 그대로 본문(사본 0 · UTF-8·LF면 재사용) · 준비본 문자열을 그리기 캐시가 그대로 씀 | 1 MB 덩어리 읽기 | 탭 닫으면 기준선 | `file.async_load_mb` · `file.large_*` | 큰 파일 **단계**가 맡는다(향상 모드 아님) |
| **메모리 회수**(83차) | 없음 | 유휴에 수 ms | — | — | 놓은 1초 뒤 1회 + 유휴 300 s | `mem.trim_on_release` · `mem.trim_secs` | — |
| **외부 파일 변경**(81차) | `file-watch`(보이는 탭만) | 폴링 0(stat 서명) · 달라졌을 때만 읽기 | 저장본 사본 1(병합용 · 큰 파일은 안 만든다) | stat 2 s · 변경 시 읽기 | 탭 닫힘 | `file.external_change` · `file.external_poll_ms` | `file.external_poll_ms=0` |
| **변수 관리**(88~89차) | 없음 | 문장 준비 1.2 µs | `VarStore` 계층(전역 ▸ 파일 ▸ 실행) · 값 상한 `vars.max_value_kb` · 서명 캐시는 **루틴당 1회**(세션 안) | 보존 파일 `vars/<해시>.sql`(실행당 ≤ 1회) | 재접속 때 서명 캐시 비움 | `vars.persist` · `vars.signature_lookup` | 제외(결과가 바뀐다 — §4-6) |
| **★ 프로젝트 탐색기**(93차 · [67 §4-1](67-project-workspace.md)) | 없음(UI 스레드) | 펼칠 때 그 폴더 1회 열거 · **필터를 칠 때만** 상한까지 훑는다 | 트리 노드 = 펼친 것 + 필터로 찾은 것만(**지연 열거**) · 상한 `project.scan_max` 5,000에서 멈추고 안내 | 프로젝트 파일 JSON 1(열 때) · 폴더 `read_dir`(펼칠 때) | 패널을 닫아도 노드는 유지(다시 열 때 재사용) · 프로젝트를 닫으면 버림 | `project.scan_max`(상한) · 패널 토글 `view.project` · `project.icons`(OS 아이콘 조회·캐시 · 향상 모드 off) · `project.auto_reveal`(탭마다 펼침 · 기본 off) | **`project.icons`만 등재**(09-23 96차 실측 [26 §7-10](26-performance-architecture.md): 트리 자체는 접속 대비 +0.07 MB · 유휴 CPU 0이지만 **OS 셸 아이콘을 켜면 프로세스에 한 번 +2 MB · GDI +37**(셸 시스템 이미지 리스트 · OS 몫 · 회수 불가) + 워커 스레드·COM 아파트먼트(핸들 ≈ 26 · 스레드 1)는 **조회가 끝난 뒤 유휴 5초에 패널이 거둔다**(`ICON_WORKER_IDLE_MS` · 캐시는 남는다) → 향상 모드 강제 off([45 §4-1](45-perf-boost-benchmark.md)) · 사용자가 끄면 캐시도 비운다) |
| **★ 미리보기 탭**(93차) | 없음 | 없음 | 미리보기 탭은 **한 번에 하나** — 다음 미리보기가 그 자리를 **재사용**(탭·버퍼 새로 만들지 않음) · 편집하면 정식 탭으로 승격 | 파일 읽기 1 | 자리 재사용이 곧 회수 | `project.preview_tab` | 미등재(끄면 탭이 늘어 **메모리가 더 든다**) |
| **★ 파일 다중 열기**(93차) | `multi-open` **1개**(요청당 · 끝나면 종료) | 자리 탭을 먼저 다 만들고 순차 적재 · 창 안 막음 | 파일마다 자리 탭 → 준비본 이동(사본 0) | 파일 ≤ `file.open_max` 10 | 탭 닫힘 | `file.open_max` | 미등재(사용자 동작에만 딸림) |
| **★ 다중 인스턴스 시작 모드**(93차) | 없음 | 기동 때 1회 `try_lock` | 잠금 파일 핸들 1개를 프로세스가 보유 | `instance.lock`(0 바이트 · 설정 폴더) | 프로세스 종료 | `project.restore_last`(기본 끔 — 둘째 인스턴스가 프로젝트를 복원하지 않는다) | 미등재(비용 0) |
| **★ 토스트 진행 막대**(93차) | 없음 | 카드가 떠 있는 동안 30 ms 틱(새 타이머 0) | — | — | 카드가 사라지면 0 | `ui.toast_progress` | ✅ `off` |
| **★ IME 안내**(93차) | 없음 | 가린 칸에 포커스가 있는 동안 150 ms마다 조회 2 | — | — | 포커스가 떠나면 0 | `ui.ime_hint` | 미등재(포커스 조건부라 상시 비용 0 · low 프리셋에서 off) |
| **확장 패널**(79차) | `ext-index` 1(사용자 동작 때만) + `curl` 자식 | 없음 | 목록 JSON 1벌 | `extensions/` · `index.json` | 패널 닫힘 | `extensions.enabled` | — |
| **탐색기 갱신**(80차) | 메타 스레드(서버별 `nsql-explorer`) | 없음 | 폴더별 디프 갱신(트리 전체 재구축 0) | — | — | `meta.refresh_*` | `meta.refresh_secs=0` · `meta.refresh_highlight_ms=0` |
| **수동 커밋 잠금 방지**(79차) | 없음(메타 세션 1문장) | 없음 | — | — | — | `tx.block_poll_secs` | ✅ `0` |
| **상태줄 git**(33차) | `nsql-git` + `git` 자식 2 | 없음 | — | 폴더 바뀜·저장·15 s | — | `statusbar.git` | ✅ `off` |
| **북마크**(09-22 · 69) | 스레드 0 · 틱마다 활성 탭의 줄 변경 기록만 소비(O(변경 수)) · 열 때 재탐색 O(줄 수 · `bookmark.relocate_max_lines` 상한) · 저장 = 디바운스 1초/최대 5초 임시 파일 → rename | `bookmark.enabled`(전부 끔) · `bookmark.persist`(저장 끔) · `bookmark.search_lines`/`relocate_max_lines` — 읽는 곳 `bookmarks.rs apply_settings` · 표시 셋 `bookmark.gutter`/`minimap`/`inline_label`(끄면 그 표시 계산 0 · `bm_refresh_tab`) | `bookmarks.rs` `sync_tab`/`tick_save` |
| **텍스트 보기 변환**(29차 · DR-33) | `nsql-textview`(변환 중만) | 없음 | **결과 복제 0**(Arc 공유) · 파생 `text_lines`만 예산에 | — | 보기를 그리드로 되돌리면 | (보기 모드) | — |
| **파일 검색**(T-81) | `nsql-search` ≤ `search.threads` | 없음 | 스트리밍(파일 전체를 들지 않는다) | 폴더 걷기 | 검색 끝 | `search.threads` · `search.max_file_kb` | — |
| **신호등**(프로브) | `nsql-probe-collect` ≤ `probe.max_inflight` | 없음 | — | 소켓 SYN | — | `probe.enabled` · `probe.interval` | ✅ 300 s · 동시 1 · ICMP off |

**스레드 원장**(이름 · 언제 생기고 죽나 — D11): `nsql-worker`(세션마다 · 세션 수명) · `nsql-sqlite`(SQLite 세션 · 연결 소유) · `nsql-cancel`(실행 취소 요청마다 · 1회) · `nsql-probe-collect`(창이 열려 있는 동안) · `nsql-explorer`(서버별 메타 · 유휴 회수) · `nsql-textview`(변환 중) · `nsql-git`(조회마다) · `nsql-log`(파일 싱크를 켰을 때) · `nsql-search`(검색 중) · `file-load`(큰 파일마다 1회) · `multi-open`(다중 열기 1회) · `ext-index`(확장 목록 1회) · `nsql-demo`(데모 DB 생성 1회). **유휴 상주 = 11~14개**(접속 0이면 11 · 접속 1이면 13~14) — 나머지는 일이 끝나면 죽는다.
**자식 프로세스**: `git`(상태줄) · `curl`(확장 목록) · `ping`(프로브 ICMP · 기본 끔) · `cmd`/`open`/`xdg-open`(OS 연결 프로그램) · `defaults`/`gsettings`(OS 테마) — 전부 **사용자 동작이나 주기 설정에 딸리고**, 끄는 키가 있다. 앱이 상주 자식 프로세스를 두지 않는다(드라이버도 in-process · DR-29).

---

> **부하원 추가(09-16 33차)**: 상태줄 git 조회 스레드 `nsql-git`(git CLI 2회 · 폴더 바뀜/저장/15초 · 동시 1) — 끄는 키 `statusbar.git`.
> **부하원 추가(09-16 29차)**: 텍스트 보기 변환 스레드 `nsql-textview`(변환 중에만 · 500행 블록 · 취소 깃발 · **결과 복제 0 — 09-17 DR-33: `ResultData` Arc 세그먼트 공유 + 인덱스 뷰** · 파생 `text_lines`는 예산에 포함) — 끄는 키 없음(보기를 그리드로 두면 0) · 자세히 [journal 29차](journal/2026-09-16.md).

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

### 4-6. ★ 실행 속도 향상 `perf.boost`(사용자 09-17) — "UI 구성에만 영향" 키를 최적값으로 **강제 + 잠금**

> **요청 원문 요약**: 성능 항목에 "실행 속도 향상" 메뉴 — 파일/메모리 로딩(I/O) · 아이콘 표시 등 메모리 증가(우클릭 포함) · 렌더링 방식이나 폴링의 시도 횟수·시간 — **실제 동작에는 문제가 없고 UI 구성에만 영향을 미치는 설정**을 모아, 켜면 그 설정들은 수정할 수 없고 설정값과 무관하게 최적값이 적용된다.

**목표(사용자 보충 09-17)**: ① 처음 실행 속도 ② 쿼리 등 네트워크 실행 속도 ③ 백그라운드·편의 기능 스레드 최소화 ④ 메모리 최소화·빠른 회수 ⑤ 체감 속도. 목표 ↔ 항목: ① = 시작 시 로그 창 안 열기 · 아이콘 래스터 0 · git 프로세스 0 ② = 프로브·라이브 로그 폴링 최소(실행 경로의 왕복 수는 그대로 — 결과에 영향이 있는 페치 크기는 제외) ③ = 프로브 스레드 1 · git/감시/자동 갱신 스레드 0 ④ = 아이콘·미니맵·HTML 복사 캐시 0 · 아이콘 캐시 128 · (텍스트 보기 캐시 즉시 해제는 DR-33에서 상시) ⑤ = 애니메이션·페이드·깜빡임·툴팁 0.

**목적 검토(내부)**: `perf.mode`(§4-1)는 *부하를 줄이는* 거버너로 개별 값이 프리셋보다 우선하고(사용자 존중) 동작 상한(행 수·타임아웃)까지 포함한다. 요청은 반대 방향의 **강제 스위치**다 — 개별 값보다 우선하고, 설정 창에서 잠그며, 대상은 **동작 결과에 영향이 0인 키로 한정**한다. 둘은 겹치지 않는다: `perf.boost`는 `perf.mode`와 독립(둘 다 켤 수 있고 강제값이 프리셋보다 위 · custom 표시와 무관). 저장값은 건드리지 않으므로 끄면 그대로 돌아온다. → 우선순위 = **향상 강제값 > 개별 값 > 모드 프리셋 > 기본**([`Settings::effective`]).

**분류(원장 = `nsql-settings::perf::BOOST` · 테스트가 레지스트리 존재·검증 통과를 강제)**

| 구분 | 키 → 강제값 | 사용자 예시와의 대응 | 비고 |
|---|---|---|---|
| 렌더링·애니메이션 | `ui.animations` off · `ui.max_fps` 30 · `editor.caret_blink` off · `ui.fade_fast/slow` 0 · `ui.fade_out_ms` 0 · `ui.slide_ms` 0 · `ui.hover_intent_ms` 120 · `ui.toast_progress` off | "렌더링 방식" | 다시 그리기 횟수를 줄인다 · `max_fps`·`caret_blink`는 배선 T-90c 뒤 효과 |
| 아이콘·부가 표시 | `explorer.icons` off · `file.os_icons` off · `file.probe_chevrons` off · **`ui.menu_icons` off(신설 · 우클릭 메뉴 아이콘)** · `tabs.tooltip` off · `editor.minimap` off · `editor.highlight_selection` off | "아이콘 표시 등 메모리 증가 · 우클릭 포함" | 메뉴 아이콘 = nexa-ctl `set_menu_icons`(토글 도형은 유지) · 툴팁·미니맵·선택어 강조는 추가 반영 |
| I/O·기동 | `statusbar.git` off(git 프로세스) · `editor.copy_rich` off(복사 시 HTML 생성) · **`ui.clipboard_probe` off(신설 · 우클릭마다 클립보드 읽기 → 붙여넣기 항상 활성)** · `log.open_at_start` off(둘째 창) | "파일/메모리 로딩(I/O) · 우클릭" · 목표 ① | 추가 반영 · `settings.watch_ms`는 미등록 키라 제외(T-90c) |
| 메모리 | `file.icon_cache` 128 | 목표 ④ | 아이콘을 껐으므로 캐시도 최소 |
| 폴링·시도 횟수·시간·스레드 | `probe.interval` 300 · `probe.max_inflight` 1 · `probe.max_retries` 1 · `probe.icmp` off  · `oracle.live.source` off · `oracle.live.interval_ms` 5000 | "Pooling 등의 시도 횟수, 시간" | 신호등은 남기되 드물게 · 라이브 로그는 표시용이라 끔 |

**제외(동작에 영향)**: `grid.max_rows`·`db.fetch_size`·`db.fetch_all_size`·`db.statement_timeout`·`db.cursor_idle_secs`(결과·페치) · `session.*`·`tx.*`(트랜잭션) · `connect.auto_reconnect`·`connect.max_concurrent`·`probe.enabled`(접속·신호등 자체) · `editor.highlight_max_kb`·`editor.max_occurrences`·`editor.undo_max`·`log.max_lines`(기능 상한 — 줄이면 사용자가 잃는다) · `ui.glyph_cache`·`file.icon_cache`(캐시는 클수록 빠름) · `ui.text_gdi/hint/snap/contrast`(글자 품질) · `log.file`(사용자 기능) · `explorer.visible`(사용자 배치).

**구현**: 레지스트리 `perf.boost`(Bool · Performance) · `perf::BOOST` 표 · `Settings::effective` 최우선 · `boost_locked(key)` · `PerfSource::Boost`(`nsql config list perf` 출처 "향상") · 설정 창 = 대상 카드 잠금 + 설명 아래 "⚡ 실행 속도 향상 적용값: X" · 호스트 = 켬/끔 때 대상 키 전부 `apply_setting`(즉시 반영) · 신설 배선 `ui.menu_icons`(nexa-ctl ctxmenu 전역) · `ui.clipboard_probe`(우클릭 클립보드 읽기 생략). 향상 모드는 부하원 원장(§3)에 새 행을 만들지 않는다(기존 키의 값만 강제).

> **09-21 추가 2(사용자 결정)**: 향상 모드 강제 표에 **`grid.result_tabs=off`** — "다중 탭을 끄는 것은 의도적으로 메모리 사용을 억제하기 위한 설정"(목표 ④). 결과 자체는 그대로 조회·표시되고 **동시에 들고 있는 결과의 개수**만 줄어든다(편집기 탭당 하나 · 나머지 즉시 해제 D-73). 다중 탭 설정은 독립 키로 유지하고, 종속 키 `grid.result_tabbar_single`(결과 1개에도 탭 영역)은 그동안 잠긴다. 큰 파일 쪽의 상시 비용은 향상 모드가 아니라 **큰 파일 단계**가 맡는다: 확장 효과 = `file.large_ext_level`(L1) · 구문 강조 = `file.large_syntax_level`(L2) — [59 §5](59-large-file-handling.md).
>
> **09-21 추가(Windows 89차 · 사용자 "추가한 기능 가운데 성능 향상 영역에서 관리할 대상 반영 · OS별 동작 포함")**: 향상 모드 강제 표에 `editor.undo_persist=off`(되돌리기 기록 파일 — 저장·닫기 때 쓰기 + 열 때 읽기·검증 · 세션 안의 되돌리기는 그대로) · `meta.refresh_highlight_ms=0`(갱신 뒤 강조가 사라질 때까지의 다시 그리기). **넣지 않은 것**: `vars.signature_lookup`·`pg.refcursor_expand`(결과가 바뀐다 → 개별 스위치로만 · §3-1) · `vars.persist`(다음 실행의 바인드 값이 달라진다) · `grid.result_per_statement`·`run.cursor_autoshow`·`vars.brace_subst`·`vars.max_value_kb`(결과 표시·값). **OS별**: `gfx.mac_present=iosurface`는 **macOS에서만** 효과가 있는 가장 큰 그리기 지렛대(present 36 → 2.9 ms · 유휴 CPU ⅓)지만 Apple Silicon·크기 조절 중 실기(T-147)와 기본값 결정(D-133)이 끝나지 않았다 — 검증 안 된 경로를 **잠금 강제**하면 문제가 났을 때 향상 모드 전체를 꺼야 하므로 **D-133 뒤 등재 후보**로만 둔다(Windows·Linux는 softbuffer 한 길이라 키가 있어도 효과 없음) · 메모리 회수(`mem.trim_secs`)는 OS별 구현(Windows `HeapCompact` · Linux `malloc_trim` · macOS zone pressure relief)이 이미 목표 ④에 맞아 그대로.
>
> 09-17 추가: 향상 모드 강제 표에 `editor.diff_marks=off`(줄 디프 계산 · 편집마다) · `log.dev_mode=off`(상세 로그 게이트 0 · [48](48-logging-architecture.md)).

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
| **T-90a** | ✅ 09-16 49차: 레지스트리 `perf::PERF`(=`Entry::perf()` · 병행 append 충돌 회피로 필드 대신 별도 표 · 필드 이관 후속) + `perf.mode` + `Settings::effective` + `nsql config list perf` + 기존 키 PerfBinding · 잔여 = 상태줄 ⚡ · 설정 창 Performance 카드(모드 + 링크 행) | 중 |
| **T-90b** | DB: `db.statement_timeout`(드라이버 cancel 포트 — Oracle `OCIBreak` · MSSQL attention · PG `pg_cancel_backend`/소켓 · SQLite `interrupt`) · `db.fetch_size` · 탐색기 숨김 = 세션 안 엶 | 중 |
| **T-90c** | CPU/GFX: ~~`editor.highlight_max_kb`~~(09-22 폐기) · `editor.max_occurrences`(09-22 배선) · `ui.max_fps` · `ui.animations`(OS 동작 줄이기 = nexa-fs::sys) · `editor.caret_blink` · `file.os_icons` · `file.probe_chevrons` | 중 |
| **T-90d** | ✅ 09-16 49차: `log.max_lines` · `editor.undo_max` · `ui.glyph_cache` · `file.icon_cache` 세터 + 상한 테스트 + main.rs 배선 | 소 |
| **T-90e** | 진단·게이트: `--trace-net` · `--trace-frames` · 기동 `--timing` · `memcycle.ps1` 시나리오 인자(L-2~L-8) · `perf.yml` CI 잡 · 로그 창 `perf` 종류 | 중 |
| **T-90f** | (후보) 더티 영역 다시 그리기 · DNS 캐시 · auto 모드 자동 전환(D-58 뒤) | 대 |

**결정 ✅ 09-16 = DR-31**: D-58 `full` + 1회 안내 · D-59 custom · D-60 `nexa-sys` 크레이트(macOS reduce motion = `CFPreferencesCopyAppValue` · 원격 세션 macOS 없음) · D-61 0.

**(기록) 결정 대기였던 것**
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
