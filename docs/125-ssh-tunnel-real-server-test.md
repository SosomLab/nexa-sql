# 125. 실서버 SSH 터널 시험 — 로컬 ssh 설정으로 Oracle 192.168.0.58에 붙기

> 요구(사용자 10-28 · 161차 win): "`oracle://BISCM:…@192.168.0.58:1521/BISCM` 이 서버에 **로컬에서 설정된 ssh**를 통해 접속되는 테스트 케이스" + "Cloudflare나 VPN 접속 정보로 시험 가능한지도". 기능 자체 = T-3 SSH 터널([21 §10](21-connection-profiles.md) · D-388) · 이 문서는 **실서버 시험 케이스**와 그 전제(이 PC의 네트워크·ssh 상태) 원장이다. 비밀번호는 여기 적지 않는다(볼트 프로필 또는 셸 환경변수).

## 1. 이 PC의 상태(10-28 조사 · 읽기 전용)

| 항목 | 결과 | 뜻 |
|---|---|---|
| 로컬 IP | 이더넷 3(USB) 192.168.110.137 · **이더넷 2 = Juniper Networks Virtual Adapter 192.168.211.93(Pulse Secure VPN · 연결됨)** · Wi-Fi 끊김 | 192.168.0.x 대역이 아님 |
| 192.168.0.58 경로 | 기본 게이트웨이 192.168.110.1(이더넷 3) 경유 — **VPN 경로 아님** | 사무실 라우팅으로 닿는다 |
| 192.168.0.58:**1521** | TCP 도달 ✓ | Oracle 직접 접속 가능(터널 없이도) |
| 192.168.0.58:**22** | TCP 도달 ✓ · sshd 응답 · 인증 방식 = `publickey, gssapi-keyex, gssapi-with-mic, password` | DB 서버 자체가 ssh 점프 호스트가 될 수 있다 |
| 이 PC 키로 로그인 | `kiros33@192.168.0.58` = Permission denied(공개 키 미등록) · known_hosts = 조사 중 `accept-new`로 등록됨(10-28) | 계정 + 공개 키 등록 1회 필요 · 호스트 키는 끝 |
| `~/.ssh/config` 별칭 | github/bitbucket(쓸모 없음) · `gitea.local`(192.168.0.60 · **22 미도달**) · **`ssh1.sosomlab.com` = ProxyCommand `cloudflared access ssh`** | Cloudflare Access 점프 후보 |
| cloudflared | 설치됨(2026.10.3) · 돌고 있는 것은 **`tunnel run`(이 PC를 원점으로 내보내는 데몬)** · `~/.cloudflared` 없음 = Access 로그인 토큰 없음 | `ssh1.sosomlab.com` 첫 접속은 브라우저 로그인 1회 |
| 로컬 sshd · docker | 둘 다 없음 | CI처럼 sshd 컨테이너를 띄우는 길은 이 PC에 없음 |
| 앱 쪽 전제 | 터널은 **시스템 `ssh`**를 그대로 쓴다 → `~/.ssh/config`(별칭·IdentityFile·ProxyCommand)·에이전트가 그대로 먹는다 · `BatchMode=yes` = 비밀번호·호스트 키 프롬프트 없음 | 로컬 ssh 설정이 곧 앱 설정 |

## 2. 경로 후보와 가능 여부

| | 경로 | 접속 문자열 | 가능 | 사용자 1회 준비 |
|---|---|---|---|---|
| **A** | **DB 서버의 sshd로 터널**(정석: ssh로 .58에 들어가 그 안의 1521로 포워딩) | `oracle://BISCM:…@127.0.0.1:1521/BISCM?ssh=<계정>@192.168.0.58&driver=thin` | ✓ 가능(22 도달 · publickey 허용) | ① .58의 ssh 계정 이름 ② 이 PC 공개 키 등록(`~/.ssh/kiros33_ed25519.pub` 등 → 서버 `~/.ssh/authorized_keys` · 비밀번호는 그때 터미널에서 1회) ③ known_hosts 등록(스크립트 `-AddKnownHost` 또는 터미널 1회 접속) |
| **B** | **Cloudflare Access 점프**(`ssh1.sosomlab.com` → 192.168.0.58:1521) | `oracle://BISCM:…@192.168.0.58:1521/BISCM?ssh=ssh1.sosomlab.com&driver=thin`(`user@` 생략 = config의 `User kiros33`) | △ 조건부 — ssh1이 192.168.0.58:1521에 닿아야 한다(미확인) · ProxyCommand는 BatchMode와 공존 | ① `cloudflared access login https://ssh1.sosomlab.com`(브라우저 1회 · 토큰 `~/.cloudflared`) ② ssh1에서 `nc -z 192.168.0.58 1521` 확인 |
| **C** | **VPN(Pulse Secure 192.168.211.x)** | — | ✗ 터널 경로 아님 — VPN은 네트워크 층이라 ssh 점프 호스트가 따로 있어야 한다 · 지금 .58은 VPN이 아니라 사무실 라우팅으로 닿는다 | VPN 쪽 ssh 호스트가 있으면 A와 같은 꼴로 시험 |
| D | 로컬 sshd/컨테이너로 자기 PC 경유 | — | ✗ 이 PC에 sshd·docker 없음(설치 = 사용자 시스템 변경 → 묻는다) | Windows OpenSSH Server 설치 시 `?ssh=kiros33@localhost` 가능 |

권고 = **A**(가장 단순 · 실제 운영 모양 · 비밀번호 없이 키만). B는 A 뒤 보너스(Cloudflare 경유가 BatchMode·ProxyCommand에서 도는지 확인하는 가치).

## 3. 시험 케이스(코드)

- `crates/nsql-drivers/tests/integration.rs::ssh_tunnel_real_server` — `NSQL_SSH_URL`(접속 문자열) 또는 `NSQL_SSH_PROFILE`(볼트 프로필) · 방언 = scheme · 판정 ① 설명 꼬리 " via ssh …" ② 방언별 확인 조회 1행(Oracle `SELECT USER, SYS_CONTEXT('USERENV','SERVER_HOST') FROM dual`) ③ 세션 닫기 → ssh 종료 → **두 번째 라운드**가 새 로컬 포트로 다시 성공 ④ 라운드 60초 상한. env 없으면 생략(CI·다른 PC 무해).
- `scripts/win-ssh-tunnel-test.ps1` — ① ssh 선검사(`ssh -o BatchMode=yes <점프> echo ssh-ok` · 실패 원문 + 힌트) ② 위 cargo 시험 ③ CLI 왕복 `nsql run -c <대상> --no-prompt -f csv -`(사용자가 실제로 쓰는 경로). `-Profile` 쓰면 비밀번호가 명령줄·셸 기록에 안 남는다.
- GUI = 접속 창 [SSH 터널…] 펼침 → 대상·키 입력 → 접속(같은 허브 경로).

## 4. 실행 절차(A)

```powershell
# 0) 1회: 서버에 공개 키 등록(비밀번호는 이때만 · 터미널) — <계정>은 .58의 리눅스 계정
type $HOME\.ssh\kiros33_ed25519.pub | ssh <계정>@192.168.0.58 "mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
# 1) 프로필(비밀번호는 볼트에 · 격리 홈 권장)
$env:NSQL_HOME = "$env:TEMP\nsql-ssh-test"
target\debug\nsql.exe conn add biscm-ssh "oracle://BISCM:<pw>@127.0.0.1:1521/BISCM?ssh=<계정>@192.168.0.58&driver=thin"
# 2) 시험 3단계
scripts\win-ssh-tunnel-test.ps1 -Profile biscm-ssh -AddKnownHost
```

호스트가 `127.0.0.1`인 이유 = 포워딩 `-L 127.0.0.1:<로컬>:127.0.0.1:1521`은 **ssh 서버 쪽에서 본** 주소라 .58 안의 리스너를 가리킨다. 리스너가 외부 IP에만 묶였으면 `@192.168.0.58:1521`로 바꾼다(그래도 터널은 .58을 지난다).

## 5. 결과 원장

| 날짜 | 경로 | 1) ssh | 2) cargo | 3) CLI | 비고 |
|---|---|---|---|---|---|
| 10-28 | A | ✗ 공개 키 미등록(`kiros33@192.168.0.58` Permission denied · 스크립트 1단계가 원문+힌트로 멈춤 ✓) | 생략 | 생략 | **사용자 결정 대기**: .58 ssh 계정 + 공개 키 등록 1회 |
| 10-28 | B | 미시도(Access 로그인 토큰 없음 · 브라우저가 떠 사용자 몫) | — | — | ssh1 → .58:1521 도달 여부도 미확인 |

## 6. 사용자에게 묻는 것(세분)

1. **A**: 192.168.0.58의 ssh 계정 이름(예 `oracle`) — 그 계정으로 공개 키를 등록해도 되는가(§4 0단계는 사용자가 터미널에서 실행 · 비밀번호 1회).
2. **B**: `cloudflared access login https://ssh1.sosomlab.com`을 한 번 해 줄 수 있는가(브라우저) · ssh1이 192.168.0.x에 닿는 호스트인가.
3. **C**: VPN(Pulse) 쪽에 ssh 점프 호스트가 따로 있는가(없으면 C는 종결).
