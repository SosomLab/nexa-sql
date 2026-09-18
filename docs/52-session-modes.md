# 52. 세션 모드 — 공유 · 전용(Private) · 개별, 그리고 실행 통제

> **선행**: [34 트랜잭션 UX](34-transaction-ux.md)(T-77 공유 세션 1차) · [27 §6 CONNECT 동사](27-cli-conventions.md)(D-45) · [43 페치 모델](43-fetch-model-and-result-tabs.md) · [44 트랜잭션 로그](44-transaction-log.md) · [26 §8 네트워크 부하](26-performance-architecture.md) · [39 §3 부하원 원장](39-resource-governance.md)
> **상태**: 🚧 1차 구현 ✅ 2026-09-18(56차 · mac) — 세션 컨텍스트 · 통제 단일화 · `CONNECT`/`DISCONNECT` 전용 세션 · 탭 표식 · 개별 모드 · 유휴 닫기 · **공유 연결 N개(추가 접속 · Disconnect 드롭다운 · 탭별 선택)**. **결정 D-96~108 = 권장안대로 개발 완료**(사용자 09-18 "결정에 따른 개발도 모두 진행" · §10) · 서버별 탐색기 유지(§2-2) · 세션 상태 검토(§6-4). T-54를 이 문서가 대체한다.

## 0. 기본 사상 (사용자 09-18)

**1 인스턴스 = 1 서버 · 1 계정.** 가볍게 쓰는 것이 기본이다. 그래서

- 기본 = **공유 세션 하나**. 모든 탭이 그 세션을 쓰고, 실행은 전체 탭을 통틀어 한 번에 하나.
- **전용(Private) 세션은 명시적 예외** — 탭에서 `CONNECT …`를 실행했을 때만 생기고, 상한(`session.max_private`)이 있다.
- **개별 모드**(`session.mode = per-editor`)도 같은 서버·같은 계정으로 세션만 탭 수만큼 여는 것이다(접속 창에서 마지막으로 성공한 스펙 = `default_spec`).
- **공유 연결은 여러 개일 수 있다**(사용자 09-18 확정): 서버 3곳에 동시에 붙고 탭이 그중 하나를 쓴다. 단 **같은 서버·DB·계정에 공유 연결을 중복으로 열지 않는다**(전용 세션의 직접 `CONNECT`는 예외) → §2-1.

## 1. 세 가지 모드

| # | 이름 | 설정 | 세션 | 통제 단위 | 비고 |
|---|---|---|---|---|---|
| 1 | **공유** | `session.mode = shared`(기본) | 공유 연결 1~N(§2-1) | **그 세션을 쓰는 모든 탭** | 한 탭이 실행 중이면 다른 탭의 실행·페치·건수·커밋도 전부 막힌다 |
| 2 | **전용** | (모드 1 안의 예외) `session.private_connect = on` | 탭에서 `CONNECT`한 세션 | **그 탭 하나** | 탭 앞 표식 🔌 · `DISCONNECT`/표식 메뉴로 공유 복귀 |
| 3 | **개별** | `session.mode = per-editor` | 탭마다 하나 | **각 탭** | 모든 탭에 표식 · 해제한 탭은 다시 접속할 때까지 실행 불가 |

모드 2는 별도 설정값이 아니라 **모드 1 위의 탭 속성**이다(→ D-96에서 재확인). 모드 3에서도 `CONNECT`는 그 탭 세션의 대상을 바꾼다.

## 2. 구조 — `Sess` 하나 = 워커 하나 = DB 세션 하나

```
App
 ├─ sess: Sess            ← 활성 편집기 탭이 쓰는 세션(기존 코드는 늘 self.sess.* 만 본다)
 ├─ parked: Vec<Sess>     ← 나머지 세션
 ├─ tab_bind: 탭 → 공유 세션 id   (없으면 default_shared · 전용 세션은 Sess.owner가 우선)
 ├─ default_spec          ← 개별 모드의 새 탭이 붙을 접속 정보
 └─ primary_sess          ← 탐색기·접속 창 표시가 따라가는 세션(접속 창으로 마지막에 붙은 것)

Sess { worker(스레드·Runner·세션) · events · busy · aux · connected · desc · spec
       · tx_pending/tx_dirty/tx_read · txlog · run_editor/run_tab/last_run_items/… · run_toast
       · key_cache · status · last_used/idle_closed · owner(전용 탭 id) }
```

- `Session`은 `Send`가 아니다(드라이버 제약) → **세션 = 스레드**. 유휴 스레드는 `recv()`에 잠들어 CPU 0 · 스택은 가상 메모리라 실사용 수십 KB. 스레드 풀/비동기 다중화는 드라이버가 전부 동기 API라 얻는 것이 없다.
- **맞바꾸기**(`sync_sess` · `with_sess`): 활성 탭이 바뀌면 `mem::swap` 한 번. 잠든 세션의 이벤트는 `drain_all`이 잠시 앞으로 꺼내 **같은 코드**(`drain_events`)로 처리한다 → 실행 상태 328곳을 고치지 않고 다중 세션이 됐다.
- 결과 패널은 원래 편집기 탭별(`panels`)이라 결과·추가 페치는 자연히 그 탭의 세션으로 간다. 세션이 바뀌는 순간(공유 → 전용 · 전용 → 공유) 그 탭의 옛 결과는 `more = false`(`freeze_tab_results`) — 다른 세션에서 OFFSET으로 이어 받으면 **다른 DB의 행이 붙는다**.
- 탐색기 메타 세션 · 라이브 로그 · 접속 창 신호등은 `primary_sess`만 따라간다(전용 세션의 접속·오류는 그 탭의 일).

### 2-1. 공유 연결 N개 (사용자 09-18)

접속 창에서 Connect하면 **기존 연결을 끊지 않고 추가**한다. 배치는 순수 판정 `sessions::login_plan` 하나:

| 상황 | 결과 |
|---|---|
| 같은 서버·DB·계정의 공유 연결이 이미 있음(접속됨 · 유휴로 닫힘) | **Use** — 중복 접속 없이 그 연결을 활성화(`connect.reconnect_same`이면 재접속) |
| 그 연결이 작업 중 | **Busy** — 거부(접속 창에 실패 표시) |
| 끊긴 채 노는 공유 세션 객체가 있음(묶인 탭 0 · 또는 세션이 하나뿐) | **Recycle** — 그 워커를 다시 씀(종전 "해제 뒤 다른 서버 접속"과 같은 모습) |
| 그 외 · 상한 미만 | **New** — 새 공유 세션(워커 +1) |
| 상한(`session.max_shared` 8) | **Limit** — 거부 + 안내 |

- **활성 공유 연결**(`default_shared`) = 마지막으로 접속 창에서 붙였거나 사용자가 고른 연결. 탐색기·접속 창 표시·묶이지 않은 탭이 따른다.
- **탭은 처음 실행한 연결에 묶인다**(`tab_bind`) — 활성 연결을 바꿔도 이미 쓰던 탭이 말없이 다른 서버로 가지 않는다(DELETE를 엉뚱한 서버에 날리는 사고 방지). 바꾸려면 탭 표식 메뉴.
- 공유 탭에도 **항상 표식**(중립색 🔌 · 전용은 강조색 · 미연결/끊김은 사선 · 09-18 "항상 보여지도록") → 클릭 = 공유 연결 고르기/미연결.
- **툴바 Disconnect = 드롭다운**(사용자 09-18 보완): 누르면 **늘 연결 목록**이 보인다 — 공유 연결(`●` = 활성)과 탭 전용/개별 세션(`접속 설명 [탭 제목]`)을 한 줄씩 · **줄을 누르면 그 연결만 해제** · 맨 아래 **모두 해제** · 공유 연결이 둘 이상이면 `활성 연결로 ▸`. 전용 세션 줄은 그 탭의 규칙(공유 복귀 · 개별 모드 = 끊김)으로 해제된다.
- 공유 연결을 해제하면: 탐색기가 그 연결을 따르고 있었으면 함께 닫힘 · 그 연결에 묶인 탭은 **끊김 표식**으로 남는다(실행 = not connected) · 남은 연결은 드롭다운/표식 메뉴에서 활성화. 끊긴 세션 객체는 묶인 탭이 없어지면 거둔다(`reap_shared`).

### 2-2. 서버 메타(탐색기 = 인텔리센스·툴팁의 단일 원천) — 서버별 유지 ✅ 09-18

**원칙(사용자 09-18)**: 탐색기의 메타가 인텔리센스·툴팁의 **한 개 소스**다 → 어떤 서버에 붙은 세션이 **하나라도** 남아 있으면 그 서버의 탐색기는 유지돼야 한다. 공유·전용·개별 어느 모드든 같은 규칙.

| 항목 | 구현(`explorers.rs` `ExplorerSet`) | 이유 |
|---|---|---|
| 단위 | **서버 키 = (방언·호스트·포트·DB·계정)** 하나당 `Explorer`(트리 + 메타 스레드/세션) 하나 | 같은 서버에 탭 세션이 3개여도 메타는 **1벌**(접속 +1뿐) |
| 추가 | **어떤 세션이든** 접속에 성공하면(`RunEvent::Connected` → `explorer_attach`) 그 서버의 탐색기를 확보 — 이미 있으면 그대로(재조회 0). 프로필 이름뿐인 `CONNECT prod`·자격 없는 문자열은 저장소에서 완성한 스펙으로 | 전용 세션으로 처음 붙은 서버도 메타가 생긴다 |
| 유지 | `sync_refs`: 지금 붙어 있는(유휴로 닫힌 것 포함) 세션들의 서버 목록으로 참조 수를 센다 — **≥1이면 절대 닫지 않는다**. 3개 탭 중 1·2개를 끊어도 유지 | 사용자 원칙 그대로 |
| 0이 되면 | **메타 접속만 닫고 트리(읽어 둔 메타)는 남긴다** = 오프라인(루트에 "· 오프라인" · 안 읽은 가지는 더 못 읽음 · 서버에 다시 붙지 않는다) — 목록에서 지우는 것은 사용자(머리줄 메뉴 "탐색기에서 제거") | SSMS식 · 사용자 안 ②(자동 추가 · 수동 제거)에 안 ①의 서버 부하 회수를 합침 — 닫히는 것은 **접속**이지 메타가 아니다 |
| 다시 붙으면 | 오프라인 서버에 세션이 다시 붙으면 메타 세션을 다시 열고 트리를 새로 읽는다 | 끊겨 있던 동안의 변경 반영 |
| 유휴 회수 | 세션이 남아 있어도 메타 요청이 `session.idle_secs` 동안 없으면 **메타 세션만** 닫는다(`Req::Suspend`) — 트리 유지 · 다음 펼침/소스 요청 때 메타 스레드가 **조용히 다시 연다** | 메타 접속이 서버당 +1이므로 같은 부하 규칙 · 정보 제공은 끊기지 않는다(읽어 둔 것은 메모리에) |
| 화면 | **서버마다 루트 노드가 접속 순으로 세로로 쌓인다**(사용자 09-18 · SSMS식 — 하나를 골라 보는 방식 폐기) · **한 트리처럼 이어진다** — 첫 서버의 내용이 끝나는 바로 아래에 다음 서버의 루트(나뉜 패널 아님 · 한 폴더 안의 내부 폴더 둘을 펼친 모습) · 각 트리는 내용 전체 높이로 놓이고 **스크롤은 전체에 하나**(공용 뷰포트 · `Explorer::set_clip`) · 마우스 = 커서 아래 칸 · 키 = 마지막으로 누른 칸 · 선택은 전체에 하나 · 오프라인 루트 우클릭 = "탐색기에서 제거" · SQLite 루트는 전체 경로 대신 **파일 이름** | 접속한 서버가 한눈에 · 탭마다 다른 서버여도 맞는 메타가 늘 보인다 |
| 라이브 로그(T-71) | 실행한 세션의 **서버 탐색기**로 폴링(`live_poll(spec, …)`) | 전용 세션 실행도 모니터 가능 |

**모드별로 보면**

| | 공유 모드 | 개별 모드 |
|---|---|---|
| 탐색기 수 | 공유 연결 수(≤ `session.max_shared` 8) + 전용 세션이 붙은 **다른** 서버 수 | 서버 수(탭 수가 아님 — 탭 30개가 한 서버면 1) |
| 해제 시 | 공유 연결 해제 → 그 서버에 다른 세션(전용 탭 등)이 없을 때만 오프라인 | 탭 세션 해제 → 같은 서버의 다른 탭이 있으면 유지 · 마지막 탭이면 오프라인 |
| 메모리 | 서버당 트리 1(펼친 만큼만 · 지연 로드) | 같음 — 탭 수와 무관 |
| 서버 부하 | 서버당 메타 접속 ≤1(유휴 회수) | 같음 |

**남은 것(T-121 잔여)**: `MetaStore`(47)로 인텔리센스를 배선할 때 서버 키로 묶기 · 오프라인 메타의 LRU 상한(`meta.cache_mb`) · 노드 우클릭 "이 서버로 새 탭" · 접속 창의 다중 "접속됨" 표시.

## 3. 실행 통제 — 단일 판정 `Sess::blocked()`

`blocked = busy || aux > 0`. **busy** = 실행·접속·Commit/Rollback(워커 `done`으로 풀림) · **aux** = 추가 페치·전체 조회·건수·키 조회(각자의 응답으로 −1).
문지기는 `gate_open()` 하나(먼저 `sync_sess` → 막혔으면 상태줄 "Running…" + false), 화면은 `sync_gate()` 하나.

### 3-1. 진입점 전수 점검 (09-18 · `worker.send` 26곳)

| # | 진입점 | 종전 | 지금 |
|---|---|---|---|
| 1 | 문장 실행(Ctrl+Enter · 툴바 · 메뉴 · 팔레트) | busy만 | `gate_open` + 툴바 **흐림** |
| 2 | 전체 실행(F5) | busy만 · 버튼은 늘 활성 | `gate_open` + 흐림 |
| 3 | 새 결과 탭에 실행(Ctrl+\\) | busy만 | `gate_open` |
| 4 | Explain | busy만 · `run_tab`/`run_editor` 미설정(옛 값 사용) | `gate_open` · 실행 탭 기록 |
| 5 | 결과 새로고침 | busy만 | `gate_open` + 도구줄 흐림 |
| 6 | **추가 페치(스크롤 끝 자동)** | ❌ 무통제 — 실행 중에도 큐에 쌓임 | 그리드가 `session_blocked`면 **요청 자체를 안 만든다** |
| 7 | **전체 조회 ⇊** | ❌ 무통제 | 흐림 + `gate_open` |
| 8 | **건수 Σ** | ❌ 무통제 | 흐림 + `gate_open` |
| 9 | **Copy SQL / SQL 보기의 키 조회** | ❌ 무통제 | `gate_open`(막혔으면 다시 시도 안내) |
| 10 | **Commit/Rollback**(툴바·메뉴·단축키) | ❌ 무통제 — 실행 중 문장 뒤에 줄 섬 | `gate_open` + 흐림 · busy로 집계 |
| 11 | 상태줄 트랜잭션 팝업의 Commit/Rollback | ❌ | #10과 같은 경로로 합침 |
| 12 | 자동/수동 전환(`SET AUTOCOMMIT`) | busy만 | 그대로(busy) |
| 13 | 접속 창 Connect · 행 ▶ | busy만(활성 탭 세션 기준 — 전용 탭에서 열면 엉뚱한 세션) | **대상 세션**(`login_place` → `login_plan`)의 `blocked` |
| 14 | 실행 인자 접속 | — | 공유 세션 |
| 15 | 중지 ■ · Esc | busy일 때만 | **막힌 상태를 푸는 버튼** = `blocked`면 활성(전체 조회 중 포함) |
| 16 | 접속 해제 | 늘 가능(갇힌 워커 버림) | 그대로 · 전용 탭이면 그 세션만 |
| 17 | 닫기/종료/해제 확인 팝업의 Commit/Rollback | 큐 | 그대로(잃는 순간의 선택 — 막지 않는다) · **닫히는 탭의 세션으로** 보냄 |
| 18 | 탐색기 메타 질의 | 별도 세션(D-46) | 무관 |
| 19 | 접속 Test | 요청당 스레드 | 무관 |

**발견한 결함 5건(이번에 수정)**: ⑥⑦⑧ 실행 중 페치/건수가 큐에 쌓여 "Fetching…"에 멈춘 것처럼 보임 · ⑩ 실행 중 Commit이 줄 서서 끝난 뒤 조용히 실행됨(사용자가 의도한 시점이 아님) · ④ Explain 결과가 직전 실행의 탭으로 감 · ⑬ 접속 창이 활성 탭 세션의 busy를 봄 · 재접속을 실행 앞에 끼우면 첫 `done`이 busy를 일찍 풂(`skip_done`으로 해결).

### 3-2. 남은 틈 (정직한 한계)

- **미커밋 확인 팝업이 떠 있는 동안 다른 세션의 실행 완료**는 그대로 진행된다(팝업은 지금 세션에만 답한다 · 의도).
- `tx.close_action = commit|rollback`(묻지 않음)으로 실행 중인 탭을 닫으면 커밋이 실행 뒤에 줄 선다 — 워커가 순차라 안전하지만 시점이 밀린다. 실행 중 탭 닫기는 **먼저 취소를 보낸다**(`reap_sessions`).
- 확장(Rainbow 등)·팔레트는 전부 `menu_action` 경유라 같은 문지기를 지난다. **새 진입점은 `gate_open()`을 부르는 것이 규칙**(30 §1-2 체크리스트에 추가 → T-122).
- 실행 상태 카드(토스트)는 세션별 — 활성 탭의 것만 보인다. 뒤에서 끝난 실행은 탭의 ▶가 사라지는 것으로만 안다(→ D-104).

### 3-3. 09-19 재검토 — "공유 세션 · 4탭 · 1탭 실행 중" 시나리오(사용자 요청)

전제: 탭 A~D가 같은 공유 세션 · A가 F5로 실행 중(`busy`) · 사용자가 B/C/D로 전환한다. `sync_sess`가 같은 `Sess`를 `self.sess`로 맞바꾸므로 **B/C/D도 `blocked()` = 참**이고, 아래 진입점이 전부 같은 판정을 본다.

| 진입점(B/C/D에서) | 판정 경로 | 09-19 검토 결과 |
|---|---|---|
| 툴바 ▶ 문장 · ▶▶ 전체 · Explain | `sync_gate` → `set_item_enabled` (비활성) · 눌러도 `gate_open` | ✅ |
| 툴바 ■ 중지 | `gate.stop` = 막힘 → 활성 · `stop_run` = 지금 세션(=A의 실행)을 취소 | ✅ 의도(같은 세션이므로 어느 탭에서든 중지 가능 · 단일 세션 사상) |
| 툴바 Commit/Rollback | `sync_tx_ui`(막힘 ∧ 대기 없음 → 비활성) · `menu_action` → `gate_open` | ✅ |
| **메뉴바 Run ▸ 문장/새 탭/전체/Explain/Commit/Rollback** | 종전엔 **항상 활성으로 보였고** 누르면 `gate_open`이 거부(상태줄 "실행 중…") | ⚠️ 표시 결함 → **수정**: `build_menus_with(blocked)` → `MenuEntry::Disabled` · `sync_gate`가 바뀔 때 메뉴 재구성 |
| 단축키 F5/F9/Ctrl+Enter · 팔레트 · 확장 | 전부 `menu_action` → `gate_open` | ✅ |
| 결과 도구줄(추가 페치 · 전체 조회 · 건수 · 새로고침) · 스크롤 끝 자동 페치 | `grid.set_session_blocked` + `send_fetch`의 문지기 | ✅ |
| 결과 우클릭(SQL로 복사 · 키 조회 · 텍스트 보기) | `begin_sql_copy`/`begin_view_sql` → `gate_open` | ✅ |
| 상태줄 트랜잭션 팝업(Auto/Manual · Commit · Rollback) | Commit/Rollback = `menu_action` ✅ · **Auto/Manual 전환**은 `!busy`만 보고 `aux`를 무시 · 막혔을 때 설정만 바뀌고 세션엔 적용 안 됨 | ⚠️ → **수정**: `set_autocommit_now`가 `gate_open`을 먼저(막히면 설정도 그대로) |
| 탭 닫기(실행 중인 A 탭 자체) | 종전엔 닫혔다(결과가 갈 탭이 사라짐 · 세션은 계속 실행) | ⚠️ → **수정**: `editors.is_running(id)`면 거부 + "■로 중지한 뒤 닫으세요" |
| 미커밋 팝업 뒤 `commit_then`/`rollback_then` · `tx.close_action` 자동 커밋 | 문지기 없이 큐에 넣는다 — 워커가 순차라 실행 뒤 순서대로 처리(의도 · 3-2 그대로) | ✅ 유지 |
| 접속 창 Connect(추가 공유 연결) | `login_plan`/`placement` — 활성 탭 세션의 busy와 무관(다른 세션) | ✅ |
| 탐색기 열기/메타 | 메타 세션(별도) — 편집기 실행과 독립 | ✅ (D-46) |
| 세션 창 "접속 해제"(작업 중 세션) | `disconnect_session` → `stuck`이면 워커 교체(즉시 해제 규약) | ✅ |
| 탭 표식 메뉴(다른 공유 연결로 · 미연결) | 세션 바꾸기 = `blocked`와 무관(B는 A의 실행에서 벗어난다) | ✅ 의도 |

수정 3건(메뉴바 · 자동커밋 전환 · 실행 탭 닫기)은 09-19 63차 후반. 나머지 원칙: **새 진입점 = `gate_open()`**(T-122).

## 4. 전용 세션 — `CONNECT` / `DISCONNECT`

편집기에서 실행(Ctrl+Enter·F5). 실행 전에 호스트가 스크립트를 **파싱만** 해서(`sessions::connect_intent` — 첫 접속 명령) 세션 배치를 정한다.

```sql
CONNECT prod                                   -- ① 저장된 프로필 이름(nsql-vault · GUI 실행 인자와 같은 규칙)
CONNECT "oracle://scott/tiger@10.0.0.5:1521/ORCL"  -- ② 접속 문자열(URL · 따옴표 선택)
CONNECT "oracle:10.0.0.5:1521/ORCL"            -- ②′ 짧은 스킴 · 자격 없음 → 같은 대상의 프로필이 **하나뿐이면** 그 자격을 빌린다
CONNECT scott/tiger@10.0.0.5:1521/ORCL         -- ②″ SQL*Plus 꼴(방언 = 기본값)
CONNECT sqlite:/Users/me/demo.sqlite
DISCONNECT                                     -- 전용 세션을 닫고 공유 세션으로(개별 모드 = 실행 불가 상태)
```

| 상황 | 동작 |
|---|---|
| 공유 탭에서 `CONNECT` | 새 `Sess`(워커) → 탭 표식 🔌 → **스크립트 전체가 그 세션에서** 실행. 그 탭의 옛 결과는 `more=false` |
| 전용 탭에서 다시 `CONNECT` | 같은 세션이 대상을 바꾼다(워커가 기존 세션 커밋·닫기 → 새 접속) |
| 전용 탭에서 `DISCONNECT`(첫 접속 명령) | 미커밋 있으면 확인 팝업 → 세션 거둠 → 공유 복귀 · 나머지 문장은 실행하지 않는다 |
| `CONNECT a … DISCONNECT`가 한 스크립트에 | 전용 세션에서 돌고, 끝의 `DISCONNECT`가 세션을 거둔다(1회용 접속) |
| 공유 탭에서 `DISCONNECT` | 종전대로 공유 세션 해제(모든 탭 영향) |
| `CONNECT` 앞에 다른 문장이 있음 | 새 세션에는 아직 접속이 없어 그 문장은 "not connected" 오류 → D-99 |
| 상한 도달 | 상태줄·로그 안내(`StSessLimit`) · 실행하지 않음 |
| `session.private_connect = off` | 종전 동작(27 §6 ④ — 공유 세션을 바꾼다) |

**인라인 접속 문자열은 비밀번호가 있어야 연다(09-19 · `worker::password_required`)**. 종전의 자격 빌리기(`fill_credentials`: 비밀번호 없는 스펙이면 저장소에서 같은 서버·계정 프로필이 하나일 때 그 비밀번호를 몰래 씀)는 **제거** — 사용자가 `CONNECT oracle://BISCM@host:1521/BISCM`을 치면 저장 프로필 SNOP-DB(같은 서버·같은 계정)의 비밀번호로 붙어 "비밀번호 없이 접속됨"으로 보였다(분석 = journal 09-19 63차). 지금 규칙: SQLite가 아니고 호스트가 있는 스펙에 비밀번호가 없거나 비면 `ErrPasswordRequired`("Password required — include it in the connection string (user:pass@host) or use a saved profile") · 저장소 프로필 이름은 워커에 오기 전에 이미 채워져 온다. 비밀번호를 문자열에 쓰지 않는 방법(편집기 변수 · 모달 입력) = **T-132**.

## 5. 명령 이름 — 접두 문자가 필요한가 (조사)

**결론: 접두 없이 `CONNECT`/`DISCONNECT`로 전 DBMS에 안전하다.** 문장 **첫머리**가 `CONNECT`/`DISCONNECT`인 서버 SQL은 어느 DBMS에도 없고, 있는 곳에서는 뜻이 같다("클라이언트가 접속을 바꾼다").

| DBMS | 클라이언트 도구의 접속 명령 | 서버 문법에서 `CONNECT`가 나오는 곳 | 충돌 |
|---|---|---|---|
| Oracle | SQL*Plus `CONN[ECT]` · `DISC[ONNECT]` | `… CONNECT BY …`(절 · 문장 중간) · `GRANT CONNECT`(롤) · `ALTER SYSTEM DISCONNECT SESSION`(ALTER로 시작) | 없음 — 단 **선택 실행으로 `CONNECT BY …` 조각만 실행**하면 오인 → 가드 추가(아래) |
| SQL Server | sqlcmd `:connect` | `GRANT CONNECT …` · `DENY CONNECT SQL` | 없음 |
| PostgreSQL | psql `\c` · `\connect` | 없음(ECPG `CONNECT TO`/`DISCONNECT`는 전처리기 = 클라이언트) | 없음 |
| MySQL/MariaDB | mysql `connect` · `\r` | MariaDB `ENGINE=CONNECT`(문장 중간) | 없음 |
| SQLite | `.open` | 없음(`ATTACH`/`DETACH`) | 없음 |
| DB2 · Informix(ODBC 급) | **`CONNECT TO db USER u` · `DISCONNECT`가 SQL 문** | ISO SQL-92 "SQL-connection statements" | 뜻이 같다 — CLI/ODBC에서는 어차피 문장으로 보내지 않고 드라이버 API로 접속한다 · `CONNECT TO …` 꼴 파싱은 ODBC 드라이버 때(M4) |
| Tibero · Altibase | tbSQL · iSQL `CONNECT`/`DISCONNECT` | Oracle과 같음 | 없음 |

- 우리 분리기는 **문장 첫머리에서만** 명령을 본다 → `SELECT … START WITH … CONNECT BY …`는 SQL 하나로 남는다(실측 ✅).
- 새 가드: 첫머리가 `CONNECT BY <뒤에 더 있음>`이면 접속 명령이 아니라 SQL(서버가 구문 오류 · 무해) — 잘린 조각으로 전용 세션이 생기는 사고를 막는다. `CONNECT BY` 단독은 프로필 이름 `BY`.
- 접두 별칭은 **선택지로만**: `:connect`(sqlcmd · 기존) + `:disconnect`(신설 · 짝). psql `\c`는 T-52. 접두를 **강제**할 이유는 없다 — SQL*Plus/Golden 사용자 습관과 기존 스크립트 호환이 더 크다. → D-98.

## 6. 유휴 세션 — 서버·클라이언트 부하 검토

탭 수에 제한을 둘 수 없으므로(개별 모드) **접속은 필요할 때 열고, 안 쓰면 돌려준다.**

### 6-1. 방식 비교

| 방식 | 서버 비용 | 클라이언트 비용 | 부작용 |
|---|---|---|---|
| ⓐ 계속 유지 | 세션 메모리 상주(아래 표) · 라이선스/`processes`/`max_connections` 점유 | 소켓 1 · 스레드 1(잠듦) | 방화벽/NAT가 유휴 TCP를 끊으면 다음 실행에서 오류 → 자동 재접속 1회 |
| ⓑ keepalive 핑(`SELECT 1` 주기) | ⓐ + 주기 왕복 · **유휴 세션이 영원히 안 끊김**(DBA의 `IDLE_TIME`·`idle_session_timeout` 정책을 무력화) | 타이머 · 왕복 | 26 §8 "접속한 적 있는 서버라도 지속 트래픽 최소" 위반 소지 — **채택 안 함**(TCP keepalive는 OS에 맡김) |
| ⓒ **유휴 시 닫고, 다음 실행 때 재접속**(채택) | 0 | 재접속 1회(수십 ms~수백 ms · Oracle 전용 서버 프로세스 fork는 더 큼) | **세션 상태 소실**: `ALTER SESSION`·`SET`·임시 테이블·`search_path`·패키지 상태. 세션 변수는 클라이언트에 살아(DR-8) 남는다 |
| ⓓ 서버측 유휴화(Oracle DRCP · pgbouncer · MSSQL `sp_reset_connection`) | 풀 있는 곳만 작음 | 구성 의존 | 클라이언트가 고를 수 없다(서버 구성) — 있으면 ⓒ의 재접속이 싸질 뿐 |

### 6-2. DBMS별 분기

| DBMS | 유휴 세션 1개의 서버 비용 | 판정 |
|---|---|---|
| **Oracle** | 전용 서버 = **OS 프로세스 1 + PGA 수 MB** · `processes`/`sessions` 한도 · 라이선스(Named User) | **닫는 이득이 가장 큼** → ⓒ 기본. 재접속 비용도 가장 커서 한도는 길게(30분) |
| **PostgreSQL** | **백엔드 프로세스 1(5~10MB)** · `max_connections` 기본 100 | ⓒ. 트랜잭션이 열린 채 유휴(`idle in transaction`)가 최악(VACUUM 막음) → **열린 트랜잭션은 닫지 않되** 34의 오래된 미커밋 경고가 담당 |
| **SQL Server** | 스레드 아님(작업자 풀) · 세션당 수십 KB~ | 이득 작음 → 같은 규칙이되 급하지 않다. `#temp` 소실 주의 |
| **MySQL/MariaDB** | 스레드 1(thread cache) · `wait_timeout` 8h 기본 | ⓒ(M4) |
| **SQLite** | 서버 없음 · 파일 핸들 1 | **닫지 않는다** — 이득 0 · `:memory:`는 닫으면 데이터 소실 |
| ODBC 급 | 드라이버 의존 | ⓒ 기본 |

### 6-3. 구현 (`sessions::idle_action` · 순수 판정 + `App::idle_tick` 30초)

닫는 조건 = **접속됨 ∧ 한가함(`!blocked`) ∧ 열린 트랜잭션 없음 ∧ SQLite 아님 ∧ `idle ≥ session.idle_secs` ∧ (전용 세션 ∨ `session.idle_shared`)**.
- 열린 트랜잭션 = 대기 문장 있음 · 수동 커밋에서 조회만 한 상태(`tx_read`)도 포함.
- 닫을 때: 워커에 `Disconnect` → `idle_closed = true` · 스펙 유지 · 탭 표식 = 끊김 · 로그 1줄.
- 깨울 때(`wake_if_idle`): 실행·페치 **직전**에 같은 스펙으로 `ConnectSpec`을 먼저 보낸다(워커가 순차라 뒤따르는 작업은 접속 뒤에 돈다). 재접속은 사용자 동작 1회당 1회 — **자동 재시도·주기 핑 없음**(26 §8 준수).
- 열려 있던 서버 커서는 세션과 함께 사라진다 → 추가 페치는 OFFSET 폴백(43 §3-4 · 엄격 모드면 교체). 이미 있는 경로.
- 개별 모드: **한 번도 활성화되지 않은 탭은 접속하지 않는다**(세션 복원으로 탭 30개가 열려도 접속 0 → 보는 탭만 1개씩). 사용자 요구 "탭을 만들 때 바로 연결"의 부하 절충 → D-100.

### 6-4. 세션이 끊기면 무엇을 잃는가 — 유휴 닫기가 맞는 방법인가 (사용자 09-18 재검토)

**질문**: "세션에 연결해서 저장해 놓는 변수 같은 개념이 있는가 · Oracle의 `DECLARE`는 세션에 종속인가."

| DBMS | 세션에 **남는** 것(접속을 닫으면 사라짐) | 세션에 남지 **않는** 것 |
|---|---|---|
| **Oracle** | **패키지 전역 변수(패키지 상태)** — 세션마다 따로 · 호출 사이에 유지(사실상 "세션 변수") · 애플리케이션 컨텍스트(`DBMS_SESSION.SET_CONTEXT` → `SYS_CONTEXT`) · `ALTER SESSION`(NLS·`CURRENT_SCHEMA`·옵티마이저 파라미터) · `SET ROLE` · 전역 임시 테이블의 **데이터**(`ON COMMIT PRESERVE ROWS`) · 전용 임시 테이블(18c `ORA$PTT_`) · `DBMS_LOCK` 사용자 잠금 · `DBMS_OUTPUT` 버퍼 · 열린 커서 | **`DECLARE … BEGIN … END;`의 변수 = 그 블록 실행 동안만**(세션 종속 아님 · `END`에서 소멸) · SQL*Plus `VARIABLE` 바인드 변수 = **클라이언트**(우리도 DR-8로 클라이언트에 산다 → 재접속해도 남는다) |
| **SQL Server** | `#temp` 테이블 · `##global temp`(마지막 참조 세션까지) · `SET` 옵션(`ANSI_NULLS`·`LANGUAGE`·`DATEFORMAT`·`ROWCOUNT`·격리 수준) · **`USE db` 현재 DB** · **`SESSION_CONTEXT`**(`sp_set_session_context` · 2016+) · `CONTEXT_INFO` · `sp_getapplock`(Session 소유) · `EXECUTE AS` · 전역 커서 | **`DECLARE @v` = 배치 범위**(`GO`/실행 단위가 끝나면 소멸) — 그래서 우리가 DR-8로 클라이언트에서 이어 준다 |
| **PostgreSQL** | `SET`/`SET SESSION` GUC(`search_path`·`role`·`TimeZone`) · **사용자 정의 GUC**(`SET myapp.user = …` / `set_config` — 세션 변수 대용) · 임시 테이블 · `PREPARE` 문 · `LISTEN` 등록 · 세션 권고 잠금 · `WITH HOLD` 커서 | `DO $$ DECLARE … $$` 변수 = 블록 범위 · `SET LOCAL` = 트랜잭션 범위 |
| **MySQL/MariaDB** | **사용자 변수 `@v`(세션 범위 · 진짜 세션 변수)** · `SET SESSION` 시스템 변수 · `USE db` · 임시 테이블 · `PREPARE` · `GET_LOCK` · `LAST_INSERT_ID()` | 저장 프로시저 안의 `DECLARE` = 블록 범위 |
| **SQLite** | `PRAGMA`(접속별) · `ATTACH` · 임시 테이블 · `:memory:` 전체 | — (이미 닫지 않는다) |

**결론**: "세션 변수" 개념은 네 DBMS 모두에 있다(Oracle 패키지 상태/컨텍스트 · MSSQL `SESSION_CONTEXT` · PG 사용자 GUC · MySQL `@v`). 그러므로 **아무 세션이나 조용히 닫는 것은 틀렸다** — 사용자가 만들어 둔 세션 설정·임시 데이터를 말없이 무효화한다. 반대로 **조회·DML만 하고 커밋된 세션**은 닫아도 잃는 것이 없다(서버 쪽 상태 0 · 클라이언트 세션 변수는 DR-8로 유지).

→ **처방(D-103 ⓒ 구현)**: 세션마다 `stateful` 깃발. 실행 스크립트의 문장을 `sessions::alters_session_state`로 본다 — `ALTER SESSION`·`SET`·`USE`·`PRAGMA`·`ATTACH` · `TEMP`/`#` 임시 객체 · `PREPARE`·`LISTEN`·`LOCK` · **`BEGIN`/`DECLARE`/`EXEC`/`CALL`/`DO`(PL/SQL·프로시저는 패키지 상태·컨텍스트를 바꿀 수 있고 글만 봐서는 모른다 → 보수적으로 전부)** · `set_config(`·`pg_advisory_lock`·`GET_LOCK(`·`sp_set_session_context`·`DBMS_SESSION.`·`:=`. 하나라도 나갔으면 그 세션은 **유휴 닫기에서 제외**(`idle_action`의 조건). 새로 접속하면 깃발을 내린다.
- 어쩔 수 없이 세션이 바뀌었을 때(접속 오류 뒤 자동 재접속 · 사용자의 재접속)는 **한 번 알린다**: "새 서버 세션입니다 — 앞 세션의 세션 설정·임시 테이블·패키지 상태는 사라졌습니다"(로그 + 토스트 · `stateful`이었을 때만).
- 글만 봐서는 모르는 것(정직한 한계): Oracle 전역 임시 테이블에 대한 **평범한 INSERT** · 함수 호출 안에서 바뀌는 패키지 상태(`SELECT pkg.f FROM dual`). 전자는 수동 커밋이면 "열린 트랜잭션"으로 걸리고, 자동 커밋 + `PRESERVE ROWS`만 샌다 → 걱정되면 `session.idle_secs = 0`.
- 공유 세션은 기본 제외 그대로(`session.idle_shared = off`).

## 7. 화면

- **탭 표식**(nexa-ctl `TabBadge` · 이미지 버튼 모양 · hover/눌림): 🔌 강조색 = 전용 세션 접속됨 · 흐림+사선 = 끊김(유휴 닫힘·해제). 공유 세션 탭 = 표식 없음.
- 표식 **좌클릭·우클릭 = 세션 메뉴**: `전용 세션 — <접속 설명>`(제목) · **접속 해제하고 공유 세션으로 복귀**(개별 모드 = "이 탭 접속 해제") · **다시 접속** · (공유 연결이 둘 이상이면) **공유 연결 선택 ▸**.
- 탭 툴팁의 접속 줄 = 그 탭 세션의 설명 · 상태줄 앞 `[전용: oracle://…]`(Golden의 "Private Session:" 줄).
- 통제 중 = 툴바 문장 실행·전체 실행·Explain·Commit·Rollback + 결과 도구줄 새로고침·전체 조회·건수 **흐림** · ■만 활성 · 상태줄 ⏳.
- 트랜잭션 배지 `●n`은 **모든 세션**의 대기 문장을 탭별로 모은다. 트랜잭션 로그 창·실행 카드는 활성 탭의 세션 것.

### 7-2. 접속 UX 전체 설계 — 툴바 두 버튼 × 모드 × 탭 표식 (사용자 09-18 · 케이스 누수 없이)

**용어**: 활성 공유 연결 = 접속 창에서 마지막으로 붙였거나 사용자가 고른 공유 연결(`default_shared`). 탭의 연결 상태는 셋 중 하나 —
**공유**(공유 연결 중 하나에 묶임) · **전용**(그 탭만의 세션) · **미연결**(어떤 연결에도 묶이지 않음 = 접속 없는 전용 자리).

| 조작 | 공유 모드 | 개별 모드 |
|---|---|---|
| **툴바 Connect(플러그) 본체** | 접속 창 열기. 창에서 Connect = `login_plan`(같은 서버·계정 = 기존 연결 활성화 · 아니면 **추가**) → 그 연결이 활성 공유 연결 · **지금 탭이 미연결/전용이어도 탭은 바뀌지 않는다**(탭 연결은 표식으로) | 접속 창 열기. 창에서 Connect = **지금 탭의 세션**에 접속(미연결 자리면 그 자리에) · 새 탭의 기본 접속 정보 갱신 |
| 플러그 **색** | 지금 탭의 연결: 초록 = 연결됨 · 빨강 = 끊김 확인(53) · 기본 = 미연결 | 같음 |
| **툴바 Disconnect** | **지금 탭의 연결 해제**(종전 · 사용자 09-18 원복 · 이 탭에 연결이 있을 때만 활성 · 전용 탭이면 그 세션 · 공유 탭이면 그 공유 연결) — 모두 해제는 세션 창 우클릭 | 같음 |
| **툴바 세션 목록 버튼**(`conn.sessions`) | **세션 창**(별도 창 · `sessions_win.rs` · 61차) — 서버 머리줄 아래 세션 줄(● 활성 공유 · 공유 · 🔌[탭] · 상태/유휴/대기/탭) · 우클릭 = 활성 연결로 / 탭으로 / 다시 접속 / 접속 해제 · 빈 곳 = 모두 해제 · 더블클릭 = 탭으로/활성 연결로. View ▸ Session Manager · 팔레트도 같은 창 | 같음 |
| **새 탭** | 그때의 **활성 공유 연결에 바로 묶임**(뒤에 활성 연결을 바꿔도 이 탭은 그대로 · 바꾸려면 표식) | **미연결 자리 + 접속 창이 바로 열림** · 접속하지 않고 닫으면 미연결 |
| **탭 표식(플러그 이미지 버튼)** 보임 | **항상**(사용자 09-18) — 중립 플러그 = 공유 · 강조 플러그 = 전용 · **사선 플러그 = 미연결·끊김** | 같음(전용 또는 미연결) |
| 표식 **메뉴** | `No connection` · 공유 연결 목록 · (전용이 있을 때만) 구분자 + 전용 연결 — **지금 것 앞에만 ✓**(배타) · 글자 열 정렬 · 그 외 문구 없음. 공유 줄 = 그 연결로(전용은 폐기) · `No connection` = 전용 해제/미연결 · 전용 줄 = 끊겨 있으면 다시 접속 | 같음(공유 연결이 없으면 `No connection`과 전용 줄만 · 접속은 툴바 Connect) |
| 전용 탭에서 공유 연결 선택 | **전용 연결 바로 폐기**(미커밋이 있으면 묻는다) → 그 공유 연결에 묶임 · 옛 결과는 이어 받기 불가(`more=false`) | 같음(개별 모드에서도 공유 연결이 있으면) |
| 탭에서 `CONNECT …` 실행 | 공유 탭 = **새 전용 연결 추가**(앞 문장 있으면 거부 D-99) · 전용 탭 = 기존 연결을 끊고 새로 — 단 **같은 서버·계정**이면 설정 `connect.reconnect_same`: 끔 = 기존 유지(CONNECT 명령만 지우고 나머지 실행 · 로그 1줄) · 켬 = 끊고 다시 | 같음 |
| 탭에서 `DISCONNECT` | 전용 탭 = 전용 연결 거둠 → 공유 복귀 · 공유 탭 = 공유 연결 해제(종전) | 전용 탭 = 끊긴 채(미연결) |
| 표식/세션 관리자의 **다시 접속** | 사용자의 명시적 요청 → `connect.reconnect_same`이 켜져 있으면 같은 서버라도 끊고 다시 · 끄면 살아 있으면 유지 | 같음 |
| 끊김 확인(53 Broken) | 표식 사선 + 플러그 빨강 + 목록 "· 끊김" · 다음 동작 때 판정 → 재접속 | 같음 |
| 탭 닫기 | 전용 세션은 함께 거둠(미커밋은 묻는다) · 공유 묶음만 풀림 | 세션 거둠 |
| 접속 창을 열어 두고 다른 탭으로 전환 | 접속 대상은 **창을 연 시점의 규칙**(공유 모드 = 공유 · 개별 모드 = 그때의 활성 탭) — `login_place`가 시도 시점의 활성 탭을 본다 | — |

**누수 점검(상태 × 동작)**: 탭 상태 3 × {툴바 Connect, 툴바 Disconnect 본체/▾, 표식 메뉴 5항목, CONNECT/DISCONNECT 명령, 새 탭, 탭 닫기, 끊김} 을 위 표가 덮는다. 남은 정직한 틈: ① 접속 창을 열어 둔 채 모드 설정을 바꾸면 창의 다음 Connect는 새 모드 규칙 ② 개별 모드에서 파일 열기로 생긴 탭도 "새 탭"이라 접속 창이 뜬다(의도 · 원치 않으면 Esc = 미연결).

## 8. 잃는 순간 (DR-30 연장)

| 순간 | 처리 |
|---|---|
| 전용 탭 닫기 | 그 세션의 대기 문장 **전부**가 대상(`tx.close_action`) · 다른 탭이면 먼저 그 탭을 앞으로 → 확인 → 세션 거둠(실행 중이면 취소 먼저) |
| 전용 세션 해제(표식·`DISCONNECT`·툴바) | 확인 팝업 → 거둠 |
| 종료 | 세션마다 한 번씩 묻는다(`request_exit`가 미커밋 세션의 탭을 차례로 앞으로) |
| 유휴 닫기 | 열린 트랜잭션이 있으면 **닫지 않는다** |

## 9. 설정 · 원장

| 키 | 기본 | 뜻 |
|---|---|---|
| `session.mode` | `shared` | `shared` / `per-editor` |
| `session.private_connect` | on | `CONNECT` = 탭 전용 세션(끄면 공유 세션 교체 = 종전) |
| `session.max_shared` | **8** | 공유 연결 상한(동시에 붙어 있을 서버 수) |
| `session.max_private` | 8 | 전용 세션 상한(개별 모드의 탭 세션 포함 · 넘으면 그 탭은 공유 세션) |
| `session.idle_secs` | 1800 | 유휴 닫기(0 = 끔 · 세션 상태를 바꾼 세션·열린 트랜잭션·SQLite 제외 · 탐색기 메타 세션에도 같은 한도) |
| `session.idle_shared` | off | 공유 세션도 유휴 닫기 |

- **26 §8 네트워크**: 전용/개별 접속 = 사용자 동작 1회당 1접속 · 자동 재시도 없음 · 유휴 뒤 재접속 = 다음 실행 1회 · 주기 트래픽 0 · 동시 접속 상한 = `session.max_private` + 공유 + 탐색기 메타 1.
- **39 §3 부하원**: DB 접속 수(`session.max_private`) · 워커 스레드 수(같은 키) · 유휴 점검 타이머 30초(`session.idle_secs = 0`이면 즉시 반환).

## 10. 결정 (사용자 09-18 "결정에 따른 개발도 모두 진행" → 권장안 확정)

| # | 결정 | 구현 |
|---|---|---|
| D-96 | 전용 = 공유 모드 위의 **탭 속성**(별도 설정값 없음) | ✅ |
| D-97 | 개별 모드의 접속 창 Connect = **활성 탭의 세션** + 새 탭의 기본 스펙 갱신 | ✅ `login_place` |
| D-98 | 접두 강제 없음 · `:connect`/`:disconnect`만(psql `\c`는 T-52) | ✅ |
| D-99 | 공유 탭에서 `CONNECT` **앞에 서버로 갈 문장이 있으면 실행 거부 + 안내**(전용 탭은 허용 — 앞 문장은 지금 세션에서 돈다) | ✅ `statements_before_connect` · `Placement::Refuse` |
| D-100 | 개별 모드 접속 = 탭이 **처음 활성화될 때** | ✅ `sync_sess` |
| D-101 | 공유 연결 N = **추가**(상한 **8** · 사용자 09-18) | ✅ §2-1 |
| D-102 | 유휴 닫기 = 전용만 30분 · 공유는 설정 | ✅ |
| D-103 | **세션 상태를 바꾼 세션은 닫지 않는다** + 세션이 바뀌면 1회 안내 | ✅ §6-4 |
| D-104 | 뒤에서 끝난 실행 = 탭 제목 앞 **✓/✗**(그 탭을 보거나 다시 실행하면 지움) | ✅ `Editors::set_done_mark` |
| D-105 | 트랜잭션 로그 = **전 세션 합본 + "세션" 열**(세션이 둘 이상일 때만 보임 · 트랜잭션·문장 번호는 세션별 격리) | ✅ `TxLog::select_session` · `TxFilter.session` |
| D-106 | 탐색기 = 서버별 · 세션 ≥1이면 유지 · 0이면 오프라인(트리 유지 · 수동 제거) · 메타 세션 유휴 회수 | ✅ §2-2 `ExplorerSet` |
| D-107 | "같은 서버" = 방언·호스트·포트·DB·**계정·역할**까지 같을 때(계정이 다르면 다른 연결) | ✅ `same_server` |
| D-108 | 묶인 탭의 연결이 끊기면 **끊김 표식으로 남김**(자동 이동 없음) | ✅ |

## 11. 할 일

| # | 내용 | 선행 |
|---|---|---|
| **T-121** | 🚧 서버별 탐색기 ✅(§2-2) · 잔여 = `MetaStore` 인텔리센스를 서버 키로 묶기 · 오프라인 메타 LRU(`meta.cache_mb`) · 노드 우클릭 "이 서버로 새 탭" · 접속 창 다중 "접속됨" 표시 · `tab_bind` 세션 복원 | T-57 |
| **T-122** | 30 §1-2 체크리스트에 "DB로 가는 새 진입점 = `gate_open()`" 추가 · `worker.send` 직접 호출을 `Sess` 메서드 뒤로 숨기기(문지기 우회 불가) | — |
| **T-123** | ✅ 09-18 세션 상태 추적(§6-4) — 잔여 = 자동 커밋에서 Oracle GTT(`PRESERVE ROWS`) INSERT 감지(카탈로그 조회 필요) | — |
| **T-124** | ✅ 09-18 트랜잭션 로그 세션 열 — 잔여 = 실행 로그 창의 세션 태그 · 세션 필터 UI | — |
| **T-125** | 실기(Windows·mac): §13 점검표 | — |
| **T-126** | CLI `nsql shell` — 전용 세션 개념 없음(세션 1개) 확인 · 27 §6 ④ 문구 갱신 | — |

## 12. 판정 로직 MC/DC 점검 (사용자 09-18)

분기 판단을 화면·워커에서 떼어 **순수 함수**(`sessions.rs`)로 모으고, 조건 하나하나가 결과를 **단독으로 뒤집는 쌍**을 테스트로 고정했다(수정 조건/결정 커버리지). 호스트는 사실만 모아 넘긴다 → 시험한 로직 = 도는 로직.

| # | 결정 | 조건(원자) | 독립 영향 쌍(테스트) | 상태 |
|---|---|---|---|---|
| D1 | `route_tab` 탭 → 세션 | private 있음 · bound 있음 · bound_alive | 기준(bound) ↔ 각 1개씩 뒤집기 3쌍 | ✅ `mcdc_route_tab` |
| D2 | `login_plan` 접속 배치 | same_server · (connected ∨ idle_closed ∨ blocked) · blocked · 재활용 4조건(!connected · !blocked · !idle_closed · bound_tabs=0 ∨ only) · len<max | Use↔New · Use↔Busy · dead-same↔idle-same · Recycle↔New ×4 · only · New↔Limit | ✅ `mcdc_login_plan`(12쌍) |
| D3 | `placement` 실행 배치 | intent 종류 · is_private · private_connect · **preceded** | NewPrivate↔Retarget↔Run · **NewPrivate↔Refuse(preceded만)** · 전용 탭은 preceded 무관 · ClosePrivate↔Run | ✅ `mcdc_placement` |
| D4 | `reap_shared` | connected · blocked · idle_closed · is_default · bound_tabs | true 기준 ↔ 5개 각각 | ✅ `mcdc_reap_shared` |
| D5 | `badge_kind` | private · connected(항상 표시 · 60차) | Shared↔Private(private만) · Private↔Off · Shared↔Off | ✅ `mcdc_badge_kind` |
| D6 | `gate_view` 통제 표시 | busy · aux>0 · multi_caret · has_pending | busy · aux 각각 단독으로 전체 차단 · multi_caret는 문장 실행만 · has_pending은 Commit만 | ✅ `mcdc_gate_view` |
| D7 | `idle_action` | limit=0 · connected · blocked · tx_open · **stateful** · SQLite · (private ∨ include_shared) · idle≥limit | Close 기준 ↔ 8개 각각 + include_shared 쌍 | ✅ `idle_close_only_when_safe` |
| D8 | `connect_intent` / `is_connect_by` | 첫 접속 명령 종류 · `CONNECT BY` + 뒤 토큰 유무 | 프로필/문자열/DISCONNECT 선행/문장 중간 CONNECT BY/조각/`CONNECT BY` 단독 | ✅ `connect_intent_…` · `connect_by_fragment_…` |
| D9 | `ConnectSpec::parse` 짧은 스킴 | 방언 이름 · `@` 유무 · logon에 `/` | `oracle:host…`(대상만) · `oracle:u/p@h` · `postgres:pw@h`(종전 유지) · sqlite 축약/종전 URL | ✅ `quoted_short_scheme_…` |
| D11 | `pane_move` 탐색기 유지 | 서버 키 있음 · 온라인 · 붙은 세션 수=0 | GoOffline 기준 ↔ 3개 각각(세션 1·3 = 유지) | ✅ `mcdc_pane_move` |
| D12 | `alters_session_state` | 문장 종류(세션 설정·임시 객체·PL/SQL·세션 함수 vs 조회·DML·영구 DDL) | 참 23종 · 거짓 9종 표 | ✅ `session_state_classifier` |
| D13 | `statements_before_connect` | 접속 명령 앞의 항목 종류(SQL·EXEC vs 클라이언트 명령) | 4쌍 | ✅ |
| D14 | `TxLog` 세션 격리 | 세션 선택 · 같은 index · 세션별 열린 트랜잭션 · 세션 필터 | 두 세션 교차 시나리오 | ✅ `sessions_are_isolated_in_one_log` |
| D10 | `password_required` 인라인 비밀번호 필수(자격 빌리기 제거 09-19) | 방언 ≠ SQLite · 호스트 있음 · 비밀번호 없음/빈 값 | 기준 1 + 조건별 반전 4 | ✅ `password_required_mcdc` |

**순수 함수 밖에 남은 분기(호스트 · 실기로만 확인 = §13)**: ① `with_sess` 맞바꾸기 복귀(세션이 안에서 사라진 경우) ② `RunEvent::Disconnected`의 "전용 ∧ !idle_closed ∧ 공유 모드 → closing" ③ `done`의 `skip_done` ④ `disconnect_private`의 (지금 세션 ∧ 미커밋 → 팝업 / 다른 세션 ∧ 미커밋 → 거부) ⑤ `open_disconnect_menu`의 "대상 1개 ∧ 활성" 바로 해제. ②④⑤는 조건 2~3개짜리라 다음에 순수 함수로 뽑을 후보(T-122에 포함).

## 13. 실기 점검표 (T-125)

① 공유: 탭 A에서 긴 질의 실행 → 탭 B로 전환 → 문장/전체 실행·Explain·Commit·Rollback 흐림 · 결과 ⇊/Σ/새로고침 흐림 · ■만 활성 → 끝나면 전부 복귀
② 탭 B에서 `CONNECT Demo` 실행 → 탭 앞 🔌 · 상태줄 `[Private: …]` · 툴팁 접속 줄 · 탭 A 실행 중에도 **탭 B는 실행 가능**
③ `CONNECT "sqlite:<경로>"` · `CONNECT "oracle:host:1521/svc"`(프로필 1개 일치 → 자격 빌림 · 2개면 오류)
④ 표식 우클릭/좌클릭 → 메뉴 → "해제하고 공유 복귀" → 표식 사라짐 · 옛 결과 ⇊ 흐림 · 이후 실행은 공유 세션
⑤ `DISCONNECT` 타이핑 실행 → ④와 같음 · 수동 커밋 + 미커밋 상태에서는 확인 팝업
⑥ `session.mode = per-editor` → 접속 창 Connect → 새 탭 활성화마다 🔌 · 한 탭 해제 → 끊김 표식 · 그 탭 실행 = "not connected" · 메뉴 "다시 접속"
⑦ `session.idle_secs = 60` → 전용 탭 1분 방치 → 표식 끊김 + 로그 → 실행 → 재접속 로그 뒤 결과
⑧ 전용 탭 닫기(미커밋 있음/없음) · 종료(두 세션에 미커밋) → 세션마다 한 번씩 묻는가
⑨ 선택 실행으로 `CONNECT BY PRIOR …` 조각 → 전용 세션이 생기지 않고 서버 구문 오류
⑩ 포커스 링·hover 규칙: 탭 표식 클릭 뒤 링 0 · 메뉴 바깥 클릭이 그대로 진행
⑪ 공유 N: 서버 A 접속 → 접속 창에서 서버 B Connect → **A 유지** · 툴바 Disconnect ▾에 둘 다 · 새 탭은 B · A에서 실행했던 탭은 A 그대로(표식 🔌 중립색 · 툴팁)
⑫ 같은 프로필 다시 Connect → 연결 수 그대로(중복 없음) · 그 연결이 활성으로
⑬ Disconnect ▾ → 연결이 하나여도 목록이 뜬다 · A 줄 클릭 → A에 묶인 탭 = 끊김 표식 · 표식 클릭 → B 선택 → 실행 가능 · 탐색기는 활성 연결을 따름
⑭ A 탭 실행 중에도 B 탭은 실행 가능(세션이 다르다) · A를 쓰는 다른 탭은 막힘
⑯ 서버 A·B 공유 연결 → 탐색기에 루트 A 아래 루트 B(접속 순) · 둘 다 펼치면 B의 루트는 A의 마지막 행 바로 아래 · 스크롤은 전체에 하나 · A를 해제해도 A에 전용 세션 탭이 있으면 A 탐색기 유지 · 마지막 세션까지 끊으면 "오프라인"(트리는 남음) · 루트 우클릭 → 탐색기에서 제거
⑰ 전용 탭에서 `ALTER SESSION …` 실행 → `session.idle_secs = 60`이어도 닫히지 않음 · 조회만 한 전용 탭은 닫힘 · 접속 오류 뒤 재접속되면 "새 서버 세션" 토스트 1회
⑱ 공유 탭에서 `select 1; CONNECT Demo` 실행 → 거부 토스트 · 전용 탭에서는 실행됨
⑲ 탭 A 실행 중 탭 B로 이동 → A가 끝나면 A 제목 앞 ✓(실패 ✗) → A를 보면 사라짐
⑳ 두 세션에서 수동 커밋 DML → 트랜잭션 로그에 "세션" 열 · 한쪽만 롤백해도 다른 쪽은 "대기"
⑮ `session.max_shared = 2`에서 세 번째 서버 Connect → 접속 창 실패 표시 + 안내
