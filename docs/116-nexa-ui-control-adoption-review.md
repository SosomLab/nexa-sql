# 116 — nexa-ui 미사용 컨트롤 점검 · 편입/유지 판단(10-16 · 협업 조사)

> 사용자 10-16: "구현 컨트롤 중 nexa-ui를 사용하지 않은 대상이 있는지 확인하고, 대상을 nexa-ui에 편입시킬지 별도로 유지할지 판단" → 후속 "다른 프로젝트에서도 안 쓰는지 점검".
> 방법 = nexa-sql 소스 전수(하위 에이전트 읽기 전용 조사 · 협업이 nexa-ui export·주석과 대조) + nexa-ui 소비 저장소 4곳 사용처 대조(아래 §3 · 협업 스크립트). **판단은 권고이며 사용자 결정 대기.**

## 1. 결론(권고 순서)

1. **두 벌 정리(nexa-ui 것으로 교체)** — 토스트 · 명령 팔레트 · 설정 창 스플리터 · 손그림 버튼/스크롤바. nexa-ui 쪽이 nexa-sql 원본을 승격한 것이고 **nexa-dir3는 이미 nexa-ui 판을 쓴다**(§3) → 두 벌 유지의 비용만 남았다.
2. **공용 목록 컨트롤 신설(nexa-ctl)** — 가상 스크롤 + 선택 + 스크롤바를 갖춘 범용 목록이 없어 nexa-sql의 단축키·세션·변수·트랜잭션 로그·확장 창·순서 편집이 각자 구현(일부는 스크롤바 없음).
3. **범용 부품 편입** — `CopyBtn` · `OrderWin`(순서 편집) = **nexa-dir3가 nexa-sql에서 복사해 간 사본이 따로 있다**(§3 · 두 앱 두 벌) → 편입 효과가 가장 크다 · 이어서 `FilterBar`/`FindBtn` · `ActivityBar` · `dragchip` · 3상태 체크박스 · `SearchHistory`(영속은 포트로 주입).
4. **상태바** — nexa-ctl `StatusBar`(dir3 사용 중)로 옮기기는 칸 편집(`statusbar.layout`)과 맞물려 2단계.
5. **장기** — 결과 그리드(`grid.rs` → nexa-grid) · 트리 패널 4종(행 그리기·펼침 표식만 공용 도움 함수 후보).

- **선행 조건**: 편입 후보 다수가 아이콘 자산 `toolicons.rs`(nexa-sql)에 기댄다 → 아이콘 **주입 방식**(호스트가 마스크를 넘김)을 먼저 정한다.
- 규칙: 편입·교체마다 nexa-ui `CONSUMER-CHANGES` 행 + 소비자 시험(dir3 포함) · nexa-sql 쪽은 기능 점검·E2E.

## 2. nexa-sql 자체 구현 분류

### 2-1. nexa-ctl에 이미 있는데 따로 만든 것 → 교체

| 대상(nexa-sql) | 줄 | nexa-ctl 대응 | 남길 것 |
|---|---|---|---|
| `toast.rs` `Toasts` | 610 | `Toasts` · `life_alpha` · `bar_remaining`(nexa-ctl `toast.rs` 머리 주석 = "nexa-sql 원본 승격 · 이관은 nexa-sql 결정") | SQL 오류 분류(`class_label`) · 실행 카드 연동 |
| `palette.rs` `Palette` | 1,326 | `Palette`(nexa-sql 원본 승격) | 명령 목록 · Goto 구문은 주입 |
| 설정 창 스플리터(`prefs_win.rs`) | — | `Splitter`(메인 창 · 객체 찾기 창은 이미 사용) | — |
| 손그림 버튼(값 필터 팝업 · 확장 패널 · Output 머리 · 실행 카드 ■ · 미커밋 경고 · 링크 카드) | — | `Button`(+ `TimeoutButton`) | — |
| 손그림 스크롤바(값 필터 팝업) · 스크롤바 없는 목록(확장 패널 · 단축키·세션·변수·트랜잭션 로그 창) | — | `ScrollBars` | — |
| 상태바(`app/paint.rs` 손그림 + 칸 rect 히트) | — | `StatusBar`(칸 · hover · 클릭 통지) | 칸 편집 `statusbar.layout`과 맞물려 2단계 |
| 트리 패널 4종(탐색기 · 프로젝트 · 북마크 · 아웃라인) | 탐색기 10.6k | `TreeView`/`TreeModel`(설정 창만 사용) | 지연 로딩·메타 결합이 깊어 유지(행 그리기·▸/▾ 공용 도움 함수 후보) |
| 결과 그리드 `grid.rs` | 11.5k | nexa-ui `nexa-grid`(nexa-sql 미의존 · 머리 주석 "nexa-grid가 오면 교체") | 편집·필터·방언 결합 = 장기 |

### 2-2. 범용인데 nexa-ctl에 없는 것 → 편입 후보

| 대상 | 줄 | 사용처(nexa-sql) | 도메인 의존 | 다른 앱 |
|---|---|---|---|---|
| `CopyBtn`(`copybtn.rs`) | 185 | 5 | 아이콘 · 설정 ms | **dir3 사본 171줄**(nexa-sql 복사 · 글리프판) |
| `OrderWin`(`order_win.rs`) | 995 | 설정 3종 | `Msg` · `OrderSpec` 어댑터 | **dir3 사본 1,002줄**(dir2 이식) · nexa-ctl `order` 모델과 짝 |
| `FilterBar`(`filterbar.rs`) + `FindBtn`(`findbar.rs`) | 1,034 + 1,015 | 9 · 4 | `nsql_core::filterq` · 정규식 · `Msg` | — |
| `SearchHistory` | 865 | 10 | 영속(`nsql_settings::json`) → 포트 | — |
| `ActivityBar`(`activity.rs`) | 279 | 2 | 아이콘 | — |
| `dragchip` | 36 | 2 | 없음 | — |
| `DiffView`(`diff_view.rs`) | 420 | 1 | `Msg` 머리 줄 | nexa-ctl `diff`(모델) 있음 |
| 3상태 체크(값 필터 팝업) | — | 1 | — | nexa-ctl `Checkbox`에 3상태 추가 |
| **공용 목록 컨트롤(신설)** | — | 7곳 각자 구현 | — | — |

### 2-3. 앱 전용 → 유지

조건 바(`condbar.rs`) · 값 필터 팝업(`valuepick.rs` · 손그림 부분만 2-1로) · 완성 카드(`intel_card.rs`) · 실행 카드(`runtoast.rs`) · 미커밋 경고(`txwarn.rs`) · IME 안내 · 메모리 창 그래프 · 객체 상세 머리 줄 — 대부분 nexa-ctl 부품의 **조합**이라 문제 없음(손그림 버튼만 교체).

## 3. 소비 저장소 대조(10-16 · `D:/Projects/kiros33` · `*.rs` · `target` 제외)

- nexa-ui 의존 저장소(Cargo.toml에 `nexa-ctl`/`nexa-gfx`) = **nexa-sql · nexa-dir3 · nexa-beep · nexa-clip** 4곳 · nexa-dir2 · nexa-memkeeper · nexa-license(·server) · sosomlab-nexa-viewer = 의존 없음 · nexa-coffee · nexa-shortcut · nexa-slide = Rust 크레이트 없음(이 PC 기준).
- 컨트롤 이름이 나오는 `.rs` 파일 수(이름 일치 · nexa-sql은 같은 이름의 자체 타입 포함):

| nexa-ctl 컨트롤 | nexa-sql | nexa-dir3 | nexa-beep | nexa-clip | 판정 |
|---|---|---|---|---|---|
| `Toasts` | 2(자체 `toast.rs`) | 1(**nexa-ctl 판** · 앱 사본 삭제 = 114차) | 0 | 0 | nexa-ui 판 = dir3 사용 · **nexa-sql만 사본** |
| `Palette` | 3(자체 `palette.rs`) | 4(**`nexa_ctl::Palette`**) | 0 | 0 | 같음 |
| `StatusBar` | 0(손그림) | 2(**`nexa_ctl::StatusBar`**) | 0 | 0 | 같음 |
| `Splitter` | 2 | 1 | 0 | 0 | 둘 다 사용 |
| `TreeView` · `TreeGrid` | 1 · 0 | 1 · 0 | 3 · 2 | 1 · 0 | 사용 중 |
| `SegProgress` | 0 | 2 | 0 | 0 | dir3만 |
| `ListEditor` | 0 | 0 | 0 | 2 | clip만 |
| `ColorPicker` | 0 | 0 | 3 | 1 | beep · clip |
| `CheckCombo` · `BusyRing` · `SpeedHud` | 1 · 1 · 2 | 0 | 0 | 0 | **nexa-sql만** |
| `Button` · `TextBox` · `ScrollBars` · `ContextMenu` · `Switch` · `Combo` · `Toolbar` | 사용 | 사용 | 사용 | 사용 | 공통 |

- **nexa-ui에 있는데 아무도 안 쓰는 컨트롤 = 이 목록 안에는 없음**(전부 한 곳 이상 사용).
- **앱 사이 사본**(파일 이름 대조): `copybtn` · `order_win` = nexa-sql + nexa-dir3 · `palette`(dir3 `app/palette.rs` = 명령 표 어댑터 · 컨트롤은 nexa-ctl) · 나머지 후보(`filterbar` · `findbar` · `activity` · `dragchip` · `search_history` · `diff_view` · `valuepick`)는 nexa-sql에만.
- 재현: 협업 스크래치 스크립트(저장소별 `*.rs`에서 이름 대조) · 상시 도구로 둘 때 = nexa-ui `scripts/consumer-usage.sh` 후보(시험 자산 = 개발 몫).

## 4. 남은 판단(사용자)

- 위 1~5 순서로 진행할지 · 각 단계는 별도 차수(nexa-ui 차수 + nexa-sql 소비 · dir3 영향 = `CONSUMER-CHANGES`).
