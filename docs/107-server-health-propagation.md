# 107 · 서버 연결 끊김 — 다계층 감지 · 기능별 반응 · 전파(ServerHealth) 설계(10-08 · 사용자 실기 사건)

> 사건(사용자 10-08): VPN으로 DBMS에 붙어 작업하던 중 VPN이 끊겼다. 그 뒤 객체 탐색기에서 프로시저 **소스 열기**를 눌렀는데 **아무 메시지 없이 동작만 안 됐다**.
> 요구: ① 다계층에서 이 현상을 막고 안내를 정확하게 ② 각 기능이 끊김에 어떻게 반응할 수 있는지 ③ 끊김이 확인되면 "연결 정보에 문제가 있다"를 프로그램 전체에 어떻게 전파할지 ④ (질문) 이때 편집기 탭의 연결 상태는 어떻게 갱신되는가.
> 선행 설계 = [53 접속 생존 관리](53-connection-liveness.md)(실행 세션 · Broken · `ensure_alive`) · [52 세션 모드](52-session-modes.md)(DR-34 통제 단일화) · [28 객체 탐색기](28-object-explorer.md)(오류 비확산 · `explorer.timeout`) · [26 §8 네트워크 부하 규칙](26-performance-architecture.md).

## 0. 사건 재구성 — 왜 "조용히" 실패했나(코드 확인 10-08)

소스 열기는 **실행 세션이 아니라 탐색기의 메타 세션**(서버 칸마다 `meta_thread` 한 개 · 별도 접속)으로 간다. 그 길에는 53의 생존 관리가 **세 겹 모두 빠져 있다**.

| 겹 | 실행 세션(53 §8 · 있음) | 메타 세션(탐색기 · 지금) | 사건에서 벌어진 일 |
|---|---|---|---|
| 동작 직전 판정 | `ensure_alive` = SYN 1회(`probe.timeout_ms`) · 마지막 성공 뒤 `probe.stale_secs`(60 s) 지나면 반드시 | **없음** — `reachable(spec)`은 유휴로 닫힌 세션을 **다시 열 때만**(`resume`) | 세션 객체는 살아 있다고 믿고 죽은 소켓에 질의 |
| 호출 상한 | `session.call_timeout_secs`(Oracle · 기본 0) · 노드 로드 `explorer.timeout` 15 s(`loading_since`) | `Req::Source`·`Details`·`GenSql`·`Sizes`는 **`loading_since` 밖** → 상한 0 · Oracle 메타 세션 call timeout 0 | 드라이버가 TCP 재전송 타임아웃(수 분)까지 갇힘 · 메타 스레드는 한 번에 한 질의라 뒤 요청도 전부 대기 |
| 안내 | Broken → 탭 표식·플러그 빨강·상태줄·로그 1줄(`sync_sess_ui`) | 시작 = 상태줄 `ExpLoadingSource` 한 줄 · 끝 = `ExplorerAction::Status(e)` **상태줄 한 줄** · 바쁨 표시 없음 · 토스트 없음 · 로그 없음 | 상태줄 글은 다음 상태 글에 바로 덮임 → "아무 메시지 없음"으로 보임 |

덧붙여 **실행 세션의 Broken은 메타 세션으로, 메타 세션의 실패는 실행 세션으로 전해지지 않는다**(같은 서버인데 상태가 둘). 끊김을 안 쪽이 있어도 다른 쪽은 모른 채 다음 동작에서 또 갇힌다.

## 1. (질문) 지금 편집기 탭의 연결 상태는 어떻게 갱신되는가

**사건 기반**이다 — 주기 핑은 없다(26 §8 "접속한 적 없는 서버에 지속 트래픽 금지 · 자동 재접속 금지"의 연장으로 D-109에서 "동작 직전 판정"을 택했다).

1. 탭이 묶인 세션에 **동작**(실행 · 페치 · 건수 · 키 조회 · 커밋/롤백 · 접속)이 오면 워커가 `ensure_alive`를 먼저 돈다 = `live_plan(preflight, suspect, dead_hint, stale, allow, auto)`(sessions.rs · MC/DC) → 판정이 필요하면 호스트:포트 **TCP SYN 1회**.
   - `dead_hint` = 드라이버 0-왕복 상태(`Session::is_alive` · Oracle `status()` · PG `is_closed()`).
   - `stale` = 마지막 성공 뒤 `probe.stale_secs`(60 s) 지남 → VPN이 끊긴 지 1분 이상이면 다음 동작은 반드시 판정부터.
   - 그 안(60 s 이내)이고 드라이버 힌트도 없으면 **판정 없이 질의** → 접속성 오류(`probe::is_connection_error` · 코드/문구 표)로 끝나면 그때 Broken.
2. 죽었으면 `ConnOutcome::Broken(사유)` 1회 → `Sess.broken = true` → `sync_sess_ui()` = **탭 표식 끊김 · 툴바 플러그 빨강 · Disconnect ▾ "· 끊김" · 세션 창 목록 · 상태줄 `[끊김] 설명 - 사유` · 로그 1줄**(events.rs). 그 동작 자체는 오류로 끝난다(막힘 0).
3. 다음 동작에서 `connect.auto_reconnect`(기본 켬)이고 판정이 살아 있으면 같은 스펙으로 **조용히 재접속** → `ConnOutcome::Alive` → 표식 복귀 · 상태 있는 세션이면 "세션 상태 소실" 안내(52 §6-4) · 열린 트랜잭션 = Lost.

따라서 **VPN이 끊긴 직후 편집기 탭은 "연결됨"으로 보이고**, 사용자가 무언가를 실행하는 순간(또는 60 s 뒤 첫 동작)에야 끊김으로 바뀐다. 탐색기만 만지고 있었다면 실행 세션은 끝까지 모른다. 이것이 ③ "전파"가 필요한 이유다.

## 2. 목표와 원칙

| # | 원칙 | 근거 |
|---|---|---|
| P1 | **조용한 실패 0** — 서버에 가는 모든 사용자 동작은 ≤ `probe.timeout_ms`(1 s) + 호출 상한 안에 성공·실패 중 하나로 끝나고, 실패는 **보이는 채널**(토스트 또는 그 자리의 표식) + 로그에 남는다 | 사건 |
| P2 | **서버 단위 한 상태** — 같은 끝점(host:port)에 붙은 실행 세션 N + 메타 세션 1은 건강 상태를 하나로 본다. 어느 쪽이 끊김을 알아도 전부에 즉시 반영 | §0 둘째 문단 |
| P3 | 네트워크 규칙 유지 — **주기 핑 없음** · 판정은 동작 직전 또는 OS 신호 때 · 자동 재접속은 사용자 동작의 일부로만 | 26 §8 · D-109~113 |
| P4 | 통제 단일화 — 끊김에 따른 "막기/허용/안내"는 `gate_open`/`sync_gate`/`gate_view` 한 길에 합류(DR-34). 기능마다 따로 if를 두지 않는다 | 52 §3 |
| P5 | 오프라인에서도 되는 것은 된다 — 캐시(메타 L1/L2 · 탐색기 트리 · 받은 결과)는 읽기·복사·필터 허용, 서버가 필요한 것만 막는다 | 61 §1-8 지연 로딩 · 85 |
| P6 | 안내는 **원인 + 끝점 + 다음 행동**을 담는다("서버에 닿지 않음 192.168.0.58:1521 · SYN 1.0 s 실패 · VPN/네트워크 확인 뒤 [다시 연결]") | 사용자 "안내를 정확하게" |

## 3. 아키텍처 — `ServerHealth` 레지스트리 + 감지 4층 + 전파 한 자리

```
감지(어디서든)                    레지스트리(App 소유 · 끝점 키)            전파(한 자리 · 구독자)
L0 OS 네트워크 변경 신호 ─┐      ServerHealth { ep, state, since,        App::health_changed(ep, state, src)
L1 동작 직전 판정(SYN)   ─┤──▶     reason, src: {Run n, Meta, Probe} } ──▶  · Sess.broken(그 끝점의 세션 전부)
L2 호출 상한(타임아웃)   ─┤      state = Alive | Suspect | Broken |          · sync_sess_ui(탭 표식·플러그·세션 창)
L3 접속성 오류 분류      ─┘              Reconnecting                         · ExplorerSet 헤더(플러그·"끊김 hh:mm")
L4 복구(재접속·재개) ───────▶   Alive 복귀                                    · 토스트 1회 + 로그 1줄(억제 창)
                                                                                · sync_gate(막힘·비활성·메뉴 라벨)
```

### 3-1. `ServerHealth`(새 모듈 `app/health.rs` · 순수 판정은 `sessions.rs`에)

- 키 = 끝점 `(host, port)`(SQLite 파일 = 늘 Alive · 파일 없음은 다른 오류). 값 = `state` · `since` · `reason`(사유 글 · 코드) · `src`(마지막으로 판정한 출처).
- **순수 함수** `health_merge(prev, event) -> next`(MC/DC 시험): 사건 = `ProbeDown(ep)` · `ConnError(ep, code)` · `CallTimeout(ep)` · `NetChanged` · `ProbeUp`/`Connected(ep)`. 규칙 = Down/ConnError/Timeout → Broken(처음이면 알림) · NetChanged → Alive→Suspect(알림 없음 · 다음 동작이 판정) · Up/Connected → Alive(끊겼던 것이면 복귀 알림).
- 실행 세션 워커의 기존 `ConnOutcome::Broken/Alive`와 메타 스레드의 새 `Resp::Health(ep, event)`가 **둘 다 이 레지스트리로** 들어온다 — `Sess.broken`은 레지스트리의 투영이 된다(한 끝점의 세션 전부에 같은 값).

### 3-2. 감지 4층(기존 ↔ 추가)

| 층 | 무엇 | 지금 | 추가 |
|---|---|---|---|
| **L0** OS 신호 | 네트워크 경로 변경(VPN 어댑터 내려감 · 기본 라우트 변경) → 모든 끝점 **Suspect** · `last_ok` 초기화 | Windows · macOS 구현(10-09 Windows · 10-10 macOS SystemConfiguration) · Linux = `probe.stale_secs` 60 s가 대신 | nexa-sys `netwatch`(Windows `NotifyIpInterfaceChange`/`NotifyRouteChange` · mac `SCNetworkReachability` · Linux netlink `RTMGRP_LINK`) → 이벤트 루프 `Wake::Net` · 부하 = 콜백뿐(0) · 설정 `net.watch`(기본 켬) |
| **L1** 동작 직전 판정 | 서버에 가기 전 SYN 1회(`probe.timeout_ms`) · 조건 = `live_plan` | 실행 세션만 | **메타 세션에도 같은 `ensure_alive`**(`MetaGuard` = 스레드 안 `last_ok`·`suspect` · 카탈로그 요청마다 `live_plan` → 판정 필요하면 `reachable` → 죽었으면 `Resp::Health(Broken)` + 그 요청은 즉시 실패) · 레지스트리가 이미 Broken이면 **UI에서 먼저 막고**(gate) 스레드까지 안 보냄 |
| **L2** 호출 상한 | 질의 하나의 상한 | 노드 로드 15 s(`loading_since`) · Oracle call_timeout 0 | ① `Req::Source/Details/GenSql/Sizes/DbSizes/Live/Blockers`도 `loading_since` 표에(키 = 요청 id) → `explorer.timeout` 지나면 그 자리 오류 + 늦은 응답 버림(세대) ② 메타 세션 `set_call_timeout(explorer.timeout)`(Oracle · 질의 자체를 끊어 스레드 큐가 풀리게) · PG `statement_timeout`은 서버 쪽이라 끊김엔 무력 → 소켓 read timeout(드라이버 어댑터) · MSSQL read timeout(53 §8 보류분) ③ 상한 초과 = `CallTimeout(ep)` → Suspect(끊김 확정은 L1이) |
| **L3** 오류 분류 | 질의가 접속성 오류로 끝남 | `probe::is_connection_error`(실행 세션) | 메타 세션 결과도 같은 분류로 → `ConnError(ep)` → Broken |
| **L4** 복구 | 재접속 | 다음 동작 때 `connect.auto_reconnect` · 메타 세션은 `resume` | 토스트 [다시 연결] 버튼 = 그 끝점의 실행 세션 재접속 + 메타 세션 `Req::Open` 재개 · 탐색기 헤더 우클릭 "다시 연결" · 성공 = `Connected(ep)` → Alive 복귀 알림 |

## 4. 기능별 반응 표(사용자 요구 ②) — 끊김(Broken) 상태에서 그 기능을 쓰면

반응 종류: **막기**(gate · 서버에 안 보냄 · 안내) · **판정 뒤**(Suspect일 때 SYN 1회 → 살아 있으면 진행/재접속 · 죽었으면 막기) · **허용**(서버 불필요 · 캐시) · **배경 정지**(사용자 동작 아님 · 조용히 멈추고 복귀 때 재개).

> **10-09 해석(④ 구현 · 6e8a347)**: 표의 "막기"는 **UI 게이트에서 동기로 막는다는 뜻이 아니다** — 게이트에서 SYN을 기다리면 UI가 멈추므로, 실행·페치·건수·커밋은 지금처럼 워커가 비동기로 판정(`ensure_alive` · 레지스트리가 Broken/Suspect면 preflight)해 서버에 보내기 전에 실패시키고 안내한다. UI 게이트에서 바로 막는 것은 **서버에 오래 붙는 작업의 시작**(Import)뿐 · 배경 워머는 끊긴 동안 아예 돌지 않는다.

| 기능 | 서버 필요 | Suspect | Broken | 안내 채널 | 비고 |
|---|---|---|---|---|---|
| 실행(F5 · Ctrl+Enter · 전체/현재 문) | ○ | 판정 뒤 · 살아 있으면 자동 재접속 | 막기 → 토스트 1회(억제 창 안 중복 = 상태줄만) | 토스트 · 상태줄 · 탭 표식 | 53 §8 그대로 + 레지스트리 선판정 |
| 페치 더 · 전체 조회 · 건수 Σ · 키 조회 | ○ | 판정 뒤 | 막기 · 받은 행은 그대로(P5) | 상태줄 · 결과 도구줄 비활성 흐림 | `gate_view` 합류 |
| **커밋 / 롤백** | ○ | 판정 뒤 · **재접속 금지**(D-109) | 막기 + **"서버가 세션을 끊어 미커밋 변경은 서버 쪽에서 롤백됐을 가능성"** 안내 · 트랜잭션 로그에 Lost | 토스트(경고) · 트랜잭션 로그 · 탭 표식 | 52 §6-4 |
| 탐색기 펼침 · 새로 고침 · 메타 새로 고침 | ○ | 판정 뒤(메타 `MetaGuard`) | 막기 · 트리는 그대로(P5) · 노드에 "끊김" 표식 | 헤더 플러그 빨강 · 노드 글 · 상태줄 | 28 오류 비확산 유지 |
| **소스 열기 · DDL 생성(SQL Preview) · 상세 패널 펼침 · 용량 확인** | ○ | 판정 뒤 | 막기 → **토스트**(끝점 · 사유 · [다시 연결]) | 토스트 · 헤더 혜성(진행 중) · 상태줄 · 로그 | ★ 사건의 길 · 진행 중 = `BusyRing` · 15 s = 오류 |
| 탐색기 검색 · 타입어헤드 · 필터 | △(인덱스 미완이면 서버) | 캐시만 · 미완 스키마는 "끊김 · 캐시만" | 허용(캐시) | 헤더 "인덱싱 d/T(끊김)" | 84 §2 |
| 인텔리센스 완성 · 시그니처 · Ctrl 링크 툴팁 | △(캐시 우선) | 캐시만 · 워머 정지 | 허용(캐시) · 없으면 "메타 없음(끊김)" 카드 한 줄 | 팝업 안 한 줄 | 배경 워머 = **배경 정지** · 토스트 없음 |
| 메타 워머(L1/L2 선적재 · 코멘트 워머 · 사전) | ○(배경) | 정지 | 정지 · Alive 복귀 때 재개 | 로그 1줄 | 사용자 동작 아님 → 알림 금지(26 §8) |
| 그리드 셀/행 편집 **적용** | ○ | 판정 뒤 | 막기 · **변경 목록 보존**(ChangeSet 유지 · 복귀 뒤 적용) | 토스트 · 변경 목록 띠 | 87 §14 데이터 보호 |
| 그리드 필터(로컬) · 복사 · 내보내기(파일) | × | 허용 | 허용 | — | 서버 승격(103)은 ○ → 판정 뒤 |
| Import 창 · `nsql import` · 대량 적재 | ○ | 판정 뒤 | 막기(시작 전) · 진행 중 끊김 = 배치 경계에서 중단 + 적재 행 수·되돌림 안내 | 창 안 띠 · CLI stderr | 89 |
| 조건 바 Enter(재실행) · 필터 서버 재조회 · 참조 행 보기 · 데이터 200행 | ○ | 판정 뒤 | 막기 → 상태줄 + 조건 바 빨간 테두리(기존 서버 오류 표시 재사용) | 조건 바 · 상태줄 | |
| 세션 창 · 트랜잭션 로그 창 · 접속 창 Test | ○ | 판정 뒤 | 끊김 줄 표시 · Test = 즉시 실패 사유 | 창 안 표 | 53 §8 신호등 |
| 탭 연결 바꾸기 · 작업 단위(DB/스키마) 전환 | ○ | 판정 뒤 | 막기 → 상태줄 · 메뉴 라벨 "(끊김)" | 메뉴 · 상태줄 | |
| 유휴 닫기 · 종료 · 탭 닫기 | ○(logoff) | — | `abandon_worker`(죽은 소켓에 logoff 안 보냄) | 로그 | 53 §8 그대로 |
| 자동 저장 · 프로젝트 · 파일 검색 · 북마크 · 편집 | × | 허용 | 허용 | — | 서버 무관 |
| CLI `nsql` 실행 | ○ | 판정 뒤 | 종료 코드 + stderr 한 줄(끝점 · 사유) | stderr | 같은 `live_plan` |

## 5. 안내 채널 설계(사용자 요구 ① "안내를 정확하게")

| 시점 | 채널 | 내용 | 규칙 |
|---|---|---|---|
| 끊김 **확정 1회**(Alive→Broken) | **토스트**(실행 카드 위 스택 · 경고색) + 로그 1줄(Error) | 제목 `서버 연결 끊김` · 본문 `끝점 · 사유(SYN 1.0 s 실패 / ORA-03113 …) · 영향(세션 n개 · 탐색기)` · 버튼 **[다시 연결]** [세션 보기] | 끝점당 1회 · 같은 끝점 재알림은 `net.notify_quiet_secs`(60) 뒤에만 |
| 끊긴 **동안 지속** | 탭 표식 끊김 · 툴바 플러그 빨강 · **탐색기 서버 헤더 플러그 빨강 + "끊김 hh:mm"** · 상태줄 `[끊김]` · 세션 창 줄 · Disconnect ▾ "· 끊김" | 상태만 | 전부 `health_changed` 한 자리에서 |
| 끊긴 동안 **동작마다** | 상태줄 한 줄 + 그 자리 표식(조건 바 빨간 테두리 · 노드 "끊김" · 버튼 흐림) · 토스트는 억제 창 밖일 때만 | `서버에 닿지 않음 — 192.168.0.58:1521 · [다시 연결]` | 막힌 동작은 서버에 아무것도 안 보냄 |
| **진행 중**(판정·질의) | 탐색기 헤더·필터 틀 **혜성**(`BusyRing` · 10-07 부품) · 실행 카드 | "소스 읽는 중…" | 600 ms 미만은 안 보임(기존 hold) · `explorer.timeout` 지나면 오류로 전환 |
| **복귀**(Broken→Alive) | 토스트(정보) + 로그 1줄 | `다시 연결됨 · 끝점 · 세션 상태 소실 여부` | 상태 있는 세션 = 52 §6-4 문구 |
| 전부 | i18n `Msg`(영어·한국어) · 끝점은 `host:port` 그대로 · 사유는 드라이버 원문 코드 + 정규화 글(42) | | |

## 6. 구현 단계(T-313 ~ · 작은 것부터 · 각 단계가 그 자체로 가치)

| 단계 | 내용 | 크기 | 시험 |
|---|---|---|---|
| **① 사건 직접 수리** | 메타 스레드 `MetaGuard`(`last_ok`·`suspect` · 카탈로그 요청마다 `live_plan` → `reachable`) · `Req::Source/Details/GenSql/Sizes/DbSizes/Live/Blockers`를 `loading_since`에 · 메타 세션 `set_call_timeout(explorer.timeout)` · 실패 = `ExplorerAction::Notify(토스트 · 로그)` + 상태줄 · 진행 중 헤더 혜성 | 소 | ✅ **10-08 구현**(174861e · bin97) · 보정 **①-b**(56fd9c5 · bin98) = ⓐ 배경 메타 스레드(`nsql-explorer-bg` · 사전·인덱스·선적재·용량)는 호출 상한 제외(Oracle 사전 14.6 s 실측이 15 s 상한에 근접) ⓑ 접속 전 판정 `reachable` 상한 = `probe.timeout_ms`(종전 고정 2 s) ⓒ 가드 SYN = ICMP 보조 없이 TCP 한 번 · **①-c**(9ebe923 · bin99) = 실행 세션 워커의 빠른 판정 셋(접속 전 · `ensure_alive` · 실행 전)도 TCP 한 번(신호등 프로브만 ICMP) · 협업 V1 = 닫힌 포트 2015→2012 ms · 응답 없는 IP 3528→2010 ms(= `probe.timeout_ms` 기본 **2000**) · 소스 열기 회귀 10~28 ms · `explorer.timeout=1`에서도 헛실패 0 · 탐색기 E2E 34/0 · 78/0 · conn-cmd 38/0 · **실 VPN 끊김 사용자 ✓ 10-08**(끊김 안내·탭 표식 → 재연결 뒤 명령 실행 = 재접속) · 남음 ②~⑥(사용자 확인 뒤) |
| **② ServerHealth** | `app/health.rs` 레지스트리 · `health_merge` 순수 함수(MC/DC) · 워커 `ConnOutcome::Broken/Alive`와 메타 `Resp::Health` 합류 · `Sess.broken` = 투영 · `health_changed` 한 자리 → `sync_sess_ui` + 탐색기 헤더 | 중 | ✅ **10-08 d39ff79**(bin101) · 보정 6e8a347(복귀 = 기록 제거 · 헤더 "끊김 hh:mm" = 이름 앞) · 시험 훅 `net.break/suspect/heal:<ep>` · `health.dump:<경로>`(레지스트리 · 세션별 broken · 칸 끊김 시각) · 같은 끊김 사건은 로그·토스트 1회(두 번째 break = 상태 변화 없음 = 사건 아님) · 협업 V1 bin101~102 ✓ |
| **③ 토스트 + 다시 연결** | 토스트 버튼 → 그 끝점 실행 세션 재접속(`ensure_alive` 길) + 메타 `Req::Open` 재개 · 억제 창 `net.notify_quiet_secs` | 소 | ✅ **10-08 d39ff79 · 6e8a347**(토스트 클릭 = `net.reconnect:<ep>` · 끊겼던 끝점은 진짜 재접속(`reconnect_same=true`) · 조용한 접속 완료 = broken 해제 + 끝점 Up) · 억제 창 `net.notify_quiet_secs` 60(D-266 · 실질 = 복귀 뒤 재끊김 토스트) · 클릭 = 놓을 때(af9a280) · 협업 V1 bin102 ② ✓ · bin103 Down만 = 무동작 · Up = 재접속 ✓ |
| **④ 기능별 게이트** | §4 표대로 `gate_open`/`gate_view`에 `health` 합류(막기·비활성·메뉴 라벨) · 워머 정지/재개 · 그리드 적용 보존 · Import 시작 전 막기 | 중 | ✅ **10-09 6e8a347 · beefed8** = 끊긴 끝점의 배경 워머 정지(컬럼 선적재 · 코멘트 · 유휴 인덱스 `index_step` · `broken_since`) · 복귀 때 재개 · Import 시작 전 막기(`StEpBrokenBlocked`) · **실행·페치는 워커 비동기 판정 유지**(게이트에서 동기 SYN = UI 정지라 하지 않음 · §4 표 "막기" 해석 참고) · 협업 V1 bin103 = 끊김 10 s 동안 메타 요청 0 · 복귀 뒤 재개 ✓ |
| **⑤ L0 OS 신호(T-130)** | nexa-sys `netwatch` 3-OS → `NetChanged` → Suspect | 중 | ✅ **10-09 6e8a347 · nexa-ui 5d128d6** = nexa-sys `netwatch`(**Windows만** · iphlpapi `NotifyIpInterfaceChange`+`NotifyRouteChange2` · 콜백 = 깃발 + 깨우기 · mac/Linux = None 후속) → `App::net_changed`(1초 합치기 · 세션 있는 끝점 전부 Suspect · 토스트 없음 · 로그 "네트워크 경로 변경 - 서버 N곳은 다음 동작 전에 다시 판정합니다" · 메타 `Req::NetChanged` · 다음 실행 preflight) · 성공 = 의심 해소(배경 메타 요청 성공으로도 바로 해소 = 의도) · 설정 `net.watch`(켬 · 다음 시작부터) · 기동 명령 `net.changed` · 협업 V1 bin102 ③ ✓ · ✅ **10-10 118차 mac = macOS SystemConfiguration(nexa-ui 195차 · `health.dump netwatch=true`) · Linux netlink 남음** |
| **⑥ 문서·위키** | 53 §9 갱신 · 28 · 52 §3 · 위키 Connections "끊겼을 때" 절 · 39 §3 부하원(netwatch · 상한 키) | 소 | ✅ 10-09 협업(53 §8 · 39 §3 · 24 · 61 §2-2-b · 30 §2 · 위키 Settings · journal) |
| **⑦ 재접속 원천(후속)** | 재접속 원천 = 러너 `Runner::connected_to()`(비밀번호 없는 접속 사양) + 세션 자격 금고 `recall` — 실행이 끝날 때마다 워커 `active_spec`을 이것으로 동기화(10-09 · 1e624fd · 사용자 결함 "전용 세션이 네트워크 복귀 뒤 재접속 안 됨 · 접속 회복 → 접속 끊김 → DPI-1010 반복" · 뿌리 = 편집기 `CONNECT` 문으로 붙은 전용 세션은 `active_spec`이 비어 `ensure_alive`가 다시 붙을 대상이 없었다) · 공유·전용(`CONNECT` 문) 같은 원천 · 사양이 없으면 "회복"이 아니라 끊김으로 보고 · Debug 훅 `sess.kill` | 소 | ✅ 10-09 1e624fd · ad2c765 · 협업 V1 bin126 = `win-reconnect-e2e.sh` 8/0 · 실서버 BISCM 전용 세션 kill → 재접속 → 1행 · recall(hit) 1 · conn-cmd 38/0 · 건강 신호 훅(`net.break`)은 접속을 죽이지 않아 이 경로에 닿지 않음(bin125) |

설정 키(모두 `REGISTRY` · 39 §3 등재): 기존 `probe.timeout_ms`(**2000** · 10-08 확인 = 레지스트리 기본) · `probe.stale_secs`(60) · `connect.auto_reconnect` · `explorer.timeout`(15) · 새 = `net.watch`(on · ⑤) · `net.notify_quiet_secs`(60 · ③) · `meta.call_timeout_secs`(= `explorer.timeout` 따름 · 0 = 끔).

## 7. 결정 대기(D-265 ~ · 권장안 표시)

| # | 결정 | 권장 |
|---|---|---|
| D-265 | 메타 세션 호출 상한 기본 | `explorer.timeout`과 같은 15 s(별도 키는 HIDDEN) |
| D-266 | 끊김 토스트 억제 창 | 끝점당 60 s(`net.notify_quiet_secs`) |
| D-267 | 끊긴 상태의 탐색기 캐시 열람 | 허용(트리·검색·완성은 캐시로 계속 · 서버가 필요한 것만 막기) |
| D-268 | L0 OS 신호 도입 시점 | ⑤로 미룸(①~④가 사건을 막는다 · T-130 유지) |
| D-269 | 커밋/롤백의 끊김 안내 | 재접속 없이 막고 "서버 쪽 롤백 가능성" 경고 + 트랜잭션 로그 Lost(53 D-109 유지) |

## 8. 사건의 교훈(61 §2에 올릴 후보)

- **서버에 가는 길은 하나의 통제를 지난다** — 실행 세션에만 있던 생존 관리가 메타 세션에는 없었다. 새 "서버로 가는 스레드"를 만들면 `ensure_alive`·호출 상한·접속성 오류 분류·건강 보고 네 가지를 같이 갖춘다(점검표 = 30 §1-2에 한 줄).
- **안내는 상태줄 한 줄로 끝내지 않는다** — 사용자가 "아무 메시지 없음"으로 받아들였다. 실패는 그 자리의 표식 + 토스트(또는 로그) 중 하나 이상에 남는다.

## 9. 전용(CONNECT) 세션 · 개별 모드에도 같은가(사용자 10-08 질문) — 두 층 모델로 확장

§3의 레지스트리는 **끝점(host:port) 단위**라 공유 연결에는 그대로 맞지만, 전용 세션(편집기 `CONNECT` · 탭 하나 = 자기 워커·접속 · 52 §4)과 개별 모드(`session.mode=per-editor` = 모든 탭이 전용)에는 아래 차이가 있다. 결론 = **개념은 같고, 건강을 "끝점"과 "세션" 두 층으로 나누면 로직 변경은 다섯 곳**이다.

### 9-1. 왜 한 층으로는 부족한가

| 상황 | 끝점 층(네트워크) | 세션 층(그 접속 하나) | 예 |
|---|---|---|---|
| VPN 끊김 · 라우트 변경 · 서버 다운 | **Broken** — 그 끝점의 모든 세션(공유 N · 전용 M · 메타) | 전부 Broken(투영) | 이번 사건 |
| 서버가 **세션 하나만** 끊음(DBA kill · `IDLE_TIME` 프로파일 · 방화벽 유휴 끊김이 그 소켓만) | Alive(SYN은 된다) | 그 세션만 Broken | 전용 탭 하나가 밤새 유휴 · 공유 세션은 멀쩡 |
| 전용 세션의 자격 만료·계정 잠금(재접속 실패) | Alive | 그 세션 Broken(재접속 불가 사유) | 비밀번호 변경 |
| 전용 세션 유휴 닫기(`session.idle_secs` · 전용만 · 52 §6) | — | **끊김 아님** = `idle_closed`(다음 실행 때 조용히 재개) | 정상 동작 — 끊김 표식·토스트를 내면 안 된다 |

끝점 층 하나만 두면 둘째·셋째 줄에서 멀쩡한 공유 세션까지 "끊김"으로 물들거나, 반대로 세션 하나의 죽음을 끝점이 Alive라는 이유로 묻는다.

### 9-2. 판정 규칙(`health_merge` 확장 · 순수 · MC/DC)

- 어떤 세션(실행·메타)이 접속성 오류/호출 상한을 만나면 **그 세션 Suspect** → `ensure_alive`의 SYN 1회가 가른다: **SYN 실패 = 끝점 Broken**(전파 전부) · **SYN 성공 = 그 세션만 Broken**(세션 층 · 끝점은 Alive 유지 · 다른 세션 안 건드림).
- L0 OS 신호 = 끝점 전부 Suspect(세션 층은 그대로). 복귀 `Connected(ep, sess)` = 그 세션 Alive + 끝점도 Alive(한 세션이 붙었으면 네트워크는 산 것).
- `idle_closed`는 건강 사건이 아니다(레지스트리에 들어오지 않는다). 재개 실패 때 비로소 세션 층 사건.
- 투영: `Sess.broken = endpoint.broken || session.broken`. 탭 표식·플러그는 투영을 본다(지금 코드와 호환).

### 9-3. 전파 범위(공유 ↔ 전용 차이)

| 대상 | 끝점 Broken | 세션 Broken(전용 하나) |
|---|---|---|
| 탭 표식 | 그 끝점에 묶인 **모든 탭**(공유 탭 + 전용 탭 각각) | **그 전용 탭 하나** |
| 툴바 플러그·상태줄 `[끊김]` | 활성 탭이 그 끝점이면 | 활성 탭이 그 세션이면 |
| 탐색기 서버 헤더 | 그 서버 칸(전용도 칸을 공유한다 — explorers.rs "공유·전용·개별 어느 세션이든 칸 유지 · 참조 수") | **바꾸지 않음**(메타 세션은 멀쩡) · 헤더 툴팁에 "전용 세션 1 끊김" 한 줄만 |
| 세션 창 · Disconnect ▾ | 그 끝점 줄 전부 "· 끊김" | 그 세션 줄만 |
| 토스트 | 끝점당 1회 · 본문에 **영향 목록**("공유 1 · 전용 탭 2(Script_3 · Script_5) · 탐색기") | 세션당 1회 · 본문 "탭 Script_3의 전용 세션(user@host)" |
| 게이트(§4 표) | 그 끝점의 모든 세션 동작 | 그 세션의 동작만(같은 서버 공유 탭은 정상) |

### 9-4. 복구(L4)의 차이

| | 공유 세션 | 전용 세션 |
|---|---|---|
| 자동 재접속 조건 | `connect.auto_reconnect` + SYN 성공 | 같음 + **자격이 있을 때만**: `CONNECT user/pw@…`는 스펙에 비밀번호가 있음 · 프로필 이름 `CONNECT 프로필`은 금고에서 · **일회성 비밀번호**(21 §7) 세션은 재접속 불가 → 토스트 [다시 연결]이 **비밀번호 입력 창**(기존 모달)으로 |
| 상태 소실 안내 | 52 §6-4(`alters_session_state`면) | 전용은 거의 늘 상태가 있다(CONNECT 자체 · `ALTER SESSION` · 변수 `VAR`은 클라이언트라 안전) → 복귀 때 **"세션 상태가 초기화됐습니다(현재 스키마·NLS·임시 객체)"**를 전용 탭에는 기본 표시 · 열린 트랜잭션 = Lost(트랜잭션 로그) |
| 메타 세션 재개 | 끝점 Alive 복귀 때 `Req::Open` | 같음(칸은 끝점 단위) |
| 개별 모드(`per-editor`) | — | 모든 탭이 전용 → 끝점 Broken 토스트의 영향 목록이 길어진다 → "전용 탭 N개" 요약 + [세션 보기] |

### 9-5. 수명(레지스트리 항목의 생성·소멸)

- 끝점 항목 = 그 끝점에 **세션(공유·전용·메타) ≥ 1**인 동안만(탐색기 칸의 참조 수와 같은 규칙 · `ExplorerSet::sync_users`). 마지막 세션이 닫히면(전용 탭 닫기 · `DISCONNECT` · 모두 해제) 항목 제거 — 나중에 같은 서버에 다시 붙을 때 옛 Broken이 남아 있지 않게.
- 세션 항목 = `Sess`에 그대로(`broken` 필드 = 세션 층 · 끝점 층은 레지스트리 조회).

### 9-6. 로직 변경 다섯 곳(§6 단계에 합류)

| # | 어디 | 변경 | 단계 |
|---|---|---|---|
| ① | `app/health.rs` | 키 = 끝점 + 세션 두 층 · `health_merge(ep_prev, sess_prev, event)` · SYN 결과로 층을 가르는 규칙(9-2) · 수명 = 참조 수 | ② |
| ② | `worker.rs ensure_alive` | 접속성 오류 뒤 SYN 성공이면 `ConnOutcome::SessionBroken(사유)`(새 변형 · 끝점은 건드리지 않음) · SYN 실패 = 기존 `Broken` → 끝점 | ② |
| ③ | `events.rs` Broken 수신 | `self.sess`(활성)만 바꾸던 것을 **끝점의 모든 세션**에 투영(`all_sess_mut()` 끝점 일치) · 세션 층은 그 세션만 | ② |
| ④ | 토스트·세션 창 | 영향 목록(공유/전용 탭 이름) · 전용 세션 토스트 문구 · [다시 연결] = 자격 없으면 비밀번호 창 | ③ |
| ⑤ | 탐색기 헤더 | 세션 층 Broken은 헤더 색을 바꾸지 않고 툴팁 한 줄 | ② |

개별 모드 전용의 추가 코드는 없다 — 전용 세션 규칙이 모든 탭에 적용될 뿐이다. 결정 추가 = **D-270** 세션 층 Broken의 전용 탭 복귀 안내 기본 표시(권장 = 표시) · **D-271** 일회성 비밀번호 세션의 [다시 연결] = 비밀번호 창(권장 = 그렇게).
