# 25 · 라이선스 종류(Device · User · Team · Organization) · 사내 인증 서버 `nexa-licensed` 설계

> **요청**(사용자 09-14 2차): *"Device당 · User당(총 5대 · OS 무관) · 소규모 조직(1~5명) · 그외 조직(6명 이상)으로 구분하면 어떤가 — 경쟁 제품·최근 정책과 교차 조사·설계. 조직은 내부 인증 서버 + 간단한 데몬으로 서버에 설정된 사용자/대수 기준 사용 — 서버 개발도 설계에 포함."*
> **선행**: [23 정품 인증](23-license-activation.md)(파일 형식 · `check(Feature)` · 기기 ID) — 이 문서는 23의 **§3 모델(D-25)·§6 D-31(Team)을 구체화**한다. 23의 판정 코드·파일 형식은 그대로 두고 **발급 단위와 배포 경로**만 늘린다.
> **상태**: 📐 설계 · 코드 0. 결정 **D-32~D-39**(§7 · §9).

---

## 0. 결론 먼저

사용자 제안 4단계는 **시장 관행과 맞고, 두 군데만 손보면 된다.**

| 제안 | 판정 | 조정 |
|---|---|---|
| Device당 | ✅ TablePlus(1대/2대)와 같은 단위. 공용 PC·랩 환경에 필요 | 그대로. **"그 PC의 누구나"** 명시 |
| User당 · 5대 · OS 무관 | ✅ 관행은 "본인 기기 무제한"(Sublime·DbVisualizer·Beekeeper)이지만 **오프라인 파일 방식에서는 기기를 열거해야 하므로 5대 상한이 정직**하다. DataGrip은 "무제한이지만 동시 1"인데 오프라인에선 동시성을 셀 수 없다 | 그대로. 재발급(기기 교체)은 **셀프 서비스 연 5회**로 마찰 제거(§3-2) |
| 소규모 조직 1~5명 | ✅ TablePlus Team(3석~)·DbVisualizer(3+ 볼륨) 구간 | **서버 없이도 산다** — 사용자 파일 5개 묶음이 기본, 서버는 선택(§3-3). 5명 조직에 데몬 운영을 강제하면 안 팔린다 |
| 그외 조직 6명+ | ✅ JetBrains License Vault · DbVisualizer 단일 키 · Navicat Site와 같은 구간 | **서버 기본**(좌석 배정·회수·감사) · **named-user 좌석이 기본, floating은 옵션**(가격 ×1.5~2) |

인증 서버 = `nexa-licensed`(Rust 단일 바이너리 · 사내망 HTTP · **SosomLab 루트 → 조직 라이선스 → 리스** 3단 Ed25519 체인 · 클라이언트는 전부 오프라인 검증 · 서버 장애 시 리스 TTL 동안 계속 사용).

---

## 1. 경쟁 제품 교차표(2026-09 조사)

| 제품 | 단위 | 기기 수 | 동시 사용 | 조직 방식 | 오프라인 | 모델 | 출처 |
|---|---|---|---|---|---|---|---|
| **TablePlus** | **기기** | Basic 1대 · Standard 2대(iOS는 ×2) · Team $79/석(3석~) | 기기 단위라 무관 | Team = 좌석 · 5석+ 볼륨·중앙 관리 | 라이선스 키 | 영구 + 업데이트 1년(갱신 $59) | [docs.tableplus.com/licensing](https://docs.tableplus.com/utilities/licensing) · [pricing](https://tableplus.com/pricing) |
| **JetBrains DataGrip** | **사용자** | 무제한 · OS 무관 | **동시 1**(집·회사 동시 금지) | **License Vault / Floating License Server** — `JETBRAINS_LICENSE_SERVER` · floating은 IDE 종료 후 **20분**에 좌석 회수 · **48시간** 오프라인 허용(재시작 전) | 오프라인 활성화 코드 | 구독(폴백 영구) | [resellers FAQ](https://resellers.jetbrains.com/hc/en-us/articles/360013819340-May-one-person-use-the-same-license-on-several-computers) · [Floating licenses](https://www.jetbrains.com/help/ide-services/floating-licenses.html) · [License Vault](https://www.jetbrains.com/ide-services/license-vault/) |
| **DBeaver PRO** | **named user** | 본인 전용이면 여러 대 | — | 볼륨 할인 · named user만(concurrent 없음) | 라이선스 키 | **구독 전환(영구 폐지)** $110~$500/년 | [license-types](https://dbeaver.com/license-types/) · [buy](https://dbeaver.com/buy/) |
| **Navicat** | **사용자** | 주 1대 + 보조 1대(동시 사용 금지) · 제거 전 **deactivate 필수** | 동시 1 | Site License · On-Prem Server(협업용 · 라이선스 서버 아님) | 키(온라인 활성화) | 영구(마이너 업데이트) / 구독(이메일 로그인) | [help.navicat.com 1대→2대](https://help.navicat.com/hc/en-us/articles/217757208-Can-I-deploy-one-license-for-both-Desktop-and-Laptop) · [perpetual vs subscription](https://help.navicat.com/hc/en-us/articles/45774388247961-What-is-the-difference-between-Perpetual-License-and-Subscription) · [site license](https://www.navicat.com/en/store/site-license) |
| **DbVisualizer Pro** | **named user** | 무제한 · **동시 사용도 허용**(본인이면) | 본인 한정 | Enterprise = 단일 키 · 사용자 수는 조직이 자체 관리 | 키 | 영구 $199 · 갱신 $89 · 3+ 볼륨 | [licensed per user](https://support.dbvis.com/support/solutions/articles/1000238418-dbvisualizer-is-licensed-per-user) · [work & home](https://support.dbvis.com/support/solutions/articles/1000196571-can-i-use-dbvisualizer-pro-with-the-same-license-at-work-and-at-home-) |
| **Sublime Text** | **사용자**(Personal) / **좌석**(Business) | 본인이 주 사용자인 모든 PC · OS 무관 | 본인 한정 | Business = 구독 좌석 → 개인에게 배정 · 사람 수 ≥ 좌석 | 키 | Personal 영구(업데이트 3년) · Business 구독 | [sales FAQ](https://www.sublimehq.com/sales_faq) |
| **Beekeeper Studio** | **사용자**("사람에게, 기기가 아니라") | — | — | 20+ 청구서 결제 | **오프라인 라이선스 문서** | 구독 + 평생 사용 폴백 | [purchasing](https://docs.beekeeperstudio.io/purchasing/purchasing-a-license/) · [offline licenses](https://docs.beekeeperstudio.io/purchasing/license-types/) |

### 1-1. 최근 정책 흐름(교차표에서 읽히는 것)

1. **사용자 단위가 표준**. 기기 단위는 TablePlus 하나. 개인 개발자는 "내 노트북·데스크톱·맥" 셋을 한 라이선스로 쓰길 기대한다 → **User 티어가 주력**이어야 한다.
2. **기기 상한은 "무제한 + 동시 1"(온라인 검증 가능 제품) 또는 "N대"(오프라인)**. 우리는 오프라인 1차이므로 **N=5**가 맞다(Navicat 2대보다 관대 · Sublime보다 엄격).
3. **조직은 named-user 좌석 배정이 기본, floating은 프리미엄**(JetBrains만 제공 · 회수 20분 · 오프라인 48h). DBeaver는 아예 named user만.
4. **영구+업데이트 기간과 구독이 공존** — DBeaver는 구독으로 갔고 DbVisualizer·TablePlus·Sublime은 영구. 개인 개발자 수용도는 영구가 높다([03 §7](03-competitive-landscape.md)). **개인은 영구, 조직은 연 구독(서버 라이선스 갱신)**이 합리적 절충(Sublime과 같은 구조).
5. **비상업 무료 티어가 확산**(DataGrip 2025-10 · Beekeeper Community) — PolyForm NC와 같은 방향. 유료는 상업 사용자만.
6. **오프라인 활성화는 여전히 필수**(JetBrains 오프라인 코드 · Beekeeper 오프라인 문서) — 폐쇄망 DBA가 우리 1순위 고객.
7. Navicat식 "제거 전 deactivate 필수"는 원성 항목 — 우리는 **셀프 재발급**으로 피한다.

---

## 2. Nexa SQL 라이선스 종류(설계)

| | **Device** | **User** | **Team**(소규모 조직) | **Organization** |
|---|---|---|---|---|
| 대상 | 공용 PC · 랩 · 키오스크 · 부서 공용 워크스테이션 | 개인·프리랜서 · 상업 사용 개인 | 1~5명 조직 | 6명 이상 |
| 묶음 | **기기 1대**(기기 ID) — 그 PC의 모든 사용자 | **사람 1명** — 기기 **최대 5대**(Windows·macOS·Linux 혼용) | 사용자 좌석 **1~5** | 사용자 좌석 **6+** 또는 동시 사용 좌석(옵션) |
| 발급물 | 라이선스 파일 1개(`machine=` 1개) | 라이선스 파일 1개(`machine=` ~5개 · `licensee=` 개인) | 기본 = **User 파일 × 좌석 수**(서버 불요) · 선택 = 조직 라이선스 + 서버 | **조직 라이선스 1개 + `nexa-licensed`**(서버가 좌석 배정) · 폐쇄 단말은 오프라인 리스 |
| 활성화 경로 | 요청 코드 → 이메일/포털 → 파일 | 기기마다 요청 코드 → 파일 재발급(기기 추가) | 관리자가 좌석마다 User 파일 받아 배포 · 또는 서버 | 서버 `init` 요청 코드 → 조직 라이선스 → 데몬 · 클라이언트는 `license.server` 설정만 |
| 모델 | 영구 + 업데이트 1년 | 영구 + 업데이트 1년 | 영구 + 업데이트 1년(좌석) · 서버 쓰면 조직 모델 | **연 구독**(좌석 수 × 단가 · 서버 라이선스 `expires` = 구독 종료 + 유예 30일) |
| 기기 교체 | 연 2회 셀프 재발급 | 연 5회 셀프 재발급 | 좌석당 User와 같음 | 서버가 처리(관리자 회수) |
| 가격 비율(권장) | 1.0 | **1.3**(주력 · TablePlus Standard $129 ≈ 기기 2대와 같은 급) | User × 좌석 × **0.9** | 좌석 × 연 0.5(3년 = 영구 1.5배) · floating 좌석 ×2 |
| 게이트 기능 | Pro 전부(D-23) | Pro 전부 | Pro 전부 | Pro 전부 + Org 전용(감사 로그 내보내기 · 중앙 접속 프로필 배포 — 후속) |

원칙:
- **네 티어 모두 같은 파일 형식·같은 `check(Feature)`**([23 §2-1·§4](23-license-activation.md)). 다른 건 `kind=` 한 줄과 `machine=` 줄 수, 그리고 Organization의 **리스 파일**(§4-3)뿐.
- **User 5대 = 파일에 기기 ID 5개 나열.** 6번째 기기는 재발급 시 가장 오래된 것을 빼거나 사용자가 고른다.
- Team은 **서버 없이 사는 것이 기본**이다. 5명이 데몬을 운영하게 하면 안 팔린다(TablePlus Team도 키만 준다). 서버가 필요해지면 좌석 수는 그대로 두고 조직 모델로 전환.
- 비상업은 여전히 파일 없이 Free(PolyForm NC).

### 2-1. 파일 형식 추가 키([23 §2-1](23-license-activation.md) 확장)

```
kind=device | user | team-seat | org                ← 새 키(없으면 user)
machine=…       machine.2=… … machine.5=…           ← user: ≤5 · device: 1 · org: 없음(서버가 리스에 넣는다)
org=ACME Corp   seats=25   seat_mode=named|device|concurrent   server_key=<base32 32B>   ← org 전용
expires=2027-09-30                                  ← org(구독) · 체험판만 채움 · 개인 영구는 비움
```

`server_key`가 **조직 라이선스와 인증 서버를 묶는다** — 서버가 발급한 리스는 이 키로 검증되고, 다른 서버의 리스는 거부된다.

---

## 3. 발급·재발급 운영

### 3-1. Device · User(개인)

23 §1 그대로. 추가 = **재발급 셀프 서비스**(D-33 온라인 2차의 첫 기능): 포털에서 라이선스 ID + 새 요청 코드 입력 → 새 파일 다운로드 · 발급기 대장이 연간 횟수(Device 2 · User 5)를 센다. 오프라인(이메일)도 같은 규칙.

### 3-2. Team(1~5)

- 구매 시 **좌석 수 N**(1~5). 관리자가 좌석마다 사용자 이름 + 그 사용자의 요청 코드를 보내면 **User 파일 N개**(같은 `order=` 묶음 · 각 `licensee=` 개인).
- 좌석 이동(퇴사) = 해당 파일을 폐기 대장에 올리고 새 사용자로 재발급(연 좌석당 2회).
- 서버로 전환하고 싶으면 §4 조직 라이선스를 `seats=N`으로 발급(추가 비용 없음 · D-32).

### 3-3. Organization(6+)

- 관리자 PC에서 `nexa-licensed init` → 서버 키쌍 생성 + **서버 요청 코드**(`NSQLSRV1.<server pubkey>.<org 메타>`) → SosomLab이 `seats` · `seat_mode` · `expires` · `server_key`가 든 **조직 라이선스** 발급 → `nexa-licensed install <파일>` → 서비스 등록.
- 좌석 추가 = 조직 라이선스 재발급(같은 `server_key` · `seats` 증가) · 서버에 `install`만 다시.
- 구독 갱신 = `expires` 갱신 파일 재발급. 만료 후 **유예 30일**(서버가 리스 계속 발급하되 클라이언트 상태줄에 경고) → 이후 리스 발급 중단 → 클라이언트는 남은 리스 TTL까지 사용 → Free.

---

## 4. 사내 인증 서버 `nexa-licensed` 설계

### 4-1. 신뢰 체인(전부 Ed25519 · 전부 오프라인 검증)

```
SosomLab 루트 공개키(앱·서버에 내장)
   └─ 서명 → 조직 라이선스 (org · seats · seat_mode · expires · server_key · features)
                └─ server_key의 개인키(서버만 보유)가 서명 → 리스 (user · machine · features · issued · expires · org_license_id)
                                                                 └─ 클라이언트 <설정 폴더>/license/lease.conf
```

- 클라이언트는 **리스 + 첨부된 조직 라이선스**를 받아 두 서명을 순서대로 검증하고, `machine`이 자기 기기 ID인지, `expires`가 남았는지 본다. **서버에 다시 묻지 않고** 판정한다 — 서버가 죽어도 TTL 동안 정식.
- SosomLab 비밀키는 **서버 운영에 관여하지 않는다**(발급 시점에만) — 조직은 SosomLab에 접속할 필요가 없다(폐쇄망 OK).
- 서버 개인키는 `nsql-vault` 봉투(Windows DPAPI · 그 외 0600)로 보관 · `init` 시 백업 안내.

### 4-2. 좌석 모델(`seat_mode` · 조직 라이선스에 고정 · 서버 설정으로 완화 불가)

| 모드 | 좌석 = | 배정 | 회수 | 용도 |
|---|---|---|---|---|
| **named**(기본) | 사용자 ID 1개(기기 수 무관 · 사용자당 기기 상한 `max_devices`=5 서버 설정) | 첫 리스 요청 시 자동(자동 배정 끄면 관리자 승인 큐) | 관리자 수동 · 또는 **비활성 N일**(기본 30일) 자동 | 일반 조직 · DBeaver/Sublime과 같은 감각 |
| device | 기기 ID 1개 | 첫 요청 시 자동 | 관리자 수동 · 비활성 N일 | 공용 워크스테이션 위주 조직 |
| concurrent | **동시 활성 리스 수** | 요청 순 · 좌석 없으면 거절(대기 안내) | 하트비트 끊긴 지 **30분**(JetBrains 20분 참고) · 정상 종료 시 즉시 | 교대 근무·큰 조직 · 가격 ×2(D-34) |

### 4-3. 프로토콜(사내망 HTTP/1.1 · 본문은 라이선스 파일과 같은 key=value)

| 요청 | 본문 | 응답 |
|---|---|---|
| `POST /v1/lease` | `user=` `machine=` `host=` `app=` `os=` · 갱신이면 `lease_id=` | 200 = 리스 파일 본문(조직 라이선스 첨부) · 409 = 좌석 없음(`reason=` · `queue=` 위치) · 403 = 거부(차단 사용자·기기) · 410 = 조직 라이선스 만료 |
| `POST /v1/heartbeat` | `lease_id=` | 204 · 404 = 회수됨(클라이언트는 재요청) |
| `POST /v1/release` | `lease_id=` | 204 |
| `GET /v1/status` | — | 서버 버전 · org · seats · 사용 중 · 모드 · expires(인증 없음 · 진단용) |
| `GET /admin/…` `POST /admin/…` | 헤더 `X-Admin-Token` | 좌석 목록 · 사용자/기기 차단·회수 · 오프라인 리스 발급 · 감사 로그 · 조직 라이선스 재설치 |

- **본문 형식을 key=value로 통일**하는 이유: 워크스페이스에 JSON 파서가 없고(core 의존 0), 라이선스 파일·설정 파일·리스가 전부 같은 파서를 쓴다. HTTP 프레이밍만 최소 구현(요청 줄·헤더·`Content-Length` — 200줄 안팎, nexa 계열 자체 구현 관례).
- **TLS는 선택**(리버스 프록시 또는 `rustls` — nsql-net 원장에 이미 있음). 리스가 서명되어 있어 **무결성은 TLS와 무관**하고, 본문에 비밀번호·토큰이 없다(사용자 ID·호스트명뿐). 관리자 API만 토큰 → 사내망 밖 노출 금지·프록시 TLS 권장.
- 서버 시계와 클라이언트 시계 차이 허용 ±10분 · 리스 `issued`가 미래면 거부.

### 4-4. 클라이언트 동작(앱 쪽 · [23 §1-4](23-license-activation.md) 확장)

```
설정 license.server = http://lic.acme.local:7433   (또는 NSQL_LICENSE_SERVER · nsql license server <url>)
     license.user   = (기본 = OS 로그인명 · 조직이 이메일을 쓰면 관리자 안내대로 지정)

부팅 ─▶ lease.conf 있음 & 유효 ──▶ Licensed(Lease)  ─┐
    └▶ 없음/만료 ────────────────▶ Free(잠정)       ─┤
                                                     └─▶ 백그라운드: POST /v1/lease → 성공 = 파일 갱신 · 상태 재계산 · 상태줄 배지
실행 중 ─▶ 10분마다 heartbeat(named/device는 리스 갱신 겸함 · concurrent는 좌석 유지)
종료   ─▶ release(concurrent만 필수 · 나머지는 best-effort)
서버 불통 ─▶ 리스 TTL(기본 7일 · 조직 설정 1~30일)까지 정식 · 남은 기간 3일부터 상태줄 경고 · 만료 = Free
```

- 파일 라이선스와 리스가 둘 다 있으면 **파일 우선**(개인이 조직에 잠깐 들어온 경우).
- **오프라인 리스(borrow)**: 관리자가 특정 사용자·기기에 최대 `offline_max_days`(기본 30)짜리 리스를 발급 → 파일로 전달 → 폐쇄 단말도 사용. 좌석은 그 기간 점유.
- 판정 코드는 변하지 않는다: `LicenseState::Licensed(License)`의 `License.source = File | Lease{server, expires}`만 추가 · `check(Feature)` 동일.

### 4-5. 서버 구현

| 항목 | 선택 |
|---|---|
| 크레이트 | `crates/nexa-licensed`(bin · 워크스페이스) · 공용 로직은 `nsql-license`(형식·서명·검증)를 그대로 의존 · `issuer` feature로 리스 서명 |
| 저장 | **SQLite**(`rusqlite` bundled — 이미 원장) `licensed.db`: `seats(user|machine, assigned_at, last_seen, blocked)` · `leases(id, seat, machine, host, issued, expires, kind)` · `audit(ts, actor, action, detail)` · `config(k,v)` |
| 설정 | `licensed.conf`(nexa-conf): `listen=0.0.0.0:7433` · `lease_ttl_days=7` · `offline_max_days=30` · `inactive_release_days=30` · `heartbeat_timeout_min=30` · `auto_assign=on` · `admin_token_file=` · `allow_users=`(선택 · 쉼표) |
| 관리 | `nexa-licensed admin seats|revoke <user>|block <user>|offline-lease <user> <machine> <days>|audit` + **정적 HTML 관리 페이지 1장**(`/admin/`, 토큰 입력 · 좌석 표·회수 버튼 — JS 최소) |
| 서비스 | Windows `sc create`(스크립트) · systemd unit · launchd plist 동봉 · 로그 = stderr → 서비스 로그 · `--foreground` |
| 규모 | 5~1,000 클라이언트 · 요청 = 사용자당 6회/시간 → 단일 스레드 + 스레드 풀 4개면 충분 · SQLite WAL |
| 가용성 | 단일 인스턴스 + DB 백업 스크립트. 클라이언트가 TTL로 버티므로 **HA 미지원이 정직**(JetBrains도 단일 License Server) · 2노드는 D-37 |
| 보안 | 관리자 토큰 파일 0600 · 리스 요청 초당 20건/IP 제한 · 감사 로그 append-only · 서버 키 봉투 · 관리 API는 `listen_admin=127.0.0.1:7434` 분리 기본 |
| 외부 crate 원장 추가 후보 | `rusqlite`(있음) · `ed25519-dalek`(T-32) · TLS 시 `rustls`(있음) — **HTTP 서버 crate 없음**(자체 200줄) |

### 4-6. 관리자 설치 절차(문서화 대상 · 5분 목표)

```
1. nexa-licensed init --org "ACME Corp" --listen 0.0.0.0:7433
     → 서버 키쌍 생성 · 요청 코드 출력(NSQLSRV1.…) · 관리자 토큰 생성(admin.token)
2. 요청 코드 → SosomLab(이메일/포털) → nexa-sql-org.license 수신
3. nexa-licensed install nexa-sql-org.license      → 서명·server_key 일치 검증
4. nexa-licensed service install                    → OS 서비스 등록·시작(또는 --foreground)
5. 사용자 PC: nsql config set license.server http://lic.acme.local:7433   (GUI 설정 화면 T-39 · 또는 배포 스크립트)
6. 확인: nsql license status  → "Org: ACME Corp · seat user=bae · lease until 2026-09-21 · server ok"
```

---

## 5. 앱·CLI 변경 요약(T-32 위에 얹는 것)

| 층 | 추가 |
|---|---|
| `nsql-license` | `kind`·`machine.N`·`org`·`seats`·`seat_mode`·`server_key` 파싱 · **리스 검증(2단 체인)** · `License.source` · 요청 코드에 `user=` |
| `nsql-settings` | `license.server` · `license.user` · (내부) `license.lease_ttl_hint` |
| CLI | `nsql license server <url>|user <id>|status|renew|release` |
| GUI | 상태줄 배지 `Org · ACME · 6d left` · 서버 불통 경고 · 설정 라이선스 탭에 서버 칸 |
| 서버 | `nexa-licensed`(§4-5) |

---

## 6. 테스트 기준(추가)

| 테스트 | 기대 |
|---|---|
| 체인 검증 | 루트 서명 ✓ + 리스 서명 ✓ = Licensed · 조직 라이선스 변조 = Invalid · 다른 `server_key` 서버 리스 = Invalid |
| named 좌석 | seats=3에 4번째 사용자 → 409 · 관리자 회수 후 → 200 · 같은 사용자 6번째 기기 → 403(`max_devices`) |
| concurrent | 3좌석 · 4번째 → 409 · 하트비트 30분 끊김 → 좌석 회복 · release → 즉시 |
| 오프라인 | 서버 종료 후 TTL 안 = Licensed · TTL 지나면 Free · 시계 뒤로 돌려도 `issued` 미래 거부 |
| 만료·유예 | `expires` 지나면 30일 유예 동안 리스 발급 + 경고 플래그 · 이후 410 |
| 동시성 | 100 클라이언트 동시 요청 → 좌석 수 초과 배정 0(SQLite 트랜잭션) |
| 파일 우선 | 개인 파일 + 리스 → 파일 판정 |

---

## 7. 사용자 결정(D-32~D-38)

| # | 결정 | 권장 |
|---|---|---|
| **D-32** | ★ Team(1~5)의 서버 — ⓐ 파일 묶음 기본 · 서버 선택(권장) ⓑ 서버 필수 ⓒ 서버 미제공 | ⓐ |
| **D-33** | ★ Organization 좌석 모드 — named만 / named + concurrent 옵션(권장 · 가격 ×2) / concurrent만 | named 기본 + concurrent 옵션 |
| **D-34** | ★ 가격 비율 — §2 표(Device 1.0 · User 1.3 · Team 0.9×좌석 · Org 연 0.5×좌석) · 조직은 구독, 개인은 영구 | §2 표 |
| **D-35** | User 기기 5대의 재발급 셀프 서비스 횟수(연 5회 권장) · 포털 착수 시점(= D-29) | 연 5회 · 포털은 이메일로 시작 |
| **D-36** | 조직 사용자 식별 — OS 로그인명(권장 · 설정 없이 동작) vs 이메일 필수 vs LDAP/AD 연동(후속) | OS 로그인명 + `license.user` 덮어쓰기 · LDAP은 요청 시 |
| **D-37** | 서버 가용성 — 단일 + 백업(권장) vs 2노드 복제 | 단일 |
| **D-38** | 리스 TTL 기본(7일 권장 · 1~30 조직 설정) · 오프라인 리스 최대(30일) · 비활성 회수(30일) · 유예(30일) | 권장값 |

---

## 8. 작업(TODO)

| ID | 항목 | 의존 |
|---|---|---|
| **T-40** | `nsql-license` 확장 — `kind`·`machine.N`·org 키 · 리스 2단 체인 검증 · `License.source` | T-32 D-32 D-33 |
| **T-41** | `nexa-licensed` — init/install/service · SQLite 좌석·리스·감사 · HTTP 최소 서버 · named/device/concurrent · 관리 CLI + 정적 관리 페이지 · 3-OS 서비스 스크립트 | T-40 D-36~38 |
| **T-42** | 클라이언트 리스 — `license.server`·`license.user` · 부팅 갱신·하트비트·release · 오프라인 리스 설치 · 상태줄 배지 · `nsql license server/status/renew` | T-40 T-33 |
| **T-43** | 발급기 `nsql-license-tool` 확장 — Device/User(5대)/Team 묶음/Org(`server_key`) 발급 · 재발급 대장(연 횟수) | T-35 |
| **T-44** | 관리자 설치 문서(§4-6) · 부하 테스트(100 동시) · 만료·유예 시나리오 실기 | T-41 T-42 |

---

## 9. 저장소·라이브러리 분리(사용자 09-14 2차 · *"인증서버는 별도 Repository · 공유 기능은 별도 라이브러리"*)

인증 서버는 **SosomLab 비공개 저장소**(kiros33 계정)에서 개발한다. 앱(공개 저장소)과 서버(비공개)가 **같은 형식·같은 서명·같은 검증 코드**를 써야 하므로 공유분을 **형제 저장소 라이브러리**로 뺀다 — nexa-ui와 같은 방식(path 의존 `../nexa-license` · SSH 별칭 clone · CI에서 나란히 체크아웃).

### 9-1. 저장소 3개 · 크레이트 배치

```
SosomLab/nexa-license            ← ★ 공유 라이브러리(형제 저장소 · path 의존)      가시성 = D-39(권장 공개)
  crates/nexa-license            lib · 의존: ed25519-dalek · sha2 · getrandom(+ features)
      format.rs   key=value 문서 파싱/직렬화 · 정규화 서명 대상(PAE식) · base32(Crockford)
      types.rs    License{kind, licensee, org, seats, seat_mode, machines, features: BTreeSet<String>, issued, updates_until, expires, server_key} · Lease{…} · Kind · SeatMode
      verify.rs   verify_license(root_keys, text) · verify_lease(root_keys, text) — 2단 체인 · 기기 일치 · 시각(빌드일·시계) 판정 → Verdict
      request.rs  요청 코드 encode/decode: 클라이언트(NSQLREQ1) · 서버(NSQLSRV1)
      machine.rs  [feature machine-id] 3-OS 기기 ID(Windows MachineGuid · macOS IOPlatformUUID · Linux machine-id)
      sign.rs     [feature issuer]  sign_license · sign_lease · 키쌍 생성 — 발급기·서버만 켠다
      proto.rs    [feature protocol] 리스 요청/응답 메시지 타입 + key=value 인코딩 · 최소 HTTP/1.1 프레이밍(요청 줄·헤더·Content-Length) — 클라이언트는 build, 서버는 parse
      keys.rs     SosomLab 루트 공개키 상수(ROOT_V1 · 회전용 ROOT_V2 자리) — 공개키라 공개 무방
                                     ★ Feature 열거형은 여기 없다(앱 도메인) — features는 문자열 집합
SosomLab/nexa-sql (공개 · 이 저장소)
  crates/nsql-license            얇은 앱 층 · 의존: nexa-license(machine-id, protocol)
      Feature 열거형 + 문자열 매핑 · LicenseState · check(Feature) · Licensing(파일/리스 폴더 관리 · 갱신 스케줄) — [23 §4-1](23-license-activation.md)
SosomLab/nexa-license-server     ← 비공개 · 의존: nexa-license(issuer, protocol)
  crates/nexa-licensed           bin — §4-5 서버(HTTP · SQLite · 좌석 · 관리 페이지 · 서비스 스크립트)
  crates/nexa-license-tool       bin — 발급기(§3 · 루트 비밀키로 개인/조직 라이선스 발급 · 재발급 대장) — 운영 도구라 서버와 같은 비공개 저장소
```

- 의존 방향은 **한쪽**: `nexa-license` ← {`nsql-license`, `nexa-licensed`, `nexa-license-tool`}. 라이브러리는 앱·서버를 모른다(nexa-ui DR-2와 같은 원칙 — 도메인 의존 0 · `Feature`·UI 문자열·저장 정책은 주입).
- **공개키만 라이브러리에**, 비밀키는 어느 저장소에도 없다(발급 PC 봉투 · D-27).
- 이름을 `nsql-`이 아니라 `nexa-`로 두는 이유: 계열 앱(nexa-clip 등)이 같은 라이선스 체계를 쓸 수 있게([13 §3](13-licensing.md) "계열 공용 후보"). `product=` 키가 앱을 구분한다.

### 9-2. 저장소 간 연결 규약

| 항목 | 규약 |
|---|---|
| clone | `git@kiros33.github.com:SosomLab/nexa-license.git` · `…/nexa-license-server.git`(SSH 별칭 · 09-13 사용자 지정) — `D:\Projects\kiros33\` 아래 나란히 |
| 앱 의존 | `Cargo.toml` `nexa-license = { path = "../nexa-license/crates/nexa-license", features = ["machine-id", "protocol"] }` |
| 서버 의존 | 같은 path 의존 + `features = ["issuer", "protocol"]` |
| CI(nexa-sql) | `actions/checkout` 한 줄 추가(`repository: SosomLab/nexa-license` · `path: nexa-license`) — 라이브러리가 **비공개면 `token: ${{ secrets.SIBLING_TOKEN }}`**(fine-grained PAT · contents:read) 필요 → D-39 |
| CI(server) | 비공개끼리 — 같은 PAT · 3-OS 빌드 + 서비스 스크립트 smoke |
| 버전 | 라이브러리 `format=nsl1` 상수가 호환 계약 · 형식 바꾸면 `nsl2` + 두 판 동시 검증 · 앱·서버는 라이브러리 커밋을 각자 고정하지 않는다(path) — 릴리스 시점에 태그로 기록 |
| 테스트 픽스처 | 라이브러리에 **테스트 전용 루트 키쌍**(`tests/fixtures/`) — 앱·서버 테스트가 같은 픽스처로 라이선스·리스를 만든다(실 루트 키와 다른 키 · `keys::ROOT_TEST`) |

### 9-3. 착수 순서(라이브러리 먼저)

1. **T-45** `SosomLab/nexa-license` 생성 · `format`·`types`·`verify`·`request`·`machine`·`sign`·`keys`·픽스처 · 3-OS CI — 23 §5 테스트 전부 여기로.
2. **T-32'** nexa-sql `nsql-license` = 얇은 층(Feature 매핑 · 상태 · 파일 관리) + CI 체크아웃 추가.
3. **T-41** `SosomLab/nexa-license-server`(비공개) — `nexa-licensed` + `nexa-license-tool`.
4. T-42 클라이언트 리스 · T-44 설치 문서·부하 테스트.

D-39: 라이브러리 가시성 — **공개 권장**(검증 코드는 어차피 공개 앱 바이너리에 들어가고 서명 코드는 비밀이 아니다 · CI 토큰 불요 · 계열 앱 재사용 쉬움) vs 비공개(코드 노출 최소 · CI PAT 필요).
