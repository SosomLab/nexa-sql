# 48 · 실행 로그 아키텍처 — 상세 계측(전송·첫 응답·페치·처리·렌더) · 개발자 모드 · **성능 영향 0에 가까운 게이트**(deep research)

> **상태**: ✅ 1차 구현(09-17 52차 · 사용자 요청 "실행 로그에 전송 시각/속도/전송량 · 첫 응답 시각 · 페치 시작~완료+네트워크 · 수신 뒤 처리 시간 · 렌더 시작/종료 · 로그 때문에 느려지면 안 됨 · 속도 향상 연동 · 레이어×메시지 유형 설정 · 로그 창 개발자 모드 · 필요할 때만 로직 구동 · 분기 예측/커버리지까지 고려한 아키텍처 deep research").
> 관련: [26 성능 구조](26-performance-architecture.md)(Timeline 단계) · [32 서버 메시지·실행 중 로그](32-server-messages-and-live-log.md) · [39 자원 거버넌스](39-resource-governance.md)(부하원 · `perf.boost`) · [42](42-db-error-normalization.md).

## 0. 결론 여섯 줄

1. **끄면 비용 = 원자 정수 load 1회 + 예측되는 분기 1개**(≈ 0.3 ns · 함수 호출 0 · 문자열 0 · `Instant::now()` 0 · 할당 0). 이것이 리눅스 커널 static key(jump label)와 `tracing`의 callsite 캐시가 도달한 하한이며, 안정 Rust에서 코드 패칭 없이 낼 수 있는 최선이다.
2. **켜져도 뜨거운 경로에는 "기록"만**: 메시지 조립(i18n · 포맷 · 문자열)은 게이트 뒤 **느린 경로 함수**(`#[cold] #[inline(never)]`)에서만 — 코드 배치가 뜨거운 경로에서 분리돼 i-cache·분기 예측기를 더럽히지 않는다.
3. **한 벌의 상태 = 원자 마스크 `u32`(층 8 × 수준 4)**: 설정 `log.dev_mode` × `log.dev_layers`가 마스크를 만들고, `perf.boost`는 `dev_mode`를 강제로 끈다(마스크 0). 소비자(로그 창)는 개발자 모드가 꺼지면 상세 수준 줄을 **표시에서도** 숨긴다.
4. **계측값은 이미 있는 것을 재사용**: 단계별 소요는 `nsql_core::Timeline`(드라이버·러너가 이미 잰다) · 전송량은 `ResultSet::approx_bytes`/`FetchProgress.bytes` · 시각은 이벤트 도착 시점의 `now_local()`(게이트 뒤에서만) — 새 타이머 0.
5. **생산자는 절대 막히지 않는다**: 로그 허브는 bounded 채널 `try_send`(가득 차면 버림) · 로그 창 버퍼는 링(`log.max_lines`) — 상세 줄이 많아져도 메모리 상한 고정.
6. **커버리지**: 게이트는 `wants(layer, level)` 한 함수로 모여 있어 테스트가 마스크를 켜고/끄고 두 갈래를 다 밟는다(`detail_mask_parse_and_gate`). 호출 지점마다 `if`가 늘어도 "분기 미실행" 경고는 매크로 한 곳 문제로 귀결된다.

## 1. Deep research — 저비용 로깅의 알려진 기법

| 기법 | 어디서 | 비용(끔) | 우리 적용 |
|---|---|---|---|
| **Static key / jump label** — 분기 자리를 `nop`으로 두고 켤 때 코드 패칭 | Linux `static_branch_unlikely()` · tracepoints | 0(패치 전) | 안정 Rust엔 없음 → **원자 load + 분기**로 대체(아래) |
| **전역 원자 레벨 + 인라인 검사** | `log` crate `max_level()` · `tracing` `LevelFilter::current()` · Java `isDebugEnabled()` | load 1 + cmp/branch(≈0.3ns) | ✅ `nsql_log::wants()` `#[inline(always)]` · `Relaxed` |
| **Callsite 캐시**(`Interest`) — 호출 지점마다 "영원히 끔/켬/매번" 3상태를 원자에 캐시 | `tracing-core` `Callsite`/`MAX_LEVEL` | 캐시 hit면 load 1 | 층×수준 32비트가 곧 callsite 표 — 별도 캐시 불필요 |
| **컴파일 타임 제거** | `log` `max_level_off` feature · `tracing` `release_max_level_*` | 0 | 후속: `cfg(feature = "devlog")` 없으면 `wants()`가 `const false`(릴리스 빌드 완전 제거 옵션) |
| **지연 포맷**(구조화 레코드 · 텍스트는 소비 시점) | `defmt`(임베디드 · 문자열 테이블) · ETW/LTTng 바이너리 레코드 · `tracing` `Event` 필드 | 레코드 복사만 | 부분 적용: 게이트 뒤에서만 문자열 조립(생산 스레드는 워커/UI · 실행 경로 대비 무시 가능). 완전 지연(레코드 구조체)은 §5 후속 |
| **비차단 전송 + 드롭 통계** | `tracing-appender` non-blocking · `slog-async` · `log4rs` | try_send | ✅ 기존 `LogHub`(bounded · `catch_unwind` 싱크) |
| **샘플링/스로틀** | Envoy/nginx 접근 로그 샘플 · `FetchProgress` 100ms | 상한 있는 생산 | ✅ 진행 로그 = 배치 100ms 간격(워커가 이미 제한) |
| **Cold path 분리** | `#[cold]`/`#[inline(never)]`(LLVM `cold` attr → 블록을 `.text.unlikely`로) · 커널 `unlikely()` | 뜨거운 경로 코드 크기 ↓ | ✅ `detail_push`가 `#[cold] #[inline(never)]` — `likely/unlikely` 힌트가 안정 Rust에 없어도 cold 함수 호출은 LLVM이 unlikely로 배치 |
| **시각 비용 최소화** | `Instant::now()` ≈ 20~25ns(QPC/vDSO) · `rdtsc` ≈ 7ns · 커널 `sched_clock` | — | 게이트 뒤에서만 `now_local()` · 단계 소요는 Timeline 재사용(추가 시각 0) |
| **분기 예측·커버리지** | 항상-거짓 분기는 예측기가 100% 맞힘 · 커버리지 도구(`llvm-cov`)는 미실행 갈래를 표시 | — | 게이트가 한 함수·한 매크로에 모여 있고 테스트가 양쪽을 밟는다(§0-6) |

핵심 인용(요지): `tracing`은 "disabled span/event의 비용은 원자 load 하나와 분기 하나"를 설계 목표로 명시한다(`tracing-core` Callsite/Interest 문서). Linux 커널은 static key로 그 분기마저 없앤다(`Documentation/staging/static-keys.rst`). `defmt`는 문자열을 바이너리에 남기지 않고 인덱스만 전송해 포맷 비용을 소비자로 옮긴다. 우리는 이 셋의 교집합을 취했다.

## 2. 모델 — 층 × 수준

| 층(`LogLayer`) | 무엇을 기록 | 원천 |
|---|---|---|
| `net` | 전송 시각 · 전송 바이트(문장 길이) · 첫 세그먼트 수신(바이트 · 속도) · `send`/`receive` 단계 | `RunEvent::Begin` · `ResultSet` · `Timeline` |
| `exec` | 첫 응답까지(서버 실행) | `Timeline` `Execute` |
| `fetch` | 페치 시작~완료 · 행 · 바이트 · **속도** · 전체 조회 진행/완료 | `Timeline` `Fetch/Receive` · `FetchProgress` · `Page{all}` |
| `load` | 수신 뒤 데이터 소스 구성(ResultData) · 렌더 준비 | 그리드 `load` 소요 |
| `render` | 렌더 시작 시각 → 종료 시각 · 소요 | 그리드 `paint`(스탬프는 게이트 켜졌을 때만) |
| `tx` · `meta` | 트랜잭션 · 탐색기/카탈로그(후속) | — |

| 수준(`LogLevel`) | 뜻 | 양 |
|---|---|---|
| `basic` | 지금까지의 기본 메시지(개발자 모드와 무관하게 표시) | 실행당 2~5줄 |
| `timing` | 단계 시각·소요·전송량·속도 | 실행당 4~7줄 |
| `progress` | 진행(페치 배치) | 100ms당 1줄 |
| `trace` | 호출 단위 추적(후속) | 많음 |

마스크 = `1 << (layer*4 + level)` · 설정 문법 `layer[:level+level]` 쉼표 · `*` = 전부 · 기본 `net,exec,fetch,load,render`(세 수준 모두).

## 3. 코드 경로

```rust
// nsql-log — 게이트(뜨거운 경로에 남는 전부)
static DETAIL_MASK: AtomicU32;
#[inline(always)] pub fn wants(layer, level) -> bool { DETAIL_MASK.load(Relaxed) & bit(layer, level) != 0 }

// nexa-sql — 호출 지점(매크로 · $make는 게이트 뒤에서만 평가)
dlog!(self, LogLayer::Fetch, LogLevel::Timing, {
    LogEntry::new(LogKind::Fetch, tf(Msg::LogDetFetchAll, &[&fmt_bytes(b), &speed_of(b, elapsed)])).rows(n).elapsed(elapsed)
});
// 느린 경로(호출 드묾 · 코드 배치 분리)
#[cold] #[inline(never)] fn detail_push(log_win, hub, layer, level, e) { e.at(layer, level) → hub.try_send · log_win.push }
```

- `LogEntry`에 `layer`·`level` 필드 — 기본 메시지는 `App/Basic`. 로그 창 `shown_entry` = 종류 필터 ∧ (개발자 모드 ∨ Basic).
- 그리드 렌더 스탬프: `paint()`에서 `perf_report && wants(Render, Timing)`일 때만 `now_local()` 2회(시작·종료) → `take_perf_report`로 호스트에.
- 설정: `log.dev_mode`(off · 로그 창 푸터 **개발자** 스위치와 동기) · `log.dev_layers`(문자열 · 로그 창 우클릭 ▸ 개발자 레이어 체크로 편집) · 종속 잠금 · **`perf.boost` 강제 표에 `log.dev_mode=off`**(39 §4).

## 4. 부하 점검(39 §6 게이트)

| 항목 | 끔 | 켬(timing) | 켬(progress) |
|---|---|---|---|
| CPU/실행 | load+분기 × 호출 지점 ≈ 10회 → < 10 ns | 문자열 ≈ 6줄 × ~1µs | +배치당 1줄(100ms 간격 · 워커가 제한) |
| 메모리 | 0 | 로그 창 링(`log.max_lines`) 안 | 같음 |
| 스레드 | 0 | 0(기존 허브 스레드) | 0 |
| 네트워크 | 0 | 0 | 0 |

## 5. 후속

- **완전 지연 포맷**: `LogRec { layer, level, stage, t0: u64, dur_ns, rows, bytes }`(32B Copy)를 링에 넣고 텍스트는 로그 창이 그릴 때 만든다 — 켠 상태의 문자열 비용까지 0으로. i18n 조립을 표시 시점으로 옮기는 것이 조건(현재 `LogEntry.message`는 문자열).
- **컴파일 타임 제거**: `nsql-log` feature `devlog`(기본 on) · off면 `wants()` = `false` 상수 → 릴리스 특수 빌드에서 상세 경로 코드 자체가 사라진다.
- `trace` 수준 실제 생산 지점(드라이버 호출 단위) · `tx`/`meta` 층 채우기 · 텍스트 보기 변환 시간(`load`에 합산).
