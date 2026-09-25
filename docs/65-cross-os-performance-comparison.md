# 65. OS별 성능 비교 — 실행 속도 · 용량 · 메모리 (Windows · macOS · Linux · 2026-09-22 기준)

> 사용자 09-22(Linux 91차): "윈도우와 맥에서 진행했던 성능 평가를 동일하게 진행하고 속도·메모리·기능·용량 전수 검사·최적화" → "각 운영체제별 차이도 분석" → "비교 문서 작성".
> 원자료: Windows = [26 §7-5·7-6](26-performance-architecture.md) · [59 §6-2](59-large-file-handling.md) · [64 §3-1·§5-1](64-dbms-clients-and-driver-packaging.md) · [45](45-perf-boost-benchmark.md)(89차 · 09-21) / macOS = [55 §3](55-editor-input-latency.md) · [62](62-macos-input-and-present.md) · [journal 09-21](journal/2026-09-21.md)(86~90차) / Linux = [26 §7-7](26-performance-architecture.md) · [journal 09-22](journal/2026-09-22.md)(91차 · 원본 `target/perf-linux-2026-09-22/`).
> ★ **읽는 법**: 세 기기는 성능이 다르다(Windows 개발 PC ≫ mac M-시리즈 ≈ · Linux는 2코어 VM). 절대값 비교가 아니라 **① 같은 기기 안 전/후 A/B ② 예산(26 §5 · 39 §2) 대비 ③ 형상이 같은가**(셀당 바이트 · 피크/상주 비율 · 회수 여부)를 본다.

## 0. 결론 다섯 줄

1. **메모리 형상은 세 OS가 같다**: 결과셋 셀당 ≈ 61 B(10만 행 × 5열 = +29~30 MB) · 65 MB 파일 피크 ≈ 106~108 MB · 탭/결과를 닫으면 기준선으로 회수 · 반복 작업 누수 없음(힙 조각만 · 10만 행 재실행 16회 +8 MB에서 평탄). 힙(창 버퍼 제외)은 4~5 MB.
2. **속도는 기기 성능 폭 안**: 편집기 입력·그리기·되돌리기·문장 준비가 Linux VM에서 Windows·mac의 1.5~2배 — 구조적 회귀 없음. 예산(프레임 ≤ 8 ms · 유휴 CPU 0)은 셋 다 만족.
3. **OS 전용 병목이 둘 있었고 둘 다 고쳤다**: macOS present(CoreAnimation 색 변환 36 → 2.9 ms · D-133) · Linux 기동(글꼴 트리 12회 걷기 + 아이콘 래스터 메모 없음 → 창까지 **522 → 111 ms · −79 %**). 아이콘 메모는 3-OS 공통 코드라 Windows·mac 기동도 줄어야 한다(다음 세션 A/B).
4. **용량**: 실행 파일은 Windows 10.3 / Linux 12.6 MiB(GUI) · 6.69 / 7.36~7.49 MiB(CLI) — 형식(PE/ELF)·링크 차이. 드라이버 4종을 전부 빼도 −2.3 MiB(두 OS 같은 비례). 무게는 앱 밖의 Oracle Instant Client(382~407 MB)다.
5. **측정 도구가 3-OS에 갖춰졌다**: `win-*.ps1`(Windows) · `ps`/`sample`/`NSQL_TRACE_MEM`(mac) · `linux-*.sh`(Linux · `/proc` · `strace`) + 공통 `NSQL_TRACE_FRAMES`(프레임 · **`[startup]` 기동 구간** 09-22) · nexa-ui `bench_editor`/`bench_undo` · `bench_vars`.

## 1. 환경

| | Windows(89차 · 09-21) | macOS(86~90차 · 09-20~22) | Linux(91차 · 09-22) |
|---|---|---|---|
| 기기 | 개발 PC(Windows 11 · NVMe · 고성능) | **Intel i9-9980HK 16코어 · 32 GB · macOS 26.6**(09-24 정정 · 종전 "M-시리즈" 오기) | **Ubuntu 26.04 VM · 2코어 · RAM 3.3 GB** · Wayland(GNOME) |
| 빌드 | Release(LTO fat · strip) | 같음 | 같음 · 워크스페이스 Release 6.5분 |
| 화면 | softbuffer(GDI DIB) · 1375×945 · 배율 1.0 | softbuffer / IOSurface(선택) | softbuffer(wl_shm) — 91차 후반부터 기본 **X11(XWayland)**(모달 때문 · journal §12) |
| 메모리 지표 | **Private Bytes**(창 버퍼 포함) | `phys_footprint`(`NSQL_TRACE_MEM`) | **RssAnon**(≈ Private · 창 버퍼는 공유 매핑이라 제외) + RSS |
| 실서버 | Oracle WAN(61.81…) · MSSQL LAN | PG만 | Oracle·MSSQL·PG 모두 LAN(RTT 12 ms) |

## 2. 실행 속도

### 2-1. 기동(창이 보이기까지 · 3초 CPU)

| | Windows(89차) | **Windows(94차 · 09-22)** | macOS | Linux(전) | **Linux(후)** |
|---|---:|---:|---:|---:|---:|
| 창까지 | 141 ms | **47 ms** | 592 → **407 ms**(100차 09-24 → 09-26 · 26 §7-11/7-12) | 522 ms | **105~111 ms** |
| 3초 CPU | 500 ms | **156 ms** | 900 → 860 ms | 430 ms | **120 ms** |
| 예산 300 ms | ✅ | ✅ | ✗(NSWindow 생성 168 ms + 프로세스 기동 · T-194) | ✗ | ✅ |

★ **Windows 94차의 −67 %는 Linux에서 고친 것이 그대로 들어온 값이다**(§6 3번의 답): 아이콘 래스터 `memo` + 글꼴 트리 걷기 캐시는 3-OS 공통 코드라, Linux 91차에서 찾아 고치고 92차에 3-OS로 복귀시키자 Windows 기동도 141 → 47 ms가 됐다. **한 OS에서 찾은 병목이 다른 OS의 이득이 되는 첫 사례**이고, OS별 전수 점검을 돌리는 이유이기도 하다([26 §7-8](26-performance-architecture.md) B단계 · mac은 아직 미계측).

Linux `[startup]`(누적 ms · 후): settings 0 → fonts 3~9 → event_loop 8~17 → … → `prefs_win` 25~41 → `act_bar` 44~57 → `app` 125 → 첫 페인트 +255~314. 무엇을 고쳤나: ① nexa-font 가족 탐색이 `/usr/share/fonts`(976 파일)를 가족마다 걷고 항목마다 `statx`(11,786회) → 걷기 1회 캐시 + `file_type()` ② 툴바·메뉴 아이콘 래스터(64×64×16 표본 · SVG 경로 아이콘 개당 ≈ 15 ms)가 메모 없이 호출마다 + 찾기 막대 11개를 시작 시 즉시 → 모양별 `memo` + 첫 그리기 때. 남은 몫(T-102) = 설정 창 생성 ≈ 40 ms · 시작 시 보이는 도구줄 아이콘(스캔라인/캐시 파일) · `[startup]`의 `app` 잔여 ≈ 70 ms.

### 2-2. 편집기·엔진 벤치(nexa-ui `bench_editor`/`bench_undo` · nsql-run `bench_vars`)

| 항목 | Windows | macOS | Linux VM | 비 |
|---|---:|---:|---:|---:|
| 70만 줄(72 MB) 글자 하나 입력 | 3.0~4.1 ms | 3.5~3.9 | 6~7 | 1.7× |
| 70만 줄 평상시 그리기 | 2.1 | 2.1 | 5.0 | 2.4× |
| 4만 줄 전 기능 입력+그리기 | 7.5~8.6 | 8 | 10~12 | 1.4× |
| 되돌리기(낱말 타이핑) | 0.07 ms | 0.07 | 0.15 | 2× |
| 붙여넣기 100 KB | 0.4 | — | 0.6 | 1.5× |
| 문장 준비(`bench_vars` plan) | 1.2 µs | 1.2 | 1.5 | 1.25× |
| 변수 대입 / 조회 | — | — | 0.7 / 0.16 µs | |

→ 비율이 항목마다 1.3~2.4×로 고르다 = 기기 차이. 특정 항목만 튀는 곳(구조 회귀) 없음.

### 2-3. 프레임 · 화면 내보내기

| | Windows | macOS | Linux |
|---|---:|---:|---:|
| 프레임 평균(1375×945) | 드래그 중 1.3 ms(09-16) | 51 → 16 ms(IOSurface · 86~88차) | **5.8~7.1 ms**(chrome 2 · editor 1 · grid 1 · present ≤ 0.9) |
| present | GDI(프레임 안) | softbuffer **36 ms** → IOSurface **2.9 ms**(`gfx.mac_present` · 선택) | wl_shm 0.03~0.9 ms |
| 유휴 CPU | 6초 47 ms(창 4개 포함) | 향상 모드 ⅓ | **0~10 ms/초** |

### 2-4. CLI 실행 시간(`nsql run --timing` · 3회)

| 질의 | Windows(09-17) | Linux(09-22) | 해석 |
|---|---:|---:|---|
| SQLite 10만 행 × 5열 | — | execute **93~150 ms** · 200행 0.3 ms | |
| Oracle 200행 | execute 6.8 · fetch 21.5 ms(WAN · 실제 표) | execute 59~73 · fetch 107~120 ms(dual · 192 KB · RTT 12 ms · strace 왕복 19/90) | 왕복 수는 과하지 않다 · 서버·경로 차 |
| SQL Server `TOP 200` | 37~38 ms(LAN) | 124~143 ms(RTT 12 ms) | |
| PostgreSQL 200행 | — | 110~123 ms | |
| 프로세스 전체(접속 포함) | Oracle 227 · MSSQL 135 ms | Oracle 840 · MSSQL 550 · PG 440~527 ms | Oracle 접속 0.6 s(OCI) |

⚠ `SELECT COUNT(*) FROM all_objects`(BISCM)는 서버에서 10분 넘게 걸린다(sqlplus도 같음) — 측정 질의로 쓰지 않는다.

## 3. 메모리

### 3-1. 시나리오별 상주(Windows Private ↔ Linux RssAnon / RSS)

| 시나리오 | Windows 89차 | Linux 91차 | 형상 비교 |
|---|---:|---:|---|
| 기동(로그인+메인) | 11.14 MB | 4.1 / 34.3 MB | Windows Private에는 창 프레임버퍼(5.2 + 1.6 MB)가 든다(26 §7-1) — 힙은 ≈ 4~5 MB로 같다 |
| 접속(SQLite) | 9.71 | 3.4 / 30.7 | |
| + 보조 창 4 | 12.42 | 4.7 / 36.8 | 창당 +1.5 MB(공유 버퍼) |
| 2 MB 스크립트(전 기능) | 27.45 | 15.3 / 43.0 | +12 vs +17.7 MB |
| 6.4 MB(L1) | 17.8 | 11.1 / 38.5 | |
| 20 MB(CRLF · 한글) 상주 / 피크 | 34.9 / 67.2 | 26.5 / 54.4 · 피크 68.5 | 피크 같음 |
| **65 MB(L2)** 상주 / 피크 | **86.5 / 106.5** | **80.3 / 108.0** | 같음 |
| 결과 200행 | 10.9 | 3.6 / 31.9 | |
| **결과 10만 행 × 5열** | **40.91**(+30) | **32.6 / 60.7**(+29) | **셀당 ≈ 61 B 동일** |
| 그 탭 닫은 뒤 | 10.9 | 3.7 / 32.0 | 회수 ✅ 둘 다 |
| 향상 모드 | 10.6 | 3.4 / 30.7 | |
| **macOS**(100차 · 26 §7-11 → 7-12 · footprint) | — | — | 접속 56 → **96 MB**(IOSurface 2벌 +40 · RSS 136 같음) · 10만 행 +24(셀당 ≈ 50 B) · 닫기 회수 ✅ · 65 MB 파일 132 → 172(같은 +40) · 신규 기능(검색 인덱스·메타 3층) +2 MB |

### 3-2. 반복 작업 누수(뒤 절반 기울기 · MB/주기)

| 주기 | Windows | Linux |
|---|---:|---:|
| 큰 파일 열기 → 닫기 | 평탄 | 6.4 MB 0.020 · 2 MB 0.030 |
| 보조 창 토글 | 평탄 | 0.000 |
| 10만 행 재실행 | +0.3/회 · 탭 닫으면 회수 | ×8 1.15(6·7주기 +2.3씩) → **×16 0.20**(4주기 32.8 → 16주기 40.4 MB · 실행 중 옛+새 72 MB → 회수 40 MB) |

→ 둘 다 **누수 아님**(결과 한 벌 + 힙 조각 ≈ 8 MB에서 평탄). 개선 여지: 새 결과가 오기 전에 옛 결과를 놓기(T-102).

## 4. 용량

| | Windows(64 §3-1) | Linux(91차) |
|---|---:|---:|
| GUI `nexa-sql` | 10.3 MB | 12.61 MiB |
| CLI 전부 | 6.69 MB | 7.36 MiB(`-p nsql-cli`) · 7.49(워크스페이스 통합 빌드) |
| SQLite만 | 4.43 | 5.02 |
| + Oracle | +0.34 | +0.32 |
| + PostgreSQL | +0.63 | +0.62 |
| + SQL Server | +1.37 | +1.45 |
| + MSSQL + PG | +1.92 | +2.02 |
| 전부 − SQLite만 | **+2.26** | **+2.34** |
| Oracle Instant Client(앱 밖) | 407 MB(23.9) | 382 MB(23.26 · Basic+SQL*Plus) |

→ 드라이버가 실행 파일에서 차지하는 몫은 두 OS에서 같은 비례(2.3 MB). 경량판은 Cargo feature 조합만으로(64 §4) · 확장으로 빼도 얻는 게 없다(64 §3-3).

## 5. OS별 구조 차이가 성능에 미치는 것(자세히 [61 §3](61-core-design-and-working-rules.md))

| 영역 | Windows | macOS | Linux | 성능 영향 |
|---|---|---|---|---|
| 글자 | GDI ClearType(D-77) | CoreText | nexa-font 자체 래스터(Noto CJK mmap 19 MB · D2Coding 가족 탐색) | Linux 기동 병목 ①(고침) |
| 화면 | softbuffer DIB | softbuffer / IOSurface | softbuffer(X11 XShm · 91차 후반 기본) | mac present 병목(고침) |
| 메모리 회수 | HeapCompact | pressure_relief | `malloc_trim` | 셋 다 즉시 1회 + 주기 |
| 창 버퍼 | 사적(Private에 포함) | 사적 | 공유 매핑(RssAnon 제외) | 지표 해석 차이만 |
| 모달 | owner + EnableWindow | 자식 창 | X11 transient + MODAL(x11rb) | 없음 |
| Oracle | oci.dll | libclntsh.dylib | libclntsh.so + libaio | 접속 0.6 s 공통 |

## 6. 남은 최적화 후보(T-102에 등재)

1. 시작 시 보이는 도구줄 아이콘 래스터(개당 ≈ 15 ms · SVG 표본마다 다각형 전체 검사) → 스캔라인 채우기 또는 캐시 파일 · `PrefsWin::new` ≈ 40 ms(설정 레지스트리 450+ 키 × 컨트롤) → 창을 열 때 만들기.
2. 10만 행 재실행: 옛 결과를 먼저 놓아 피크 72 → 43 MB.
3. ~~Windows·mac에서 아이콘 메모의 기동 효과 A/B(같은 코드).~~ ✅ **Windows 09-22 확인**(창까지 141 → **47 ms** · 3초 CPU 500 → 156 ms · §2-1) · **mac 09-24/26 계측**(592 → 407 ms · 남은 몫 = NSWindow 생성 168 ms · T-194).
5. mac 캐럿 깜빡임 = 활성 유휴 CPU 40~50 ms/초(IOSurface 뒤 · 26 §7-12 G-2) → 부분 present(T-224).
4. Linux X11 백엔드에서의 상주·프레임 재측정(3-1 표는 Wayland에서 잰 값).

## 7. 방법 · 재현

- Windows: `scripts/win-startup-probe.ps1` · `win-big-probe.ps1` · `win-leak-cycle.ps1` · `bench-boost.ps1`.
- macOS: `ps -o rss,%cpu` · `/usr/bin/time -l` · `NSQL_TRACE_MEM=1`(`phys_footprint`) · `scripts/mac-capture.sh`.
- Linux: `scripts/linux-perf-all.sh -H <격리 홈> -D <데이터> -o <결과>`(기동 5회 · 시나리오 12 · 릭 4 · CLI) · `linux-startup.sh` · `linux-probe.sh` · `linux-leak.sh` · `strace -f -ttt`(기동 타임라인) · `scripts/linux-all-tests.sh`(게이트 + 실서버 통합).
- 공통: Release로 · 빌드 직후 첫 실행은 버림 · A/B는 같은 시각에 번갈아 · `NSQL_TRACE_FRAMES=1`(`[startup]` · `[frames]` · `[load]`) · nexa-ui `cargo run --release -p nexa-ctl --example bench_editor <줄> all` · `bench_undo` · nexa-sql `--example bench_vars`.
