# 80. 메모리 모니터 — 상태줄 총량 + 메모리 맵 창(100차 mac · 2026-09-24)

> 사용자 요구(09-24): 화면 한구석에 **사용 메모리 총량** · 클릭(효과) → **메모리 맵 창**(처음 모달 → **모델리스 · 최상위 옵션**으로 변경) · 시스템/데이터 카테고리 · 색으로 전체·영역별 사용량 한눈에 · 창이 떠 있을 때 **1초(설정)마다 갱신** · **창이 없을 때는 메인 프로세스 성능에 영향 0** · 자료 구조·호출 구조가 체계적일 것.

## 1. 원칙
1. **비용은 창이 열려 있을 때만** — 닫혀 있으면 타이머 0 · 순회 0. 상태줄 숫자는 **어차피 그리는 프레임 안에서** OS 총량 한 번(`task_info`/`GetProcessMemoryInfo`/`statm` ≈ µs)만, 그것도 `mem.status_refresh_ms`(5 s)보다 자주는 아니다. 앱이 가만히 있으면 숫자도 가만히 있다(깨우지 않는다).
2. **계측 원장 하나**(`memstat::Cat`) — 카테고리는 열거형으로 고정 · 라벨·색·그룹은 원장에서만 나온다 · 새 캐시를 만들면 원장에 한 줄 + `MemSource` 구현 한 줄(39 §3 부하원 등재와 짝).
3. **포트 하나**(`memstat::MemSource::mem_report(&self, &mut Acc)`) — 각 부품(편집기·결과·메타·완성·로그)은 자기 바이트를 `Acc::add(Cat, n)`으로 보고만 한다. 수집은 호스트 `App::mem_sample()` **한 곳**(DR-33·77의 "한 세트 + 투영"과 같은 사상 — 창과 상태줄은 같은 `Sample`을 읽는다).
4. **어림은 어림이라고 표시** — 데이터 카테고리는 각 부품의 `approx_bytes` 규칙(4 B/글자 등)이라 OS 총량과 차이가 나며, 그 차이는 **기타(런타임·라이브러리·미집계)** 한 줄로 드러낸다(숨기지 않는다).

## 2. 자료 구조(`crates/nexa-sql/src/memstat.rs`)
| 형 | 뜻 |
|---|---|
| `Cat` | 데이터 카테고리 원장 — `ResultData` 결과 데이터 · `ResultText` 결과 텍스트 캐시 · `EditorText` 본문 · `EditorHistory` 되돌리기 · `EditorCache` 그리기 캐시 · `Meta` 메타(탐색기·완성) · `Intel` 완성(낱말·후보·아이콘) · `Logs` 로그·트랜잭션 로그 · `Surfaces` 창 표면 · `Icons` 아이콘 캐시 · `Other` 기타. `label()` `color()` `ALL` |
| `SysMem` | OS가 말하는 프로세스 총량 — `footprint`(mac phys_footprint · Win Private · Linux resident−shared) · `resident` · `anon`(mac internal · Win private · Linux resident−shared) · `file_backed`(mac external · Win ws−private · Linux shared) · `compressed`(mac) · `heap_used`/`heap_held`(mac `mstats` · glibc `mallinfo2` · Win `HeapSummary`) |
| `Acc` | `[u64; Cat::N]` 누적기 · `add(cat, n)` · `get` · `sum` |
| `MemSource` | 포트 — `fn mem_report(&self, acc: &mut Acc)` |
| `Sample` | `at` · `sys` · `data: Acc` · `other()` = footprint − data 합(포화) · `fmt(bytes)` |

구현 부품: `Editors`(탭마다 nexa-ctl `TextBox::mem_parts` = 본문·기록·캐시) · 결과(`Grid::mem_parts` = 데이터·텍스트 캐시 · `all_grids`) · `ExplorerSet → Explorer`(메타 `approx_bytes` · 아이콘 캐시) · `Intel`(문서 낱말 · 후보 · 아이콘) · `LogWin::approx_bytes` + `TxLog` 항목 수 × 256 · 창 표면 = 메인·메모리 창 `w×h×4`.

## 3. 호출 구조
```
그리기(메인)      ── mem.statusbar on ─▶ mem_status_text(): 5 s 지났으면 memstat::sys_total() 1회 → 상태줄 세그먼트
상태줄 클릭       ── MouseDown = 눌림 표시 · 창 토글(open_mem 깃발 → about_to_wait에서 열기) · MouseUp = 눌림 해제
about_to_wait     ── mem_win 열림 ─▶ now ≥ mem_next ? mem_sample() → mem_win.set_sample · 두 창 redraw · next = min(next, mem_next)
                  ── 닫힘 ─▶ 아무것도 안 함(깨움 없음)
창 이벤트         ── Paint → mem_win.paint(sample) · Toggled(on) → 설정 mem.always_on_top · Close → 기하 기억
```
- 창은 `sessions_win.rs`와 같은 골격(메인 소유 · 모델리스 · 기하 메모 `window.mem_*` · `winfocus::owned_by` · `keep_on_screen`) + 로그 창의 최상위 스위치(`WindowLevel::AlwaysOnTop`).
- 갱신 주기 = `mem.refresh_ms`(1000 · 250~10000). 창이 열린 동안만 `WaitUntil`에 참여.

## 4. 화면(메모리 맵)
1. 머리: 총량(풋프린트) 큰 글자 + `상주 N` 부제 · 오른쪽 **최상위** 스위치.
2. **총량 막대**: 풋프린트를 100 %로 데이터 카테고리(색) + 기타(회색)를 비율로 쌓는다 — 영역 간 비교가 한눈에.
3. **데이터** 표: 색 칩 · 이름 · 막대(풋프린트 대비) · 바이트 · %.
4. **시스템** 표: 풋프린트 · 상주 · 익명(힙·스택) · 파일 매핑(라이브러리·글꼴) · 압축 · 힙 사용 중(mac `mstats().bytes_used` — 아직 건드리지 않은 예약 페이지도 세어 풋프린트보다 클 수 있다) · 힙 여유(예약·가상 — mac `mstats().bytes_free`는 가상 예약이라 상주보다 클 수 있다 · 회수 `memtrim`이 돌려줄 수 있는 몫의 상한).
5. 바닥: "N초 전 갱신 · 매 1 s".
색은 테마와 무관한 고정 팔레트(밝은/어두운 배경 모두 읽히는 중간 채도) · 글자는 테마 색.

## 5. 성능 점검(71 체크리스트 · 39 §3 등재)
| 항목 | 값 |
|---|---|
| 창 닫힘 | 스레드 0 · 타이머 0 · 그릴 때 OS 조회 1회/5 s(≈ 3 µs) · 상주 +0 |
| 창 열림 | 1 s마다 `mem_sample()` — 편집기 탭 N + 결과 탭 M + 탐색기 서버 K 순회(각 O(1) 어림 · 총 < 50 µs) + 창 그리기 1회(표 20행) |
| 메모리 | `Sample` 하나(≈ 200 B) · 창 표면(w×h×4 · 닫으면 해제) · 이력 없음(이력이 필요하면 `VecDeque<u64>` 60칸 = 480 B) |
| 상한 키 | `mem.statusbar`(끄면 조회 0) · `mem.refresh_ms` · `mem.status_refresh_ms`(HIDDEN 5000) · `mem.always_on_top` |
| 네트워크 | 없음 |

## 6. 결정
- **D-207** 창 = 모델리스 + 최상위 스위치(사용자 09-24 · 모달 취소) · 메인 소유.
- **D-208** 상태줄 숫자 = 풋프린트(mac) / Private Bytes(Win) / resident−shared(Linux) — 활성 상태 보기·작업 관리자의 "메모리"와 맞춘다(26 §7-2).
- **D-209** 데이터 카테고리는 어림 · 차이는 "기타"로 표시 · 정확 계측(할당자 후킹)은 하지 않는다(성능 원칙 1).

## 7. 할 일
- 첫 캡처(09-24 · 격리 인스턴스 · 접속 없음): 풋프린트 72 MB · 상주 242 MB · 익명 73 · 파일 매핑 52 · 힙 사용 44 · 창 표면 25 MB(메인 2750×1954×4 + 창) · 기타 47 MB.
- T-197 ✅ 1차(이 문서 · `memstat.rs` · `mem_win.rs` · 배선 · 설정 4 · 위키) · 남음 = 이력 스파크라인 · 카테고리별 "정리" 버튼(`memtrim` 호출 · 결과 탭 닫기 안내) · CLI `nsql mem`(같은 `Sample`을 표로).
