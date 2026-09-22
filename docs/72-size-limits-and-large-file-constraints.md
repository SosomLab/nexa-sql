# 72 · 크기·줄 수에 따른 기능 제한 — 제약 원장 (2026-09-22 · 사용자 "전체 선택이나 일치 항목 표시 등은 대용량 파일에서 제한해야 할 기능 — 현재 어느 수준까지 레벨별로 제한하는지 조사 · 용량/줄 수 제한은 제약 문서와 위키에 정리")

> 이 문서가 **원장**이다: 파일 크기·줄 수·행 수·바이트에 따라 앱이 스스로 끄거나 상한을 두는 모든 기능을 한 표에 둔다. 사용자용 설명은 [wiki/Large-Files-and-Limits](wiki/Large-Files-and-Limits.md)(같은 내용을 사용 관점으로) · 설계·측정 배경은 [59 대용량 파일](59-large-file-handling.md) · 부하원 원장은 [39 §3](39-resource-governance.md). **새 상한이나 단계별 제한을 넣으면 세 곳(이 표 · 39 §3 · 위키)을 함께 고친다.**

## 0. 한 줄

편집기는 파일을 **보통 · L1 · L2** 세 단계로 나눈다(**바이트 또는 줄 수 중 하나라도 넘으면** 그 단계). L1(5 MB / 10만 줄)부터 *그리기·부가 기능*을 끄고, L2(20 MB / 30만 줄)부터 *구문 강조*까지 끈다. 50 MB부터는 열기 전에 **어떻게 열지 묻는다**. 본문의 편집·선택·찾기 자체는 어느 크기에서도 제한하지 않는다(설계상 O(보이는 줄) · 거대 편집만 확인).

## 1. 단계 기준

| 단계 | 기준(설정 키 · 기본) | 판정 시점 | 표시 |
|---|---|---|---|
| 보통 | L1 미만 | — | — |
| **L1** | `file.large_l1_mb` **5** MB **또는** `file.large_l1_lines` **100,000** 줄 | 읽은 직후 · 저장 직후(`reclassify`) | 열 때 상태줄 안내(`StLargeMode`) + 활성 탭인 동안 상태줄 표식 `L1`(`StLargeSeg`) · 팔레트 `file.large_force`("Toggle Large File Optimizations (This Tab)") |
| **L2** | `file.large_l2_mb` **20** MB 또는 `file.large_l2_lines` **300,000** 줄 | 같음 | "L2" |
| 열기 선택 | `file.large_ask_mb` **50** MB(0 = 안 묻음) | 열기 전(크기만 보고) | 팔레트 4택: 그대로 열기 / **앞부분만** `file.large_head_mb` **8** MB(읽기 전용 · 경로 없음) / 읽기 전용 / **열지 않고 실행** |
| 비동기 적재 | `file.async_load_mb` **8** MB | 열기 전 | 자리 탭 + 진행 막(그 탭만 · Esc 취소) — [59 §5-2](59-large-file-handling.md) |

- 단계는 **탭마다** 따로(같은 창의 작은 탭은 그대로). 사용자가 팔레트에서 **`file.large_force`**를 실행하면 그 탭은 단계와 무관하게 전부 켠다(다음 읽기/저장 재판정까지).
- 기준을 바꾸면(`nsql config set file.large_l1_mb 10` 등) 열려 있는 탭 전부 즉시 재판정.

## 2. 단계별로 꺼지는 것 — 기능 원장

"⭘" = 그대로 · "✕" = 끔 · 괄호 = 그 단계와 무관하게 늘 걸리는 **자체 상한**.

| 기능 | 보통 | L1 | L2 | 결정 키 | 왜 | 근거 코드 |
|---|---|---|---|---|---|---|
| 미니맵 | ⭘(줄 **4,000**까지만 그림 · `MINIMAP_MAX_LINES`) | ✕ | ✕ | 단계 고정 | 매 편집마다 전체 줄 래스터 | `editors.rs enforce_large` · nexa-ctl `textbox.rs` |
| 선택어 강조(일치 항목 표시 · 같은 단어 상자) | ⭘(보이는 줄만 훑음) | ✕ | ✕ | 단계 고정 | 보이는 줄만이라 비용은 작지만 캐럿 이동마다 다시 훑음 | 같음 · `set_occurrence_highlight` |
| 줄 변경 표시(거터 띠 · 저장본 대비) | ⭘(디프는 **1,500줄** 창 안에서만 LCS · `LCS_CAP`) | ✕ | ✕ | 단계 고정 | 저장본 사본(파일 크기)을 들고 있어야 한다 | `set_baseline(None)` |
| 저장본 사본(더러움 판정 · 외부 변경 3-way 병합) | ⭘(병합 `file.external_merge_max_kb` **2,048** KB까지) | ✕ 사본 없음 → 더러움 = 저장 지점 O(1) · 외부 변경은 고치지 않은 탭이면 기준 = 버퍼, 고친 탭은 병합 없이 묻기 | ✕ | 단계 고정 | 메모리(파일 크기만큼) | `is_dirty` · `main.rs` 외부 변경 |
| **확장 효과**(Rainbow Pairs — 괄호 깊이 색 · 짝 표 · 짝 없음 표시) | ⭘(확장 자체 상한 `rainbowpair.max_kb` 기본 **0 = 단계 따름** · 값을 주면 그 크기에서 확장만 먼저 멈춤) | ✕ | ✕ | `file.large_ext_level` **l1**(0/l1/l2) | 편집마다 본문 전체 쌍 표 · 09-22 전에는 2 MB 자체 상한과 L1이 두 겹이었다 | `large_bracket_opts` · `rainbow_pairs.rs` |
| **구문 강조** | ⭘ | ⭘ | ✕(Plain) | `file.large_syntax_level` **l2**(0/l1/l2) | 줄 단위 캐시라 L1까지는 버틴다(2 MB 실측 21 MB 상주) | `set_highlighter(None)` |
| 되돌리기 **기록 파일**(재시작 뒤 되돌리기) | ⭘(`editor.undo_persist_mb` **4** MB · `editor.undo_persist`) | ✕ 저장 안 함 | ✕ | 단계 고정 | 기록이 파일 크기에 비례 | `undo_persist_save/load` |
| 되돌리기 **메모리** | ⭘ | ⭘ | ⭘ | `editor.undo_budget_mb` **64** · `editor.undo_max` **1,000** 단계 · 선택 되돌리기 500 단계 | 예산을 넘으면 오래된 단계부터 버림 | nexa-ctl `EditState::evict` |
| **거대 편집 확인**(큰 선택 위에 타이핑·붙여넣기·삭제) | ⭘ | ⭘ | ⭘ | `editor.undo_giant_mb` **32** MB(0 = 끔) | 한 번에 지우는 글자가 이보다 많으면 먼저 안내(같은 동작 되풀이 = 실행) | `set_giant_edit_limit` · `giant_notice` |
| **전체 선택**(Ctrl+A) · 드래그·Shift 선택 | ⭘ | ⭘ | ⭘ | **제한 없음** | 선택 = 구간 둘(앵커·캐럿) · 그리기는 보이는 줄만 · 편집은 위 거대 편집 확인이, 복사는 아래 복사 확인이 막는다 | nexa-ctl `EditState` |
| **복사/잘라내기**(Ctrl+C/X · 큰 선택) | ⭘ | ⭘ | ⭘ | `editor.copy_confirm_mb` **32** MB(0 = 안 물음) | 문자열 사본 + Windows UTF-16 변환 = 선택의 2~3배 순간 할당 → 첫 누름은 안내 · 3초 안 되풀이 = 실행(문자열을 만들기 전 바이트 수로 판정) | `main.rs copy_confirm_pending` |
| **다중 선택 구간 수**(Ctrl+D · Alt+F3 · Ctrl+K,Ctrl+D · Ctrl+클릭 캐럿 · Split into Lines · 열 선택) | ⭘ | ⭘ | ⭘ | `editor.max_occurrences` **10,000**(모든 입구 공통 · 넘으면 상태줄 안내) | 구간마다 캐럿·편집이 곱해진다 | nexa-ctl `EditState::max_regions`(`add_selection`·`toggle_caret`·`set_regions`) · `main.rs regions_cap_notice` · ★ 09-22 전까지 **키만 있고 미배선**(§5) |
| 찾기/바꾸기(찾기 막대) | ⭘ | ⭘ | ⭘ | 제한 없음(줄 단위 검색 · 본문 사본 없음) | 65 MB에서 실측 [59 §6-2](59-large-file-handling.md) | `find_in_chars` |
| 파일 검색(Find in Files) | — | — | — | `search.max_file_kb` **1,024** KB 넘는 파일은 건너뜀 · 결과 상한 | 디스크·메모리 | `nsql-search` |
| 다중 캐럿·열 선택 | ⭘ | ⭘ | ⭘ | 구간 수 = 위 `editor.max_occurrences`(열 선택·줄 나누기는 앞에서부터 상한 개만) | — | |
| 자동 들여쓰기 · 괄호 자동 닫기 | ⭘ | ⭘ | ⭘ | 제한 없음(줄 단위) | | |
| 동시 편집(칸 나누기) | ⭘ | ⭘ | ⭘ | `editor.split_max` **3** | 칸마다 그리기 | |
| 읽기 전용 · 앞부분만 · 열지 않고 실행 | 선택 | 선택 | 선택 | §1 열기 선택 | | `fileload.rs` · `file.run_file` |

**향상 모드(`perf.boost`)와의 관계**: 향상 모드는 *상주·유휴 CPU*를 줄이는 전역 강제(미니맵·강조·애니메이션 등 [45 §4-1](45-perf-boost-benchmark.md))이고, 큰 파일 단계는 *그 탭의 크기*로 정한다. 둘은 독립이며 겹치면 둘 다 끈 것이 된다.

## 3. 편집기 밖의 상한(크기·수)

| 대상 | 키 · 기본 | 넘으면 |
|---|---|---|
| 결과 자동 페치 행 수 | `grid.max_rows` **200** | 스크롤 끝에서 다음 200 · 전체 조회 = 카드 경로([43 §11](43-fetch-model-and-result-tabs.md)) |
| 결과 메모리 예산 | `grid.memory_budget_mb` **1,024** | 넘으면 페치를 멈추고 안내(텍스트 캐시는 별도 예산 [43 §6]) |
| 결과 탭 수 | `grid.result_tabs_max` **8** | 고정 안 한 가장 오래된 탭부터 퇴거 |
| 실행 카드 수 | `run.toast_max` **30**(향상 모드 8) | 끝난 카드부터 버림 |
| CLI 행 수 · 열 폭 | `cli.max_rows` **200** · `cli.max_col_width` **60** | 잘라 표시 |
| 변수 값 크기 | `vars.max_value_kb` **1,024** | 잘라 저장 + 안내 |
| 로그 창 · 트랜잭션 로그 | `log.max_lines` **10,000** · `txlog.max_entries` **10,000** · 파일 `log.file_max_kb` | 오래된 줄부터 버림 / 회전 |
| 프로젝트 트리 열거 | `project.scan_max` **5,000**(폴더당) | 나머지는 "… 더 있음" |
| 파일 열기 다중 선택 | `file.open_max` **10** | 초과분 제외 표시 |
| 되돌리기 기록 파일 | `editor.undo_persist_mb` **4** | 저장 안 함 |
| 세션 수 | `session.max_shared` **8** · `session.max_private` **8** | 접속 거부 + 안내 |
| 접속 동시성 · 신호등 | `connect.max_concurrent` **4** · `probe.max_inflight` **16** | 큐 |
| **CLI `nsql` · "열지 않고 실행"(`file.run_file`)** | 상한 없음 — **스트리밍 분할기 없음**: 파일을 통째로 읽어 문장으로 나눈다(메모리 ≈ 파일 크기 × 2~3 · 59 §5-1 "하지 않은 것" · **D-128** 3단계 결정 대기) | 65 MB 실측 = 실행 중 상주 ≈ 파일 크기 × 2 · 문장 수만큼의 `String` | 
| 편집기 열기 선택 "앞부분만" | `file.large_head_mb` **8** | 읽기 전용 · 경로 없음 |

## 4. 실측 근거(어느 크기에서 무엇이 버티나)

- 2 MB(3만 문장 · L0): 상주 21 MB · 입력 3 ms · 구문 강조 켬([26 §7-9](26-performance-architecture.md)).
- 65 MB(L2 · 100만 줄): 상주 87 MB(버퍼 교체 뒤) · 입력 3 ms · 첫 페인트 55 ms · 회수 시험 R3 = 닫으면 기준선 +0.2 MB([59 §6-2](59-large-file-handling.md) · 26 §7-9 C-2).
- 70만 줄 벤치 **전 기능**(미니맵·강조·선택어·괄호 다 켬): 편집마다 **85 ms** · 첫 페인트 1.2~1.4 s → 이것이 L1/L2에서 부가 기능을 끄는 근거다([26 §7-9 F](26-performance-architecture.md)).

## 5. 이번 조사(09-22)에서 드러난 불일치와 조치

| 발견 | 조치 |
|---|---|
| `editor.max_occurrences`(10,000)가 설정·부하원 원장(39 §3)에 있으나 **어디서도 읽지 않았다** → Alt+F3가 끝까지 돈다 | ★ 배선: 전체 일치 선택은 이 수에서 멈추고 상태줄 "일치 n개에서 멈춤(상한 …)" · Ctrl+D/건너뛰기도 상한에서 거부 |
| `editor.highlight_max_kb`(1,024 KB · "이보다 크면 평문")가 **미배선** — 실제 평문 전환은 L2(`file.large_syntax_level`)뿐이라 설명이 거짓이었다 | ★ 키 **폐기**(레지스트리·i18n·부하원 원장에서 제거 · 설정 파일에 남은 값은 무시) — 구문 강조 컷오프는 L2 하나 |
| 59 §5-1은 "L1 = 미니맵·선택어·기준선, L2 = +구문"만 적고 확장·되돌리기 기록·병합·거대 편집·일치 수는 흩어져 있었다 | 이 문서 §2 표가 원장 · 59는 배경 |
| 위키(사용자 설명)가 없었다(T-103 잔여) | [wiki/Large-Files-and-Limits](wiki/Large-Files-and-Limits.md) 첫 페이지 + [wiki/Home](wiki/Home.md) + `scripts/wiki-publish.sh`(T-169) |
| Rainbow Pairs `rainbowpair.max_kb`(2 MB)와 L1(5 MB)이 **두 겹**(2~5 MB 구간은 확장 자체 상한이 먼저 끔 — `highlight_max_kb`와 같은 종류) | 기본 **0 = 단계 따름**(값을 주면 확장만 먼저) |
| 복사/잘라내기에 상한이 없어 65 MB 전체 선택 뒤 Ctrl+C = 130 MB 순간 할당 | `editor.copy_confirm_mb` 32(거대 편집 확인과 같은 꼴) |
| `editor.max_occurrences`가 Ctrl+D/Alt+F3만 막고 Split into Lines · 열 선택 · Ctrl+클릭은 못 막았다 | nexa-ctl `EditState::max_regions`로 **모든 입구 공통** · 라벨 "다중 선택 구간 상한" |

## 6. 규칙

1. 크기·수로 무언가를 끄거나 자르는 코드를 넣으면 **설정 키 하나 + §2/§3 표 한 줄 + 39 §3 + 위키 한 줄**이 한 묶음이다(키만 만들고 안 읽는 일이 다시 없게 — 부하원 원장의 키는 **읽는 자리**를 표에 적는다).
2. 단계 판정은 `Editors::level_for` 한 곳(바이트·줄 수 중 하나라도) · 단계별 기능 적용은 `enforce_large` 한 곳.
3. 상한에 걸리면 조용히 자르지 말고 **상태줄·로그에 키 이름과 함께** 안내한다(`nsql config set <키>`로 바꿀 수 있다는 것을 사용자가 알게).
