# 53. 접속 생존 관리 — 장기 방치 · 네트워크 단절(VPN) 때의 상태 관리

> **선행**: [52 세션 모드](52-session-modes.md)(DR-34 · `Sess` · 유휴 닫기 §6) · [26 §8 네트워크 부하](26-performance-architecture.md) · [44 §6 실행 취소](44-transaction-log.md) · `probe.rs`(신호등) · `worker.rs`(빠른 판정 · 자동 재접속)
> **상태**: ✅ 1차 구현 2026-09-18(59차 · mac · 사용자 *"제안 방식으로 끊어진 세션 관리 기능 구현"* = D-109~114 권장안 확정) — §8 구현 기록. 잔여 = T-130(OS 네트워크 변경 신호).

## 0. 지금 앱의 실제 상태 (09-18 실측 · VPN 끊긴 채)

사용자 캡처: Disconnect ▾ 목록에 `oracle://BISCM@61.81.244.214:1521/BISCM`이 **접속됨**으로 남아 있다. 실행 중인 앱(PID 40393)을 OS 쪽에서 봤다.

| 확인 | 결과 |
|---|---|
| 앱의 소켓 | `192.168.211.62:50502 → 61.81.244.214:1521 ESTABLISHED` **1개**(VPN 주소로 맺은 세션) |
| 경로 | 기본 경로는 `en0`(192.168.45.1) · utun0~2는 UP이지만 61.81.x는 더 이상 그쪽으로 안 감 |
| 도달성 | `nc 61.81.244.214 1521` = 실패(SYN 무응답 · 75초 뒤 포기) |
| OS TCP keepalive | `always_keepalive=0` · `keepidle` 7,200,000ms(2시간) · 드라이버가 SO_KEEPALIVE를 켜지 않는 한 **영원히 ESTABLISHED** |
| 앱의 인식 | `Sess.connected = true` · 신호등은 마지막 확인(초록) 그대로(창이 닫혀 있으면 주기 확인 없음) · `suspect = false` |

이 상태에서 무슨 일이 일어나는가(코드 검토):

1. **실행(F5)**: `preflight`는 신호등이 초록이 아닐 때만 → 초록이라 **빠른 판정을 건너뛰고** 죽은 소켓에 그대로 보낸다. Oracle 클라이언트는 응답을 기다리며 **TCP 재전송이 끝날 때까지(macOS 수 분)** 막힌다. 그 뒤 `ORA-03113/03135`가 오면 `is_connection_error` → `suspect = true` · 다음 실행부터 빠른 판정 + 자동 재접속(`connect.auto_reconnect`).
2. **중지(■)**: Oracle 취소 = `OCIBreak` — **같은 소켓**으로 보내므로 함께 막힌다(44 §6). 빠져나오는 길은 **접속 해제**(워커 버림 · 09-16 즉시 해제 규약)뿐.
3. **유휴 닫기**(52 §6): 전용 세션이 유휴가 되면 `Disconnect`(커밋 시도 → 막힘 가능) → 워커가 갇힌다. 공유 세션은 대상이 아니다.
4. **탐색기 메타 세션**: 소켓이 하나뿐인 것으로 보아 이미 유휴 회수됐거나 끊겼다(§2-2 재개는 다음 펼침 때 → 그때 접속 시도가 타임아웃까지 막힌다 · 메타 스레드는 별개라 UI는 안 멈춤).
5. **드라이버 타임아웃**: Oracle = 없음(`call_timeout` 미설정) · PG = `connect_timeout` 15초만(keepalive·`tcp_user_timeout` 미설정) · MSSQL = 접속 소켓에 `set_nodelay`만(read timeout 없음 · PRELOGIN 탐침에만 timeout) · SQLite 해당 없음.

→ **정리**: "접속됨"은 *마지막으로 성공했던 사실*일 뿐이고, 끊김은 **다음 실행이 몇 분 막힌 뒤에야** 드러난다. 다른 도구도 근본적으로 같은 한계(TCP는 트래픽 없이는 단절을 모른다)를 갖지만, **막히는 시간을 짧게** 하고 **상태를 화면에 정직하게** 보이는 장치가 있다.

## 1. 다른 도구는 어떻게 하는가 (조사)

| 도구 | 끊김 감지 | 사용자에게 | 복구 | 유휴/keepalive |
|---|---|---|---|---|
| **DBeaver**(소스 검토 `InvalidateJob` · `JDBCExecutionContext`) | 실행 오류가 "접속 오류"로 분류되면 **Invalidate** 작업: ① 네트워크 핸들러(SSH 터널 등) 재수립 ② 각 실행 컨텍스트를 `BEFORE_INVALIDATE`(닫기) → `INVALIDATE`(**같은 설정으로 재접속** — 실패하면 컨텍스트는 남기고 연결만 null · 다음 Invalidate 때 재시도) → `AFTER_INVALIDATE` · 전부 실패 + `disconnectOnFailure`면 **강제 해제 + 대화상자** "destination database unreachable" | 오류 대화상자 + 재접속 진행 · 수동 커밋이면 열린 트랜잭션을 `invalidateTransaction`으로 **롤백 시도**(로그 억제 · smart commit 오작동 방지 #9066) | 자동(오류 뒤) · 수동 메뉴 **Invalidate/Reconnect** | 접속 유형별 **Keep-Alive 간격**(초 · 신호 전송) · **Close idle connection after**(초 · 기본 켬) — 둘 다 접속별 재정의 가능([wiki](https://github.com/dbeaver/dbeaver/wiki/Configure-Connection-Initialization-Settings)) |
| **DataGrip** | 실행 오류 | 콘솔 상단에 "connection closed" + **Reconnect** 링크 · 재시도 | 수동(링크) · 다음 실행 때 자동 재접속 옵션 | **Run keep-alive query each N s**(드라이버별 질의 · Oracle `SELECT 1 FROM DUAL`) · **Auto-disconnect after N s**([문서](https://www.jetbrains.com/help/datagrip/configuring-database-connections.html)) |
| **SSMS / SqlClient** | 실행 시 "A transport-level error … (provider: TCP Provider, error: 0)" | 오류 창 · 상태줄 "Disconnected" | 다음 F5 = SqlClient 풀이 죽은 연결을 버리고 **자동 재접속**(세션 상태 소실 · `#temp` 사라짐) | keepalive = 드라이버 SO_KEEPALIVE(30초) · 서버 `remote query timeout` |
| **SQL Developer** | 실행 시 `ORA-03113`/`IO Error: The Network Adapter…` | "Connection … lost. Reconnect?" 대화상자 | 사용자 확인 뒤 재접속 · 워크시트 유지 | 확장 "Keep Alive"(N분마다 `SELECT 1 FROM DUAL`) · 접속 설정 `Connection Timeout` |
| **psql**(libpq) | 다음 명령의 오류 `server closed the connection unexpectedly` | 한 줄 + `Attempting reset: Succeeded.` | **자동 reset**(`PQreset`) · 그 명령은 실패, **다음 명령부터** 정상 | libpq `keepalives_*` · `tcp_user_timeout` 접속 파라미터 |
| **SQL*Plus** | `ORA-03113 end-of-file` → 이후 `ORA-03114 not connected` | 오류 줄 | **없음** — 사용자가 `CONNECT` 다시 | `sqlnet.ora` `SQLNET.EXPIRE_TIME`(서버 → 클라이언트 탐침 · 죽은 클라이언트 정리) · `SQLNET.RECV_TIMEOUT` |
| HeidiSQL · TablePlus · Toad(참고) | 실행 오류 | "Lost connection … Reconnect?" 프롬프트(Heidi) · 조용히 재접속(TablePlus) | 프롬프트/자동 | Heidi "keep-alive query every N s" |

공통점 세 가지:
1. **감지는 "다음 동작" 때**다. 유휴 중에 끊김을 알아채는 도구는 keepalive를 **켠** 경우뿐이고, 그것도 간격만큼 늦다.
2. **재접속은 사용자 동작에 묶여 있다**(오류 뒤 자동 또는 프롬프트). 백그라운드에서 스스로 붙는 도구는 없다(우리 26 §8 "자동 재접속 금지"와 같은 판단).
3. **세션 상태 소실은 알린다**(SSMS·SQL Developer 대화상자 · DBeaver 롤백 시도). 우리는 52 §6-4의 `stateful` 안내가 이미 있다.

차이는 **막히는 시간**이다 — 드라이버에 **호출 타임아웃/keepalive**를 건 도구(SqlClient 30초 · libpq 옵션 · DBeaver keep-alive)는 수십 초, 안 건 도구(SQL*Plus 기본 · 우리 Oracle)는 OS 재전송 한도까지(분 단위).

## 2. 사용할 수 있는 드라이버 API (검토)

| 드라이버 | 있는 것 | 비용 |
|---|---|---|
| **Oracle**(`oracle` 0.6 / ODPI-C) | `Connection::set_call_timeout(Duration)` — **모든 OCI 호출에 상한**(초과 = `ORA-03136`/DPI-1067 · 세션은 살아 있음) · `status()` — **왕복 없이** 마지막 네트워크 결과·FIN/RST 인지(`ConnStatus::NotConnected`) · `ping()` — 1 왕복 | `status()` 0 · `ping()` 1 RTT · `call_timeout`은 실행마다 무료 |
| **PostgreSQL**(`postgres` 0.19) | `Config::keepalives(true)` + `keepalives_idle/interval/retries` · `tcp_user_timeout`(Linux) · `Client::is_closed()`(0 왕복 · 소켓 EOF 인지) · `is_valid(timeout)` = 빈 질의 1 왕복 | keepalive = OS가 보내는 빈 패킷(부하 0에 가까움 · 서버 유휴 정책 무력화 **안 함** — 서버의 `idle_session_timeout`은 SQL 활동 기준) |
| **SQL Server**(자체 TDS) | 우리가 소켓을 쥔다 → `SO_KEEPALIVE`(std에는 없음 · `socket2` 또는 OS 호출) · `set_read_timeout` · 이미 있는 취소 = Attention/소켓 종료 | 같음 |
| SQLite | 해당 없음 | — |

**TCP keepalive vs 질의 keepalive**: TCP keepalive는 데이터가 없는 빈 세그먼트라 DBMS의 유휴 세션 정책(`IDLE_TIME`·`idle_session_timeout`·`wait_timeout`)을 깨우지 **않으면서** 죽은 경로를 OS가 알아채게 한다(끊기면 소켓이 오류 상태가 되어 `status()`/`is_closed()`가 즉시 안다). 질의 keepalive(`SELECT 1`)는 서버에 세션 활동으로 잡혀 **DBA 정책을 무력화**한다 — 26 §8 취지에 어긋나므로 **채택하지 않는다**(DataGrip·DBeaver가 기본 제공하지만 우리는 "가볍게"가 원칙).

## 3. 제안 — 상태 모델

```
            접속 성공                          실행/펼침/유휴 닫기 직전 빠른 판정 실패
 Disconnected ────────► Connected ───────────────────────────────────────────► Broken
      ▲                    │  ▲                                                  │
      │   해제(사용자)      │  │ 재접속 성공(사용자 동작 1회당 1회 · 설정)           │ 재접속 실패
      └────────────────────┘  └──────────────────────────────────────────────────┘
                              (Idle-closed: 52 §6 — 스펙만 남기고 닫음 · 다음 동작 때 재접속)
```

| 상태 | 뜻 | 화면 |
|---|---|---|
| Connected | 마지막 동작이 성공 | 지금과 같음 |
| **Broken**(신설) | 서버에 닿지 않는 것을 **확인**했다(빠른 판정 실패 · 드라이버 접속 오류 · `status()` NotConnected) · 세션 객체·스펙은 유지 | 툴바 플러그 = 빨강 · Disconnect ▾ 줄에 "· 끊김" · 탭 표식 끊김 · 탐색기 루트 흐림 + "끊김" · 상태줄 |
| Idle-closed | 우리가 닫음(52 §6) | 표식 끊김(지금) |
| Disconnected | 사용자가 해제 | 지금과 같음 |

**감지 4경로**(전부 사용자 동작 시점 또는 무료):
1. **빠른 판정을 늘 건다**: 지금은 신호등이 초록이 아닐 때만 → **마지막 성공 뒤 `probe.stale_secs`(기본 60초) 이상 지났으면** 실행·페치·펼침 전에 `probe_once`(TCP SYN 1 · `probe.timeout` 2초). 성공하면 시각 갱신. 비용 = 오랜만의 첫 동작에 SYN 1개.
2. **드라이버 0-왕복 상태**: Oracle `status()` · PG `is_closed()` — 실행 직전에 공짜로 본다. TCP keepalive를 켜 두면 끊긴 소켓이 여기서 잡힌다.
3. **호출 타임아웃**: Oracle `set_call_timeout(session.call_timeout_secs)`(기본 0 = 끔 · 켜면 죽은 소켓이 N초 안에 `ORA-03136`) — 긴 질의와 구분이 안 되므로 기본은 끄고 **빠른 판정이 주 수단** · MSSQL `set_read_timeout`도 같은 키.
4. **오류 분류**: 지금의 `is_connection_error` → Broken.

**복구**: Broken 세션에 실행이 오면 ① 빠른 판정 다시(SYN 1) ② 살아 있으면 같은 스펙으로 재접속(`connect.auto_reconnect` · 지금도 있음) + `stateful`이면 "세션 상태 소실" 안내 + 열린 트랜잭션 `Lost` ③ 죽어 있으면 즉시 오류(막힘 0). 백그라운드 재시도 없음. 유휴 닫기·메타 재개도 같은 빠른 판정을 먼저 건다(죽은 서버에 `Disconnect`/`Open`으로 갇히지 않게 — 갇히면 워커/메타 스레드 교체 = 09-16 즉시 해제 규약).

**중지(■)가 막힐 때**: 빠른 판정이 실패한 서버의 실행 취소는 `OCIBreak`를 보내지 않고 **워커 교체**(끊긴 것이 확실하므로) — 44 §6의 `drops_session` 경로 재사용.

## 4. 우리 소스에서 고칠 곳 (검토 결과)

| # | 위치 | 지금 | 처방 |
|---|---|---|---|
| 1 | `worker.rs` `Cmd::Run.preflight` | 신호등 ≠ 초록 또는 `suspect`일 때만 | **오래된 성공**(§3-1)도 조건에 · `FetchPage`/`Keys`/`Count`에도 같은 판정(지금은 Run만) |
| 2 | `probe.rs` / `conn_win` | 주기 확인은 접속 창이 열려 있을 때만(의도 · 26 §8) | 그대로 — 대신 §3-1의 "동작 직전 판정"이 창 없이도 동작 |
| 3 | `nsql-driver-oracle` | `call_timeout` 없음 · `status()` 미사용 | `Session::is_alive()`(0 왕복 · 기본 구현 = true) 포트 추가 → Oracle `status()` · PG `is_closed()` · 실행 직전 호출 · 설정 `session.call_timeout_secs`(Oracle·MSSQL) |
| 4 | `nsql-driver-pg` | keepalive 미설정 | `keepalives(true)` + `keepalives_idle`(설정 `net.keepalive_secs` 기본 60) — OS 빈 패킷 · 서버 유휴 정책 무관 |
| 5 | `nsql-driver-mssql` | 소켓에 keepalive 없음 | `SO_KEEPALIVE` + idle(`socket2` 의존 추가는 DR-3 원장 등재 필요 → 또는 `libc::setsockopt` 직접) |
| 6 | `sessions.rs` `Sess` | `connected: bool` | `broken: bool`(+ `last_ok: Instant`) · `badge_kind`에 Broken → LinkOff · `gate_view` 무관 |
| 7 | `main.rs` Disconnect ▾ · 탭 표식 · 탐색기 루트 · 툴바 플러그 색 | 접속됨/아님 2값 | Broken 표시(빨강 · "끊김") · 줄 클릭 = 해제 그대로 |
| 8 | 유휴 닫기 `idle_tick` · 메타 `Suspend`/재개 | 판정 없이 `Disconnect`/`Open` | 빠른 판정 실패면 닫기 = **워커 교체**(갇힘 방지) · 재개는 판정 뒤 |
| 9 | `stop_run` | Oracle = OCIBreak | Broken이면 워커 교체 경로 |
| 10 | `nexa-sys`(D-60 OS 신호) | 배터리·원격 세션 | **네트워크 경로 변경 신호**(macOS `SCNetworkReachability`/Windows `NotifyRouteChange2`/Linux netlink) → 모든 Connected 세션의 `last_ok`를 0으로(다음 동작 때 반드시 판정) — 실제 소켓을 건드리지 않는다 · 선택(D-113) |

## 5. 부하 원칙과의 관계 (26 §8)

- 새 트래픽 = **사용자 동작 직전 SYN 1개**(오래된 성공일 때만) + TCP keepalive 빈 패킷(60초 · 접속당) — 접속한 적 없는 서버에는 0 · 주기 질의 0 · 자동 재시도 0.
- keepalive는 "지속 트래픽"이지만 ① 데이터가 없고 ② 서버 유휴 정책을 깨우지 않으며 ③ 끊긴 소켓을 OS가 정리해 **좀비 세션(서버 쪽 프로세스 점유)** 을 오히려 줄인다 — 원장 표에 등재하고 설정 하나로 끌 수 있게(`net.keepalive_secs = 0`).

## 6. 결정 (사용자 09-18 "제안 방식으로 구현" → 권장안 확정)

| # | 질문 | 선택지 | 권장 |
|---|---|---|---|
| **D-109** | 동작 직전 빠른 판정의 기준 | ⓐ 마지막 성공 뒤 `probe.stale_secs`(60초) 이상이면(권장) · ⓑ 항상(SYN 1 · 2초 상한) · ⓒ 지금대로(신호등·suspect만) | ⓐ |
| **D-110** | TCP keepalive | ⓐ 켬(60초 · PG·MSSQL · Oracle은 `sqlnet` `ENABLE=BROKEN` 접속 문자열/`DISABLE_OOB` 검토) · ⓑ 끔 | ⓐ(빈 패킷 · DBA 정책 무관) |
| **D-111** | Oracle 호출 타임아웃 기본 | ⓐ 끔(빠른 판정이 주 수단 · 긴 질의 오탐 없음) · ⓑ 60초 | ⓐ · 설정 키는 둔다 |
| **D-112** | Broken 뒤 재접속 | ⓐ 다음 실행 때 자동(지금 `connect.auto_reconnect` on) + 세션 상태 소실 안내 · ⓑ 프롬프트(SQL Developer식) | ⓐ(수동 커밋 대기가 있으면 ⓑ처럼 묻는다 — 잃는 순간 규칙 DR-30) |
| **D-113** | OS 네트워크 변경 신호(`nexa-sys`) | ⓐ 넣음(VPN 끊김 즉시 "재판정 필요" 표시) · ⓑ 안 넣음 | ⓐ — 소켓은 안 건드리고 다음 동작의 판정만 강제 |
| **D-114** | Broken 세션의 표시 | ⓐ 끊김 표식·빨강 플러그·탐색기 루트 흐림(권장) · ⓑ 즉시 해제로 정리 | ⓐ — 사용자가 VPN을 다시 켜면 그 자리에서 재접속 |

## 8. 구현 (09-18 · 59차)

| 항목 | 구현 |
|---|---|
| 동작 직전 판정(D-109) | 워커 `ensure_alive` — 순수 판정 `sessions::live_plan(preflight, suspect, dead_hint, stale, allow, auto)`(MC/DC D15) → 판정 = TCP SYN 1(`probe.timeout`) · **Run · FetchPage · Count · Keys · Commit/Rollback** 전부 · 커밋/롤백은 재접속 안 함 · `probe.stale_secs`(60 · 0 = 신호등 조건만) · 마지막 성공 시각 `last_ok`는 명령이 성공할 때마다 |
| 접속 자체도 판정 먼저 | 접속 창 Connect · 실행 인자 · 유휴 뒤 재접속 · 탐색기 메타 Open/재개 — 모두 SYN 1 뒤 드라이버 접속(끊긴 네트워크에서 드라이버 타임아웃 대기 0) · 테스트 `unreachable_server_fails_fast_and_reports_broken`(TEST-NET-1 · 4초) |
| Broken 상태(D-114) | `ConnOutcome::Broken/Alive`(워커 → UI · 1회) · `Sess.broken` · 접속성 오류(`is_connection_error`)도 Broken · 표시 = 탭 표식 끊김 · 툴바 플러그 **빨강**(`ToolTone::Danger`) · Disconnect ▾ 줄 "· 끊김" · 상태줄 `[끊김]` · 로그 1줄 · 살아나면 "접속 회복" 1줄 |
| 재접속(D-112) | 판정이 살아 있고(`suspect`/`dead_hint`) `connect.auto_reconnect`면 같은 스펙으로 · 성공 = Connected → `broken=false` · `stateful`이면 세션 상태 소실 안내(52 §6-4) · 열린 트랜잭션 = Lost |
| 드라이버 0-왕복 상태(T-128) | `Session::is_alive()` 포트(기본 true) — Oracle `status()`(`OCI_SERVER_NOT_CONNECTED`) · PG `Client::is_closed()` · 판정의 `dead_hint` |
| TCP keepalive(D-110 · T-129) | PG `keepalives(true)` + idle/interval/retries · SQL Server `socket2::SockRef::set_tcp_keepalive`(어댑터 안 · tokio가 이미 끌어오는 크레이트) · `net.keepalive_secs`(60 · 0 = 끔 · 새 접속부터) · Oracle은 EZConnect 문자열에 keepalive 파라미터를 넣을 수 없어 **`status()` + 판정**으로 대신 |
| Oracle 호출 상한(D-111) | `session.call_timeout_secs`(기본 0 = 끔) → `Connection::set_call_timeout` |
| 끊긴 서버에서 중지 ■ | `sess.broken`이면 `abandon_worker`(워커 교체 · 세션은 Broken+`idle_closed`로 남겨 다음 실행 때 재접속) — OCIBreak를 죽은 소켓에 보내지 않는다 |
| 유휴 닫기 | `idle_tick`도 `abandon_worker`(닫기 = commit + logoff가 죽은 소켓에 갇혀도 세션 큐가 막히지 않는다) |
| 남은 것 | T-130 OS 네트워크 변경 신호(D-113) — 지금은 `probe.stale_secs`(60초)가 그 자리를 대신한다 · MSSQL read timeout(취소 = 소켓 종료가 이미 있어 급하지 않음) · `Suspend`로 잠든 메타 세션의 logoff가 갇히면 그 메타 스레드는 다음 요청까지 대기(재개 전 판정은 하지만 갇힌 스레드 자체는 교체하지 않음) |

## 7. 할 일

| # | 내용 | 선행 |
|---|---|---|
| **T-127** | ✅ 09-18 §8 | — |
| **T-128** | ✅ 09-18 §8(MSSQL read timeout은 보류) | — |
| **T-129** | ✅ 09-18 §8(Oracle은 `status()`로 대신) | — |
| **T-130** | `nexa-sys` 네트워크 경로 변경 신호 → `last_ok` 초기화 | D-113 |
