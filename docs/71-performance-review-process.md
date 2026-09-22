# 71 · 성능 종합 점검 프로세스 — 규정 대상 · 순서 · 판정 (사용자 09-22)

> **요청**(사용자 09-22): *"성능 평가해달라는 요청에 대해서 규정된 대상, 순서에 따라 종합 점검이 진행되도록 점검 프로세스 정리."* + 앞선 같은 지시의 점검 차원 열거 — *"메모리 · 실행 속도 · 동적 라이브러리 개수 · 정적 라이브러리 개수 및 최종 용량 · 구성 파일 개수 · 메모리 회수 · 로딩된 상태에 대한 데이터 구조 및 재사용성 · 파일 및 메모리 사용량 최적화 · 속도 · 실행 · 기능 · 용량 · I/O 속도 · 병목 · 프로세스 & 쓰레드 적용 · 기능이 메인 프로세스에 영향을 미치는지."*
>
> **이 문서의 자리**: [26 성능 아키텍처](26-performance-architecture.md)는 *원칙과 실측 기록*, [39 자원 거버넌스](39-resource-governance.md)는 *부하원 원장과 설정*, [65 OS별 비교](65-cross-os-performance-comparison.md)는 *3-OS 대조*다. 이 문서는 그 셋을 **언제 · 어떤 순서로 · 무엇을 보고 판정하는가**로 묶은 **절차서**다. "성능 점검해줘"라는 한 마디가 오면 여기 §2의 순서를 그대로 돈다.

---

## 0. 원칙 다섯 줄

1. **측정 먼저 · Release로 · 같은 시각 A/B**. 빌드 직후 첫 실행은 버린다([61 §4](61-core-design-and-working-rules.md)).
2. **격리 설정 폴더**(`NSQL_HOME`) + **기동 명령**(`NSQL_STARTUP_CMD`)으로만 몬다 — 키·마우스 주입 없음, 사용자 설정 폴더 불변([61 §2-4](61-core-design-and-working-rules.md)).
3. **절대값이 아니라 ① 같은 기기의 전/후 ② 예산([26 §5](26-performance-architecture.md) · [39 §2](39-resource-governance.md)) ③ 형상**(셀당 바이트 · 피크/상주 비율 · 회수 여부)을 본다.
4. **차원을 빠뜨리지 않는다** — §1의 12개는 매번 답이 있어야 한다(해당 없음도 답이다).
5. **점검은 원장을 갱신하며 끝난다** — 새 부하원은 [39 §3](39-resource-governance.md)에, 수치는 [26 §7](26-performance-architecture.md)·[65](65-cross-os-performance-comparison.md)에, 남은 것은 [TODO](TODO.md)에.

---

## 1. 규정 대상 — 12 차원(원장)

| # | 차원 | 무엇을 재나(지표) | 단계 | 원장·기준 |
|---|---|---|---|---|
| D1 | **최종 용량** | GUI/CLI 실행 파일 바이트 · PE/ELF 섹션별(`.text`/`.rdata`) · 드라이버 feature별 증분 | A | [64 §3-1](64-dbms-clients-and-driver-packaging.md) · 예산 30 MB(DR-17) |
| D2 | **정적 라이브러리 개수** | 링크되는 crate 수(워크스페이스/형제/외부 구분) | A | DR-3 예외 원장([10](10-decision-record.md)) — 외부 crate가 늘면 근거를 적는다 |
| D3 | **동적 라이브러리 개수** | 기동 때 무조건 올리는 import DLL/so · 지연 import · 런타임 `LoadLibrary`/`dlopen` · 실제 로드된 모듈 | A | 기동 시간의 원천 · Oracle OCI만 런타임 로드([64](64-dbms-clients-and-driver-packaging.md)) |
| D4 | **구성 파일 개수** | `NSQL_HOME` 아래 실제로 생기는 파일·폴더와 크기 · 언제 생기나 | A | [21](21-connection-profiles.md) · [33](33-distribution-and-packaging.md) — exe 옆에 쓰지 않는다(DR-27) |
| D5 | **기동 속도** | 창이 보이기까지 ms · 안정까지 CPU ms · `[startup]` 구간 | B | 예산 300 ms([39 §2](39-resource-governance.md)) |
| D6 | **메모리(상주·피크)** | 시나리오별 Private(win) / RssAnon·RSS(linux) / footprint(mac) · 피크 워킹셋 | C | [26 §7](26-performance-architecture.md) · 힙(창 버퍼 제외) ≤ 6 MB |
| D7 | **메모리 회수** | 탭·결과를 놓은 뒤 기준선 복귀 여부 · `memtrim` 전/후 | C·E | [59 §2](59-large-file-handling.md) |
| D8 | **반복 누수** | 같은 동작 N주기의 **뒤 절반 기울기**(MB/주기) | E | 0에 수렴해야 한다(힙 조각은 예외로 설명) |
| D9 | **실행·입력 속도** | 글자 입력 ms · 평상시 그리기 ms · 되돌리기 ms · 문장 준비 µs · 프레임 ms | F | 프레임 ≤ 8 ms · 입력 ≤ 16 ms |
| D10 | **I/O 속도** | 파일 열기(첫 페인트까지) · 적재 CPU · UI가 멎는 시간 · 프로젝트 스캔 · 설정 읽기 | B·G | UI 정지 0([61 §2](61-core-design-and-working-rules.md) 불변식) |
| D11 | **프로세스 · 스레드** | 이름 있는 스레드 원장(언제 생기고 언제 죽나) · 자식 프로세스 · 상주 스레드 수 | C·G | [39 §3](39-resource-governance.md) 등재 = 끄는 키 |
| D12 | **메인 프로세스 영향** | 그 기능이 UI 스레드를 잡는가 · 창 전체를 막는가 · 그 탭에만 갇히는가 | G | "오래 걸리는 일은 그 대상(탭) 하나에 가둔다"([61 §2](61-core-design-and-working-rules.md)) |

> **D13(횡단) 향상 모드**: 위 전부를 `perf.boost` off/on으로 한 번 더 본다(단계 D) — 자세히 [45](45-perf-boost-benchmark.md).
> **D14(횡단) 데이터 구조·재사용성**: 적재된 상태를 무엇이 몇 벌 들고 있나 · 복사인가 공유인가 · 세대(rev) 열쇠가 있나 — 수치가 아니라 **구조 심사**이고 [39 §3-7](39-resource-governance.md)의 프로파일 표에 적는다.

---

## 2. 순서 — A부터 G까지

```
A 인벤토리 ─→ B 기동 ─→ C 시나리오 상주 ─→ D 향상 모드 A/B ─→ E 누수 주기
   (용량·의존·구성)      (기동 CPU)        (메모리·스레드)      (같은 시나리오 off/on)   (기울기)
                                   └─────→ F 벤치(입력·그리기·되돌리기·엔진)
                                   └─────→ G 병목 추적(프레임·strace·[startup]·I/O)
```

**왜 이 순서인가**

1. **A가 먼저다** — 용량·의존·구성 파일은 앱을 띄우지 않고 재고, 여기서 늘어난 것이 보이면 뒤 단계의 해석이 달라진다(예: crate가 늘었다 → 기동·용량 회귀의 1차 용의자).
2. **B는 C보다 앞** — 기동 경로의 회귀는 모든 시나리오의 바닥을 올린다.
3. **C 다음에 D** — 향상 모드는 *같은 시나리오의 차이*로만 의미가 있으므로 기준(off)이 먼저 있어야 한다.
4. **E는 C 뒤** — 기준선을 모른 채 주기를 돌리면 "오르는 중"인지 "원래 그 값"인지 못 가른다.
5. **F·G는 C·D와 독립** — 앱이 아니라 벤치·트레이스로 재므로 언제든 끼워 넣을 수 있다. 다만 **회귀가 보이면 G로 내려가 원인을 지목**하고 끝낸다(숫자만 남기고 끝내지 않는다).

**생략 규칙**: 문서·주석만 바뀐 변경은 A만. UI 배치만 바뀌면 A·C. 자료 구조·적재 경로·스레드·설정 기본값을 건드렸으면 **전 단계**.

---

## 3. 단계별 방법 · 도구 · 판정

실행기: **Windows `scripts/win-perf-all.ps1`** · **Linux `scripts/linux-perf-all.sh`** · macOS는 단계별 도구를 손으로(§7).

```powershell
pwsh -NoProfile -File scripts/win-perf-all.ps1 -HomeDir C:\tmp\nsql-home -DataDir C:\tmp\nsql-data -Out target\perf-win
pwsh -NoProfile -File scripts/win-perf-all.ps1 -HomeDir ... -DataDir ... -Out ... -Stages boost   # 한 단계만
```

### A. 인벤토리 — `scripts/win-inventory.ps1`

| 재는 것 | 방법 | 판정 |
|---|---|---|
| D1 용량·섹션 | PE 헤더 직접 파싱(의존 0 · `dumpbin` 불필요) | 예산 30 MB · 전 커밋 대비 +5 % 이상이면 근거를 적는다 |
| D2 crate 수 | `cargo tree -p <bin> --edges normal` 중복 제거 | 외부 crate가 늘면 DR-3 예외 원장에 등재됐는가 |
| D3 동적 | PE import(1번) + delay import(13번) 디렉터리 · 기동 뒤 `Process.Modules` | import가 늘면 기동 경로를 다시 본다 · delay는 0이 정상(지연 로드를 쓰지 않는다) |
| D4 구성 파일 | 격리 홈으로 한 번 기동·종료한 뒤 재귀 열거 | 새 파일이 생겼으면 [33](33-distribution-and-packaging.md)·[21](21-connection-profiles.md)에 적혔는가 · 지울 수 있는가 |
| 설정 키 | `nsql config list all` · `list perf` | 새 부하원에 끄는 키가 있는가([39 §3](39-resource-governance.md)) |

### B. 기동 — `scripts/win-startup-probe.ps1` · `NSQL_TRACE_FRAMES=1`의 `[startup]`

창 핸들까지 ms · `-SettleSecs`까지 CPU ms · 그때의 Private/핸들/스레드를 `-Runs`번 재고 **중앙값**. 구간이 필요하면 `[startup]` 누적 로그로 어느 단계가 늘었는지 지목한다(Linux 91차의 글꼴·아이콘 병목을 이렇게 찾았다 — [26 §7-7](26-performance-architecture.md)).

### C. 시나리오 상주 — `win-perf-all.ps1 -Stages scenarios`(내부적으로 `win-big-probe.ps1`과 같은 표본)

**규정 시나리오**(늘리기만 하고 빼지 않는다 — 시계열 비교가 목적):

| # | 시나리오 | 기동 명령 | 무엇을 보나 |
|---|---|---|---|
| 1 | 기동(로그인 + 메인) | — | 바닥값 |
| 2 | 접속(SQLite) | 인자 `Local` | 워커·탐색기 스레드 |
| 3 | 보조 창 4(로그·트랜잭션·세션·설정) | `view.log,view.txlog,view.sessions,edit.prefs` | 창당 프레임버퍼 · GDI/USER |
| 4 | 2 MB 스크립트 | `open:<파일>` | 편집기 캐시 · 유휴 CPU |
| 5 | 결과 10만 행 × 5열 | `open:<질의>,@after:2000:run.all` | 셀당 바이트(≈ 61 B) |
| 6 | **프로젝트 패널**(09-22 신설) | `project.load:<파일>,@after:1500:view.project` | 트리 노드 · 지연 열거 |
| 7 | 확장 패널 | `view.extensions` | 네트워크 스레드가 잠드는가 |

각 시나리오에서 Private·WS·피크·핸들·GDI·USER·스레드·유휴 CPU(6초)를 찍는다.

### C-2. 메모리 **회수** 시험(D7) — 규정 대상(사용자 09-22 "편집기·결과 그리드·대용량·다중 결과 그리드·커서 회수 시험이 없었다")

E(누수 주기)는 *되풀이의 기울기*를 보고, 여기는 **놓은 뒤 기준선으로 돌아오는가**를 본다. 각 항목은 ① 기준선(기동+접속 Private) ② 올림(대상을 만든 뒤 Private) ③ 놓음(닫기·비움 뒤 즉시) ④ `memtrim` 뒤(59 §2-1 · 유휴 회수 주기 지나서) 네 점을 찍고, **④ − ① ≤ 허용치**면 통과. 기동 명령은 C의 것과 같은 규약(`@after:`).

| # | 대상 | 올림 | 놓음 | 무엇이 남으면 안 되나 | 허용치 |
|---|---|---|---|---|---|
| R1 | **편집기 탭**(2 MB · 70만 줄) | `open:<2MB>` | 탭 닫기(`file.close_tab`) | `TextBuf`·줄 캐시·되돌리기 기록·구문 캐시(59 §6 · 60) | +2 MB |
| R2 | **결과 그리드**(10만 행 × 5열) | `open:<질의>,@after:2000:run.all` | 결과 탭 닫기 / 새 질의로 덮음 | `ResultData` Arc 세그먼트·`View`·텍스트 캐시(DR-33 · 43 §6) — 참조 하나라도 남으면 세그먼트 전체가 남는다 | +3 MB |
| R3 | **대용량 파일**(65 MB · L1/L2) | `open:<65MB>`(적재 완료 대기) | 탭 닫기 · 적재 중 Esc 취소 둘 다 | 자리 탭·`PreparedText`·적재 스레드 버퍼(59 §5) · 취소 경로의 반쪽 결과 | +8 MB |
| R4 | **다중 결과 그리드**(결과 탭 8개 · 각 1만 행) | 질의 8개 순차 실행(43 결과 다중 탭) | 탭 하나씩 닫기 → 전부 닫기 | 탭별 `ResultData` · 고정 탭이 아닌 것의 자동 퇴거(43 §7) · 탭 목록 메뉴의 참조 | +3 MB |
| R5 | **커서 결과**(Oracle/PG refcursor 출력 뒤 · T-150) | `examples/oracle-refcursor-pkg.sql` 실행 → 커서 결과 탭 | 결과 탭 닫기 · 접속 해제 | 클라이언트에 남은 **페치 대기 커서**(RowSource · 아직 안 읽은 서버 커서 핸들) · 접속 해제 뒤 워커의 커서 소유 | +1 MB · 서버 열린 커서 0(`v$open_cursor`/`pg_cursors`) |
| R6 | 결과 + 편집기 동시(R1+R2 반복 5회) | 위 둘을 번갈아 | 전부 닫기 | 서로의 캐시가 상대를 붙드는가(텍스트 캐시 예산 43 §6) | +4 MB |

판정 뒤 26 §7에 "회수 표"로 남긴다(전/후 4점 · 기기 · 판). **아직 실행하지 않았다**(사용자 09-22 "절차에 반영만 · 테스트는 나중에").

### D. 향상 모드 A/B — `-Stages boost`

**같은 시나리오 7개를 `perf.boost` off → on 순으로 번갈아** 돌리고 차이를 낸다. 끝나면 반드시 `off`로 되돌린다(스크립트가 한다). 판정: 향상 모드는 **동작 결과를 바꾸지 않으면서** 유휴 CPU·스레드·핸들·상주를 줄여야 한다. 결과·행 수·타임아웃이 달라지면 강제 표에 잘못 들어간 키가 있다는 뜻이다([39 §4-6](39-resource-governance.md) 제외 목록).

### E. 누수 주기 — `scripts/win-leak-cycle.ps1`

같은 동작 묶음을 N번 되풀이하고 **뒤 절반의 기울기**(MB/주기)를 본다. 규정 주기: 큰 파일 열기/닫기 · 10만 행 재실행 · 보조 창 여닫기 · 프로젝트 패널 여닫기 · 새 탭/닫기. 첫 1~2주기의 상승은 캐시가 차는 정상이다.

### F. 벤치 — nexa-ui `bench_editor` / `bench_undo` · nsql-run `bench_vars`

앱을 띄우지 않고 재는 순수 비용. 70만 줄·4만 줄의 입력/그리기, 되돌리기(낱말·붙여넣기), 문장 준비(µs). **자료 구조를 바꿨으면 단순 모델과의 난수 대조 테스트**를 같이 돌린다([61 §2](61-core-design-and-working-rules.md)).

### G. 병목 추적(회귀가 보일 때만)

| 증상 | 먼저 볼 것 | 도구 |
|---|---|---|
| 유휴 CPU가 높다 | 어느 창·어느 구간이 매 프레임 다시 그리나 | `NSQL_TRACE_FRAMES=1`의 `[frames]` 구간별 ms |
| 기동이 느리다 | 어느 초기화가 늘었나 | `[startup]` 누적 · Linux는 `strace -f -ttt` |
| 입력이 느리다 | 경로인가 그리기인가 표시인가 | `tmark` 입력→present 한 줄(`win-latency-probe.ps1`) |
| 파일 열기가 느리다 | 사본이 몇 벌인가 · 첫 페인트가 무엇을 재나 | `[load]` · 피크/상주 비율 |
| 메모리가 안 준다 | 누가 들고 있나 | 소유 구조 감사 + `memtrim` 전/후([59 §2-1](59-large-file-handling.md)) |

---

## 4. 판정 기준

| 지표 | 기준 | 출처 |
|---|---|---|
| 기동(창까지) | ≤ 300 ms | [39 §2](39-resource-governance.md) |
| 프레임 | ≤ 8 ms(120 fps 여유) · 드래그 중에도 | [39 §2](39-resource-governance.md) |
| 입력(글자 하나, 큰 파일) | ≤ 16 ms | [59](59-large-file-handling.md) |
| 유휴 CPU | 0에 가깝게(창 4개 포함 6초에 ≤ 100 ms) | [26 §7-3](26-performance-architecture.md) |
| 유휴 상주(창 버퍼 제외 힙) | ≤ 6 MB | [26 §7-2](26-performance-architecture.md) |
| 결과셋 | 셀당 ≈ 61 B · 10만 행 < 150 MB | DR-17 |
| 누수 기울기 | ≈ 0(설명 가능한 힙 조각만) | [26 §7-5](26-performance-architecture.md) |
| 실행 파일 | ≤ 30 MB | DR-17 |
| UI 정지 | 0(오래 걸리는 일은 그 탭에 갇힌다) | [61 §2](61-core-design-and-working-rules.md) |

**회귀 판정**: 같은 시각 A/B에서 ① 상주 +5 % 또는 +1 MB ② 유휴 CPU 2배 ③ 기동 +20 % ④ 벤치 항목 +20 % 중 하나라도 넘으면 **원인을 지목할 때까지 단계 G**로 내려간다. 넘지 않아도 "왜 늘었는지 설명할 수 있는가"가 진짜 기준이다.

---

## 5. 기능을 추가할 때 — 등재 체크리스트

새 기능을 넣은 커밋은 아래를 채운 뒤에야 "끝"이다(§1의 12차원을 기능 하나에 적용한 것).

1. **부하원인가?** 스레드·소켓·디스크·매 프레임 재그리기·캐시를 쓰면 [39 §3](39-resource-governance.md)에 등재하고 **끄거나 상한을 두는 설정 키**를 만든다(사용자 09-16 규칙).
2. **향상 모드 판정** — 동작 결과에 영향이 0이면 [39 §4-6](39-resource-governance.md)의 `BOOST` 표 후보, 결과가 바뀌면 **개별 스위치로만**(09-21 결정).
3. **스레드·프로세스** — 이름을 주고(`Builder::new().name(...)`) 언제 죽는지 적는다. 상주라면 왜 상주인지.
4. **메인 프로세스 영향** — UI 스레드에서 몇 ms를 쓰나 · 오래 걸리면 그 탭에 가두는가.
5. **데이터 구조·재사용** — 적재된 상태를 몇 벌 들고 있나 · 복사 대신 공유(`Arc`·세그먼트·인덱스 뷰 — DR-33)인가 · 세대 열쇠로 캐시를 무효화하나.
6. **구성 파일** — 새 파일을 만들면 이름·위치·크기 상한·수명(언제 지우나)을 [33](33-distribution-and-packaging.md)에.
7. **용량·의존** — 외부 crate를 더했으면 DR-3 예외 원장에.
8. **[39 §3-7 기능 성능 프로파일](39-resource-governance.md)에 한 줄** — 위 항목을 압축한 원장 행.
9. **네트워크를 만들면** [26 §8](26-performance-architecture.md) 6항목 점검.
10. **측정** — 해당 시나리오가 없으면 §3 C의 규정 시나리오에 추가한다(늘리기만 한다).

---

## 6. 산출물 · 기록 위치

| 무엇 | 어디에 |
|---|---|
| 원자료(수치 · stderr) | `target/perf-<os>-<날짜>/`(저장소에 넣지 않는다) |
| 시나리오 표 · 해석 | [26 §7](26-performance-architecture.md)에 날짜·차수로 새 절 |
| 3-OS 대조 | [65](65-cross-os-performance-comparison.md) |
| 향상 모드 | [45](45-perf-boost-benchmark.md) |
| 새 부하원 · 기능 프로파일 | [39 §3](39-resource-governance.md) |
| 발견·조치·남은 것 | `docs/journal/<날짜>.md` → [DEVLOG](DEVLOG.md) → [STATUS](STATUS.md) → [TODO](TODO.md) |

---

## 7. OS별 도구 대응

| 단계 | Windows | macOS | Linux |
|---|---|---|---|
| A 인벤토리 | `win-inventory.ps1`(PE 파싱) | `otool -L` · `size` · `du` | `ldd` · `readelf -d` · `size` |
| B 기동 | `win-startup-probe.ps1` | `/usr/bin/time -l` + `[startup]` | `linux-startup.sh` · `strace -f -ttt` |
| C 시나리오 | `win-perf-all.ps1 -Stages scenarios` · `win-big-probe.ps1` | `ps -o rss,%cpu` · `NSQL_TRACE_MEM`(`phys_footprint`) · `vmmap` | `linux-probe.sh`(`/proc/<pid>/status` RssAnon) |
| D 향상 모드 | `-Stages boost` · `bench-boost.ps1` | (미구현 — 손으로) | (미구현 — 손으로) |
| E 누수 | `win-leak-cycle.ps1` | (미구현) | `linux-leak.sh` |
| F 벤치 | `cargo run --release -p nexa-ctl --example bench_editor` | 같음 | 같음 |
| G 병목 | `NSQL_TRACE_FRAMES` · `win-latency-probe.ps1` | `sample` · `NSQL_TRACE_FRAMES` | `strace` · `NSQL_TRACE_FRAMES` |

지표가 OS마다 다르다는 점을 잊지 않는다: Windows **Private Bytes**(창 프레임버퍼 포함) ↔ macOS **`phys_footprint`**(레티나 버퍼 2~3벌 포함) ↔ Linux **RssAnon**(창 버퍼는 공유 매핑이라 제외) — 절대값 비교가 아니라 형상 비교다([65 §3](65-cross-os-performance-comparison.md)).

---

## 8. 이번 실행(2026-09-22 · Windows 93차 뒤 · 94차)

**전 단계 A~F를 돌렸다**(G는 회귀가 없어 들어가지 않았다). 결과 = [26 §7-8](26-performance-architecture.md)(A 인벤토리 · B 기동 · C 시나리오 · E 누수 · F 벤치) · [45 §4](45-perf-boost-benchmark.md)(D 향상 모드) · [39 §3-7](39-resource-governance.md)(기능 프로파일 · 스레드 원장).

| 단계 | 한 줄 결과 |
|---|---|
| A 인벤토리 | GUI 10.07 MiB · crate 165(외부 142) · **동적 import 27 = 전부 OS 제공 · 지연 0** · 구성 파일 3개 294 B · 설정 키 343(부하원 25 · 강제 36) |
| B 기동 | 창까지 **47 ms**(89차 141) · 3초 CPU 156 ms(500) — Linux에서 고친 아이콘·글꼴 캐시가 3-OS 공통이라 Windows에 그대로 들어왔다 |
| C 시나리오 | 기동 11.10 · 접속 9.82 · 2 MB 21.14(89차 27.45) · 10만 행 39.43 · **프로젝트 패널 = 접속 +0.07 MB · CPU 0** · 확장 패널 10.20 |
| D 향상 모드 | 유휴 CPU가 어느 상태에서도 0 · 2 MB 스크립트 −26 % · 확장 패널 CPU −86 % · **결과 데이터는 불변**(설계 확인) |
| E 누수 | 파일 0.008 · 10만 행 0.136(89차 0.26~0.30) · 로그 창 0.000 · **프로젝트 패널 0.000** MB/주기 |
| F 벤치 | 70만 줄 입력 3.4/4.1 ms · 4만 줄 전 기능 8.8 ms · 되돌리기 0.08 ms · 문장 준비 1.3 µs — 전부 이전과 같다 · **70만 줄 전 기능 84 ms는 새 데이터**(앱은 그 크기에서 L2로 기능을 끈다) |

**이번에 새로 재지 않은 차원**(정직하게 남긴다): **D7 회수 시험 C-2(R1~R6)** — 이번 실행 때는 목록 자체가 없었다(사용자 09-22 지적 뒤 신설 · 실행은 다음 점검) · **D10 I/O 속도** — 파일 열기의 첫 페인트·적재 CPU·UI 정지 시간은 [59 §5-3·§6-2](59-large-file-handling.md)의 84차 실측(65 MB 여는 CPU 0.33초 · UI 정지 0 · 첫 페인트 0.15~0.2초)을 그대로 쓴다. 이번 F 단계의 70만 줄 `set_text` 58 ms · 첫 페인트 100 ms가 그 근사치이고 같은 자릿수다. 적재 경로를 건드리는 변경이 오면 C·G에서 `[load]`와 함께 다시 잰다. **G 병목 추적**은 회귀가 없어 들어가지 않았다.

**측정 도구에서 나온 교훈 둘**(둘 다 고쳤다): ① PowerShell은 변수 이름의 대소문자를 구분하지 않아 `$stages = $Stages.Split(...)`가 `[string]` 파라미터로 되돌아가 **모든 단계가 조용히 건너뛰어졌다**(결과 파일이 비어야 알아챘다) ② 앞 시나리오가 큰 결과를 들고 있으면 종료에 수 초가 걸리는데 400 ms만 기다려 다음 표본이 "이미 죽은 프로세스"의 0을 냈다. → **0은 값이 아니라 측정 실패다**: 실행기는 죽은 표본을 1회 다시 재고(`Sample-Retry`), 종료를 `WaitForExit`로 기다린다. 09-19의 "계측이 지연을 만들지 않게"와 같은 종류의 함정이다.

이 문서가 생기기 전까지 점검은 매번 "그때 필요한 것"만 골라 돌았다(09-14 메모리 · 09-17 향상 모드 · 09-19 큰 파일 · 09-21 종합 · 09-22 Linux 전수). 앞으로는 §2의 순서가 기본값이고, 생략할 때 **왜 생략했는지**를 보고에 적는다.
