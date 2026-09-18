# 55 — 편집기 입력 지연(캐럿 이동 속도) 조사·실측·개선 (사용자 09-19)

> 요구: "마우스·키보드 커서 이동이 VS Code·Sublime보다 느린 것 같다 — 기술 검토와 실측으로 개선 방법을 deep research".
> 결론: **캐럿 한 번 움직일 때 창 전체·문서 전체를 다시 그렸고**, 그 안에 **문서 크기에 비례하는 일**이 여럿 있었다.
> 1차 개선(09-19)으로 2 MB 본문에서 페인트 **195 → 13 ms**, ↓ 키 **7.5 → 0.00 ms**. 남은 구조적 항목은 §5.

## 1. 경로 — 키 하나가 화면에 닿기까지

```
winit KeyboardInput → App::window_event → ctl_event → route → TextBox::on_event(↓)   … A
  → Invalidations(무시) → request_redraw                                              … B
about_to_wait: refresh_dirty()(모든 탭 본문 비교) · tick 여러 개                         … C
RedrawRequested → App::paint(): 크롬·편집기·그리드·탐색기·팝업 **전부** → buf.present()  … D
```

- **B**: `Invalidations`는 컨트롤이 채우지만 호스트가 버린다 → 부분 무효화 없음(창 전체).
- **D**: macOS softbuffer CG 백엔드는 프레임마다 `vec![0; w*h]`(Retina 2750×1890 ≈ **20 MB 할당+0 채움**)을 새로 만든다(`age()`=0 · 손상 영역 불가) → 4~10 ms 바닥. 실측 `present 11 ms`(빈 편집기 · `NSQL_TRACE_FRAMES=1`).

## 2. 문서 크기에 비례하던 일(1차에서 제거)

| # | 위치 | 문제 | 조치 |
|---|---|---|---|
| 1 | `TextBox::paint_multiline` 내용 폭 | **모든 줄**의 폭을 매 프레임 폰트 엔진으로 측정(줄당 mutex 2·HashMap 2) — 가장 큰 항목 | 본문 세대(`EditState::rev`)·글꼴·wrap 키로 **캐시**(`ml_width_cache`) |
| 2 | `logical_lines` | 줄마다 `String` 할당 · 페인트당 **2번** 호출 | `(usize, &str)` 슬라이스 · wrap 아니면 1번 |
| 3 | `ml_move_vert`/`ml_line_edge`/`caret_column`/스마트 Home/⌘End | 키마다 `text()`(2 MB String) + `chars().collect()`(Vec<char>) | `EditState::chars()` 슬라이스(복사 0) |
| 4 | `paint_multiline` 본문 | `text()` + `chars().collect()` 두 벌 | preedit 없으면 슬라이스 · 있을 때만 String |
| 5 | `Editors::is_dirty`(`about_to_wait`의 `refresh_dirty` · 이벤트 루프마다 **모든 탭**) | 탭마다 본문 String 생성 + 비교 | 세대 캐시 `dirty_cache`(저장 시 무효화) |
| 6 | 상태줄 `caret_line_col`/`selection_summary` | 본문 String 생성 | 캐럿 앞 슬라이스만 |

## 3. 실측 — `cargo run --release -p nexa-ctl --example bench_editor [lines]`(오프스크린 1600×1000 · 시스템 고정폭 · mac M-시리즈)

| 본문 | 페인트(전) | 페인트(후) | ↓ 키(전) | ↓ 키(후) | Home/End(전) | (후) |
|---|---|---|---|---|---|---|
| 20 KB · 200줄 | 5.75 ms | **3.30 ms** | 0.041 | **0.000** | 0.061 | **0.000** |
| 200 KB · 2,000줄 | 21.8 ms | **4.9 ms** | 0.47 | **0.000** | 0.67 | **0.000** |
| 2 MB · 20,000줄 | 195 ms | **12.9 ms** | 7.46 | **0.000** | 10.5 | **0.000** |

(전 = 09-19 오전 코드 · 후 = 1차 개선. 페인트에는 글리프 블렌딩·미니맵 포함. 실제 프레임은 여기에 크롬·그리드·탐색기 + present ≈ 11 ms가 더해진다.)

**해석**: 종전엔 줄 수에 거의 정비례(~9.5 ms/1,000줄) → 지금은 2 MB에서도 13 ms(남은 O(n) = 표시용 String 1벌 · 줄 분해 · 캐럿 줄 세기 · 하이라이터 상태 따라잡기). 200 KB 이하 스크립트는 페인트 5 ms 안쪽이라 체감은 **present(창 전체 20 MB)와 60 fps 핀**이 지배한다.

## 4. VS Code · Sublime과의 구조 차이(왜 거기가 빠른가)

- VS Code: DOM/GPU 합성 · 보이는 줄만 레이아웃 · 줄 단위 토큰 캐시 · 캐럿은 별도 레이어(본문 리페인트 없음).
- Sublime: 자체 GPU/CPU 렌더러 · 줄 레이아웃 캐시 · 부분 무효화(dirty rect) · 문서 크기와 무관한 프레임 비용.
- 우리: CPU 래스터(설계상 의도 · DR-1) — 문제는 래스터가 아니라 **"전체 다시 그리기 + 문서 비례 일"**이었다. 래스터 자체는 편집기 본문 2~4 ms 수준.

## 5. 남은 항목(우선순위 · T-136)

| # | 항목 | 기대 |
|---|---|---|
| D6 | **픽셀 버퍼를 앱이 보관**(softbuffer의 프레임당 20 MB 할당 제거 · CALayer/CGImage 보존 버퍼) | present 11 → ~3 ms |
| D5 | **부분 무효화**(`Invalidations` 합집합 → 클립 · 겹치지 않는 층은 건너뜀) — D6 뒤 | 캐럿 이동 프레임 < 1 ms |
| D12 | 휠 뒤 2초 스크롤바 표시 때문에 60 fps 전체 리페인트 핀 → 페이드 전환(300 ms)만 프레임 | 스크롤 직후 타이핑 체감 |
| D16 | 자동 반복 중 입력이 밀려 있으면 페인트 1회로 합침 | 누르고 있을 때 따라옴 |
| D7 | 하이라이터 상태(블록 주석) 줄별 캐시 — 지금은 뷰포트 위 모든 줄을 매 프레임 재토큰화 | 깊이 스크롤 시 수 ms |
| D9/D10 | 글꼴 경로(글자당 mutex·HashMap 4회) · 글리프 아틀라스 + LRU | 본문 2~4× |
| D11 | 미니맵 키 해시를 세대 기반으로 | 0.2~0.5 ms |
| D13 | `CursorMoved` 합치기 · 커서 모양은 바뀔 때만 | 드래그 선택 매끄럽게 |
| D15 | 캐럿 깜빡임을 편집기 띠만 다시 그리기(D5 뒤) | 유휴 CPU |

계측: `NSQL_TRACE_FRAMES=1`(60프레임 구간별 ms) · `examples/bench_editor`(nexa-ctl · 편집기 단독). 39 §6 게이트에 "2 MB 본문 페인트 ≤ 20 ms · ↓ 키 ≤ 0.1 ms"를 추가한다.
