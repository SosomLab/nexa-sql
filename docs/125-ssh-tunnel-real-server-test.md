# 125. 실서버 SSH 터널 시험 — 로컬 ssh 설정으로 Oracle에 붙기

> 요구(사용자 10-28 · 161차 win): "로컬에서 설정된 ssh를 통해 Oracle 서버에 접속되는 테스트 케이스" + "Cloudflare나 VPN 접속 정보로도 가능한지". 기능 자체 = T-3 SSH 터널([21 §10](21-connection-profiles.md) · D-388) · 이 문서는 **실서버 시험 케이스**와 그 전제(이 PC의 네트워크·ssh 상태) 원장이다. 비밀번호는 여기 적지 않는다(볼트 프로필 또는 셸 환경변수).
>
> ★ **대상(사용자 10-28 확정)** = 점프 호스트 **`jumpuser@172.16.0.108`** → DB **172.16.0.110:1521** · 서비스 **APPDBDEV** · 계정 APPDB · "DB에 접속할 수 있도록 구성". 처음 거론된 172.16.1.58(APPDB)은 §1-1 조사 기록으로만 남긴다. Cloudflare(B)·VPN(C)은 **사용자 제외**.

## 0. 지금 상태 한 줄

> 주소·호스트는 172.16/16으로 익명화(10-28 · 사용자 지시) · 대응은 비공개.
> 문서의 계정·서비스·프로필 이름(APPDB · APPDBDEV · appdb-ssh 등)도 익명화(10-28 2차) · 실제 볼트 이름과 다르다.

★ **통과(10-28 밤)** — 사용자가 jumpuser에 키 `~/.ssh/id_ed25519`를 구성한 뒤 `scripts/win-ssh-tunnel-test.ps1 -Profile appdb-ssh` 3단계 전부 ✓: ① ssh 선검사 로그인 OK ② `ssh_tunnel_real_server` 2라운드 ✓(설명 " via ssh jumpuser@172.16.0.108" · `USER=APPDB` · `SERVER_HOST=db-dev-01` · 접속+조회 0.66s / 0.49s) ③ `nsql run` 왕복 `VIA_SSH=1` ✓. 프로필 `appdb-ssh`(`…?ssh=jumpuser@172.16.0.108&ssh_key=%USERPROFILE%/.ssh/id_ed25519&driver=thin`)는 격리 홈과 **실제 볼트**(사용자 승인 · `%APPDATA%\nexa-sql\profiles\ca3d2fab6ddb510c.conf`) 둘 다에 있다 → GUI 접속 창에서 `appdb-ssh`를 불러오면 SSH 터널 칸이 자동 펼침되고 [접속]이 같은 터널을 지난다(사용자 실기 몫).

막혔던 지점(기록) = jumpuser 인증 — 172.16.0.108은 22 도달·호스트 키 등록 ✓이나 이 PC 기본 키를 거절(허용 = publickey·gssapi·password) · 앱 터널은 `BatchMode`라 jumpuser용 키가 필요했다 → 사용자가 키 등록(ⓐ) · 스크립트 선검사가 프로필의 `ssh_key`를 `-i`로 넘기지 않던 결함도 고침.

## 1. 이 PC의 상태(10-28 조사 · 읽기 전용)

### 1-1. 공통 · 처음 대상(172.16.1.58)

| 항목 | 결과 | 뜻 |
|---|---|---|
| 로컬 IP | 이더넷 3(USB) 172.16.2.137 · 이더넷 2 = Juniper Networks Virtual Adapter 172.16.3.93(Pulse Secure VPN · 연결됨) · Wi-Fi 끊김 | — |
| 172.16.1.58 | 22·1521 TCP 도달 ✓ · sshd 인증 = `publickey,gssapi-keyex,gssapi-with-mic,password` · 이 PC 기본 키 = Permission denied · 경로 = 172.16.2.1(사무실 라우팅 · VPN 아님) · known_hosts = 조사 중 `accept-new`로 등록됨 | 참고용(사용자가 대상을 172.16.0.x로 바꿈) |
| `~/.ssh/config` 별칭 | github/bitbucket · `git.internal`(172.16.1.60 · 22 미도달) · `ssh1.example.com`(ProxyCommand `cloudflared access ssh`) | B 후보였으나 제외 |
| cloudflared | 설치됨(2026.10.3) · 돌고 있는 것은 `tunnel run`(이 PC를 원점으로 내보내는 데몬) · `~/.cloudflared` 없음(Access 로그인 토큰 없음) | B 제외 |
| 로컬 sshd · docker | 둘 다 없음 | CI처럼 sshd 컨테이너를 띄우는 길은 이 PC에 없음 |
| 앱 쪽 전제 | 터널은 **시스템 `ssh`**를 그대로 쓴다 → `~/.ssh/config`(별칭·IdentityFile·ProxyCommand)·에이전트가 그대로 먹는다 · `BatchMode=yes` = 비밀번호·호스트 키 프롬프트 없음 | 로컬 ssh 설정이 곧 앱 설정 |

### 1-2. 확정 대상(172.16.0.108 → 172.16.0.110)

| 항목 | 결과 | 뜻 |
|---|---|---|
| 172.16.0.108:**22** | TCP 도달 ✓ · 배너("<회사> 자산 … 인가되지 않은 사용자 접근 시 처벌") · 인증 = `publickey,gssapi-keyex,gssapi-with-mic,password` · 호스트 키 = 스크립트 `-AddKnownHost`로 등록 ✓ | 점프 호스트 OK · **키 또는 비밀번호** |
| `jumpuser@172.16.0.108` 로그인 | 이 PC 기본 키(`Host *` IdentityFile) = Permission denied → **`-i ~/.ssh/id_ed25519` = OK**(사용자 구성 · 10-28 밤) | 프로필 `ssh_key=`로 지정 |
| 172.16.0.110:**1521** | 이 PC에서 **직접 도달 ✓**(22도 열림) | 터널 없이도 붙는다 → 시험은 "터널을 지나는가"를 설명 꼬리 " via ssh …"·`SYS_CONTEXT('USERENV','SERVER_HOST')`로 판정 |
| 프로필 `appdb-ssh` | `oracle://APPDB:…@172.16.0.110:1521/APPDBDEV?ssh=jumpuser@172.16.0.108&ssh_key=%USERPROFILE%/.ssh/id_ed25519&driver=thin` · `conn show` = "… via ssh jumpuser@172.16.0.108" ✓ · 격리 홈 `%TEMP%\claude\nsql-ssh-test` + **실제 볼트**(사용자 승인 10-28) | 비밀번호 = 볼트 봉투(DPAPI) · 키 = 파일 경로만 |

## 2. 경로 후보와 가능 여부

| | 경로 | 접속 문자열 | 가능 | 사용자 1회 준비 |
|---|---|---|---|---|
| **A′** | **사용자 지정 배스천 경유**(§1-2 · `jumpuser@172.16.0.108` → 172.16.0.110:1521) | `oracle://APPDB:…@172.16.0.110:1521/APPDBDEV?ssh=jumpuser@172.16.0.108&driver=thin` | △ **인증 대기** — 22 도달·호스트 키 ✓ · 이 PC 키 거절 | jumpuser의 개인 키 파일(`&ssh_key=<경로>`) **또는** 이 PC 공개 키를 jumpuser `authorized_keys`에 등록(비밀번호 1회 · 터미널) |
| A | DB 서버의 sshd로 터널(172.16.1.58 · 처음 대상) | `oracle://APPDB:…@127.0.0.1:1521/APPDB?ssh=<계정>@172.16.1.58&driver=thin` | 가능했으나 **대상 변경으로 보류** | 계정 + 공개 키 등록 1회 |
| B | Cloudflare Access 점프(`ssh1.example.com`) | — | **종결(사용자 제외 · 10-28)** | — |
| C | VPN(Pulse Secure 172.16.3.x) | — | **종결(사용자 제외 · 10-28)** — VPN은 네트워크 층이라 ssh 점프 호스트가 따로 있어야 터널 경로가 된다 | — |
| D | 로컬 sshd/컨테이너로 자기 PC 경유 | — | ✗ 이 PC에 sshd·docker 없음(설치 = 사용자 시스템 변경 → 묻는다) | Windows OpenSSH Server 설치 시 `?ssh=kiros33@localhost` 가능 |

## 3. 시험 케이스(코드)

- `crates/nsql-drivers/tests/integration.rs::ssh_tunnel_real_server` — `NSQL_SSH_URL`(접속 문자열) 또는 `NSQL_SSH_PROFILE`(볼트 프로필) · 방언 = scheme · 판정 ① 설명 꼬리 " via ssh …" ② 방언별 확인 조회 1행(Oracle `SELECT USER, SYS_CONTEXT('USERENV','SERVER_HOST') FROM dual`) ③ 세션 닫기 → ssh 종료 → **두 번째 라운드**가 새 로컬 포트로 다시 성공 ④ 라운드 60초 상한. env 없으면 생략(CI·다른 PC 무해 · 161차 통합 44/0에서 skip 확인).
- `scripts/win-ssh-tunnel-test.ps1` — ① ssh 선검사(`ssh -o BatchMode=yes <점프> echo ssh-ok` · 실패 원문 + 힌트) ② 위 cargo 시험 ③ CLI 왕복 `nsql run -c <대상> --no-prompt -f csv -`(사용자가 실제로 쓰는 경로). `-Profile`(별칭 · 정식 `-ProfileName`) 쓰면 비밀번호가 명령줄·셸 기록에 안 남는다 · `-AddKnownHost` = 점프 호스트 키를 `~/.ssh/known_hosts`에 추가(사용자 파일 수정이라 명시적 스위치) · `-SkipCargo` = 1·3단계만.
- GUI = 접속 창 [SSH 터널…] 펼침 → 대상·키 입력 → 접속(같은 허브 경로).

## 4. 실행 절차(A′ · 현재 대상)

```powershell
# 0) 1회: jumpuser 인증 준비 — §6-1 ⓐ(키 파일 → 프로필에 &ssh_key=<경로>) 또는 ⓑ(공개 키 등록 · 비밀번호 1회 · 터미널)
# 1) 프로필(비밀번호는 볼트에 · 격리 홈) — 10-28 생성 완료
$env:NSQL_HOME = "$env:TEMP\claude\nsql-ssh-test"
target\debug\nsql.exe conn add appdb-ssh "oracle://APPDB:<pw>@172.16.0.110:1521/APPDBDEV?ssh=jumpuser@172.16.0.108&driver=thin"
# 2) 시험 3단계(ssh 선검사 → cargo → nsql run)
scripts\win-ssh-tunnel-test.ps1 -Profile appdb-ssh -AddKnownHost
```

포워딩 `-L 127.0.0.1:<로컬>:172.16.0.110:1521`의 대상 주소는 **배스천(108)에서 본** 주소다 — 110이 배스천 안쪽 사설 IP로만 열려 있으면 그 IP로 바꾼다(터널은 그대로 108을 지난다). 옛 대상(172.16.1.58 = DB 서버 자체의 sshd)은 호스트를 `127.0.0.1`로 두는 꼴(§2 A).

## 5. 결과 원장

| 날짜 | 경로 | 1) ssh | 2) cargo | 3) CLI | 비고 |
|---|---|---|---|---|---|
| 10-28 | A | ✗ 공개 키 미등록(`kiros33@172.16.1.58` Permission denied · 스크립트 1단계가 원문+힌트로 멈춤 ✓) | 생략 | 생략 | 대상 변경으로 보류 |
| 10-28 | B · C | — | — | — | 사용자 제외로 종결 |
| 10-28 | A′ | ✗ `jumpuser@172.16.0.108` Permission denied(publickey,gssapi-keyex,gssapi-with-mic,password) · 호스트 키 등록 ✓ · 배너 출력 ✓ · 프로필 `appdb-ssh` 생성 ✓ | 생략 | 생략 | 사용자 결정 → 키 등록(ⓐ) |
| 10-28 밤 | **A′** | ✓ `-i id_ed25519` 로그인 OK | ✓ 2라운드(0.66s · 0.49s · USER=APPDB · SERVER_HOST=db-dev-01) | ✓ `VIA_SSH=1` | **전부 통과** · 실제 볼트 등록 ✓ · GUI 실기 = 사용자 |

## 6. 사용자에게 물은 것(세분 · 10-28 밤 전부 해소)

- 결과: 1 = ⓐ(사용자가 jumpuser에 `id_ed25519` 공개 키 등록) · 2 = 실제 볼트 등록 승인(✓ 반영) · 3 = B·C 제외. 아래는 당시 질문 원문.

1. **jumpuser 인증(막힌 지점)** — 셋 중 하나:
   ⓐ jumpuser용 **개인 키 파일**이 있다 → 경로를 알려 주면 프로필에 `ssh_key=`로 넣는다(암호 걸린 키는 ssh-agent에 올려야 BatchMode에서 돈다).
   ⓑ 키가 없고 **비밀번호만** 안다 → 사용자가 터미널에서 1회(비밀번호 입력) 이 PC 공개 키를 등록: `type $HOME\.ssh\id_ed25519.pub | ssh jumpuser@172.16.0.108 "mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"` · 뒤에 내가 스크립트 3단계.
   ⓒ 키 등록이 불가(정책) → 앱이 **비밀번호 ssh 인증**을 지원해야 한다 = T-3 3차 설계(BatchMode 정책 변경 · 사용자 결정 3번과 연결 · Windows에는 sshpass가 없어 자체 구현 필요).
2. **실제 볼트 등록** — 격리 홈의 `appdb-ssh` 프로필을 실제 설정(`nsql-vault`)에도 만들까(사용자 리소스라 승인 뒤 · 또는 접속 창에서 직접 저장).
3. (종결) B · C = 사용자 제외.

## 7. 원리 — ssh 경유 접속은 어떻게 이루어지나(사용자 10-28 요청 · 162차)

### 7-1. 한 줄

앱은 DB 프로토콜을 **고치지 않는다**. 시스템 `ssh`에게 "내 PC의 127.0.0.1:<임시 포트>로 들어오는 연결을 배스천 너머 `<DB 호스트>:<DB 포트>`로 넘겨 달라"(`-L` 로컬 포트 포워딩)고 시키고, 드라이버는 **평소처럼 127.0.0.1:<임시 포트>에 TCP로 붙는다**. 드라이버·확장 ABI는 터널의 존재를 모른다.

### 7-2. 다이어그램(접속 1회)

```
 이 PC (nexa-sql / nsql)                                   배스천 172.16.0.108              DB 172.16.0.110
 ┌──────────────────────────────────────────┐              ┌──────────────┐                ┌──────────────┐
 │ 1 사용자: 프로필 appdb-ssh [접속]          │              │ sshd :22     │                │ Oracle :1521 │
 │    ?ssh=jumpuser@172.16.0.108&ssh_key=…  │              └──────┬───────┘                └──────┬───────┘
 │ 2 허브 nsql-drivers::open                 │                     │                               │
 │    ├ 같은 대상 터널이 살아 있으면 공유 ──┐ │                     │                               │
 │    └ 없으면 빈 포트 P 고르고 ssh 실행    │ │                     │                               │
 │ 3 ssh -N -o BatchMode=yes -i <키>        │ │  SSH 세션(암호화)   │                               │
 │      -L 127.0.0.1:P:172.16.0.110:1521 ───┼─┼═══════════════════►│                               │
 │      jumpuser@172.16.0.108               │ │                     │                               │
 │ 4 127.0.0.1:P 가 받을 때까지 대기(≤15 s)  │ │                     │                               │
 │ 5 드라이버(Oracle Thin) ── TCP ──► 127.0.0.1:P │                │   sshd가 평문 TCP로 이어 줌    │
 │      (스펙의 host/port만 127.0.0.1:P로 바뀜) │◄────────────────►│ ──────────────────────────────►│
 │ 6 Tunneled { 세션, Arc<Tunnel> }          │                     │   TNS 프로토콜 그대로 통과       │
 │    describe = "…@172.16.0.110:1521 via ssh jumpuser@172.16.0.108"                                 │
 └──────────────────────────────────────────┘
```

- 포워딩 대상 `172.16.0.110:1521`은 **배스천이 보는 주소**다(배스천 안쪽 사설 IP여도 된다 · DB가 이 PC에서 직접 안 닿아도 된다).
- SSH 세션 하나 위로 DB 연결 여러 개가 다중화된다(ssh가 연결마다 채널을 연다) → 객체 탐색기·편집기·추가 접속이 **ssh 프로세스 1개**를 나눠 쓴다(162차 공유 · 그 전에는 세션마다 1개 = "까만 창 3개").
- TLS를 함께 쓰면 드라이버가 보는 호스트 이름은 `127.0.0.1`이라 `verify_identity`는 맞지 않는다 → `verify_ca`.

### 7-3. 인증 · 프롬프트가 없는 이유

`BatchMode=yes` = ssh가 **아무것도 묻지 않는다**. ① 호스트 키 = **`StrictHostKeyChecking=accept-new`**(사용자 10-29 결정 · D-392): 처음 보는 호스트는 키를 `known_hosts`에 자동 등록하고, 등록된 키와 **다르면** 거절(서버 교체·중간자) → 안내 = 서버 확인 뒤 `ssh-keygen -R <host>`. 스크립트 `-AddKnownHost`는 이제 선택. ② 인증은 키·에이전트·`~/.ssh/config`(IdentityFile · ProxyCommand 포함)만 — 비밀번호 로그인은 실패하고 안내문이 붙는다(사용자 10-29 결정 = **키만 유지** · SSH_ASKPASS 도우미는 채택하지 않음).

### 7-4. 수명 · 좀비 방지(162차)

```
 접속 ──► Tunnel 생성(ssh pid · 포트) ──► PID 원장에 "소유자 pid ⇥ ssh pid ⇥ 포트" 한 줄 (+ Windows Job Object에 등록)
           │
           ├ 같은 대상 세션 추가 ──► Arc 공유(ssh 그대로 · 원장 그대로)
           │
 세션 전부 닫힘 ──► Arc 마지막 drop ──► ssh kill + wait ──► 원장에서 그 줄 삭제
           │
 앱 비정상 종료 ──► Windows: Job 닫힘 = 커널이 ssh 종료 · 3-OS: 다음 기동 cleanup_stale = 소유자 pid 죽음 + (이름 ssh 또는 포트 열림)인 줄만 종료
```

- 원장 = `<설정 폴더>/run/ssh-tunnels.pids`(`NSQL_HOME` 격리 규약 그대로) · 다른 인스턴스(소유자 살아 있음)의 줄은 건드리지 않는다.
- Windows 콘솔 창 = `CREATE_NO_WINDOW`로 뜨지 않는다(ssh는 stdin 없음 · stderr만 파이프).
- 끊김: 배스천이 세션을 끊으면 ssh가 끝나고 드라이버 소켓이 닫힌다 → 다음 [접속]은 새 터널(자동 재접속 없음 · 26 §8).
