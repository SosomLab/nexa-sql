# 25 · 라이선스 종류(Device · User · Team · Organization) · 사내 인증 서버 `nexa-licensed` 설계

> **요청**(사용자 09-14 2차): *"Device당 · User당(총 5대 · OS 무관) · 소규모 조직(1~5명) · 그외 조직(6명 이상)으로 구분하면 어떤가 — 경쟁 제품·최근 정책과 교차 조사·설계. 조직은 내부 인증 서버 + 간단한 데몬으로 서버에 설정된 사용자/대수 기준 사용 — 서버 개발도 설계에 포함."*
> **선행**: [23 정품 인증](23-license-activation.md)(파일 형식 · `check(Feature)` · 기기 ID) — 이 문서는 23의 **§3 모델(D-25)·§6 D-31(Team)을 구체화**한다. 23의 판정 코드·파일 형식은 그대로 두고 **발급 단위와 배포 경로**만 늘린다.
> **상태**: 📐 설계 · 코드 0. 결정 **D-32~D-40**(§7 · §9 · §10). **§10 = 범용성 점검(beep·clip·dir2) — §9-1 계층은 §10-3 수정본이 우선.** §11 = 발급 코드 위치(공개 라이브러리는 검증 전용) · 장치 vs 사용자 인증 차이 · 백업/자동 복원.

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

---

## 10. 범용성 점검 — beep · clip · dir2에 그대로 쓸 수 있는가(사용자 09-14 3차)

**결론: 골격(형식·서명·체인·기기 ID·서버)은 범용이지만, 23·25 초안에는 nexa-sql 전용 가정이 7곳 남아 있다.** 아래 표대로 고치면 세 앱 모두 `nexa-license` 한 판을 쓴다. 대조 근거 = 각 저장소 실측(09-14).

### 10-1. 형제 앱 제약 실측

| 앱 | 암호 crate | 설정 저장 | 설정 폴더 | 외부 crate 정책 | 라이선스 관련 기존 결정 |
|---|---|---|---|---|---|
| **nexa-beep** | `ed25519-dalek = "2"` · `snow 0.9.6`(curve25519-dalek 4.1.3 공유 · **2.x 핀**) · `sha2 0.10` · `getrandom 0.2` | 자체 vendored `crates/nexa-conf` | `user_config_dir("nexa-beep")` | DR-21 **외부 기술은 포트 뒤에**(포트 시그니처에 외부 타입 금지) | DR-12 PolyForm NC · 계측 정당성 = 상업 라이선스 |
| **nexa-clip** | `snow 0.9.6` · `sha2 0.10`(nclip-sync) · dalek 없음(추가 시 beep과 같은 판이면 트리 중복 0) | 자체 vendored `crates/nexa-conf`(DR-32 계열 공용) · `nclip-store` 봉투(`sealed.rs`·`keys.rs`) | `user_config_dir("nexa-clip")` | 원장 기록 | PolyForm NC |
| **nexa-dir2** | **없음** — B3 게이트 = **OS 인박스 DLL만 임포트**(bcrypt·crypt32 화이트리스트) · Windows 전용 | 자체 `settings.cfg`(key=value) · nexa-conf 아님 | **exe 옆 `data\`**(포터블 DR-3 · 쓰기 불가면 `%LOCALAPPDATA%\NexaDir\data`) | **외부 crate 0**(B2 exe ≤10MB) | **X-15 오프라인 라이선스 계획: ECDSA P-256 + Windows CNG(bcrypt.dll) · exe 옆 `license.key` · 발급 CLI `nexa-lic`** |
| nexa-sql | dalek(예정) · sha2 · getrandom · rusqlite | nexa-ui `nexa-conf`(path) · `nsql-vault` 봉투 | `user_config_dir("nexa-sql")` / `NSQL_HOME` | DR-3 원장 | 23 · 25 |

공통: edition 2021 · rust-version 1.82 · sha2 0.10 · getrandom 0.2 — 라이브러리 MSRV·판을 여기에 맞추면 충돌 없음.

### 10-2. 초안에 남은 nexa-sql 전용 가정과 수정

| # | 초안(23·25) | 문제 | 수정(라이브러리 설계에 반영) |
|---|---|---|---|
| 1 | 요청 코드 `NSQLREQ1`·`NSQLSRV1` · `format=nsl1` · 파일 `nexa-sql.license` · 도메인 태그 `nexa-sql/license/v1` | 이름이 제품에 박혀 있다 | **`Product` 기술자**를 호출측이 넘긴다: `Product { id: "nexa-sql" \| "nexa-beep" \| …, license_file: "<id>.license" }`. 접두는 공통 `NEXAREQ1`·`NEXASRV1` · `format=nxl1` · 서명 도메인 `nexa/license/v1`(제품 구분은 서명 본문의 `product=`가 한다 — 도메인에 넣지 않아야 한 루트 키로 전 제품 발급) |
| 2 | 기기 ID 도메인 `nexa-sql/license/machine-v1` | 제품마다 기기 코드가 달라져 사용자가 앱마다 다른 요청 코드를 보내야 한다 | 도메인 `nexa/machine-v1`로 **계열 공통 기기 코드** — 한 PC = 한 코드. User 라이선스가 여러 제품을 묶는 번들도 가능해진다(`product=nexa-sql,nexa-clip`) |
| 3 | `<설정 폴더>/license/` 고정 · nexa-conf `write_atomic` 재사용 | dir2는 exe 옆 `data\`(포터블) · beep/clip은 nexa-conf를 **각자 vendored**(nexa-ui path를 끌어오면 크레이트 중복) | 라이브러리는 **경로를 받는다**(`Licensing::open(dir)`) · **파서·원자적 쓰기는 자체 구현**(nexa-conf 의존 0 · 형식이 단순해 80줄) — 각 앱이 자기 폴더 규칙으로 부른다 |
| 4 | 서명 = `ed25519-dalek` 고정 | **dir2는 외부 crate 0(B3)** — dalek을 못 들인다. X-15는 CNG P-256을 계획했고 CNG에는 Ed25519가 없다 | **서명 알고리즘을 포트로**: `trait SigVerifier { fn verify(alg, key, msg, sig) }` · 파일에 `alg=ed25519 \| p256` · 라이브러리 기본 feature `ed25519`(dalek 2.x — beep 핀과 동일) · dir2는 `alg=p256` + **CNG 어댑터**(bcrypt.dll · dir2 저장소에서 구현 · 인박스) · 루트 키는 알고리즘별 2개(`ROOT_ED25519_V1` · `ROOT_P256_V1`) · 발급기는 둘 다 서명. beep DR-21과도 맞는다 → **D-40** |
| 5 | `Denial { reason, hint }`에 안내 문구 · 오류 메시지 문자열 | 앱마다 i18n 체계가 다르다(beep/clip `Msg` · dir2 `.lang` 파일) | 라이브러리는 **문자열을 만들지 않는다** — `Reason`·`InstallError` **열거형만** 반환 · 문구는 앱의 i18n(nsql-i18n `Msg::License*`) |
| 6 | `NSQL_BUILD_DATE` · 설정 키 `license.server`·`license.user` | 환경변수 이름·설정 레지스트리가 앱 소유 | 빌드일은 `Product.build_date`로 주입(각 앱 `build.rs`) · 설정 키는 각 앱 레지스트리에 같은 이름으로(권장 관례일 뿐 라이브러리는 모른다) |
| 7 | 서버 = 조직 라이선스 1개 · `product=nexa-sql` 암묵 | 한 조직이 beep·clip·sql을 함께 쓰면 데몬 3개? | **한 데몬이 제품별 조직 라이선스 N개 보유** · 리스 요청에 `product=` 필수 · 좌석 풀은 제품별 · 관리 페이지에 제품 탭 · 번들 좌석(한 사용자 = 전 제품)은 조직 라이선스 `product=*`로 |

### 10-3. 라이브러리 계층(수정본 · §9-1 대체)

```
nexa-license (lib · 의존 기본 0 · features)
  core(항상)   format(자체 key=value 파서·정규화·base32) · types(Product·License·Lease·Kind·SeatMode·Reason) · request(NEXAREQ1/NEXASRV1)
               verify(체인 검증 — SigVerifier 포트 · 시각·기기 판정 · 순수 함수 · I/O 0)
  ed25519      dalek 2.x 어댑터(기본 켬 · beep/clip/sql)          ← dir2는 끈다
  machine-id   3-OS 기기 ID(std만 · Windows 레지스트리/ioreg/machine-id)
  fs           Licensing(폴더 받아 파일·리스 관리 · 원자적 쓰기 자체 구현)   ← beep은 포트 뒤에서 호출
  protocol     리스 메시지 + 최소 HTTP 프레이밍(클라이언트 build · 서버 parse)
  (issuer 없음) 서명·키 생성은 비공개 nexa-license-server(nexa-license-tool)에만 — §11-1
  keys         SosomLab 루트 공개키(ed25519 · p256 각 1) + 테스트 픽스처
```

각 앱이 더하는 것 = `Product` 상수 1개 · `Feature` 열거형 ↔ 문자열 매핑 · 게이트 진입점 · i18n 문구 · 설정 키 2개 · (dir2만) CNG P-256 어댑터 ~150줄.

### 10-4. 앱별 적용 예상

| 앱 | 쓰는 feature | 앱이 추가로 쓰는 것 | 걸림돌 |
|---|---|---|---|
| nexa-sql | ed25519 · machine-id · fs · protocol | `nsql-license` 얇은 층(23 §4-1) | 없음 |
| nexa-clip | ed25519 · machine-id · fs · protocol | `Product{id:"nexa-clip"}` · 상업 기능 목록(동기화 서버? 후속) · nclip-store 봉투로 리스 키 보관 불요(리스는 공개 정보) | dalek 추가 = beep과 같은 판이면 트리 +2 crate · 원장 기록 |
| nexa-beep | ed25519 · machine-id · fs | 이미 dalek 있음(nbeep-crypto) · DR-21대로 `LicensePort` trait 뒤에 라이브러리 배치 · 계측 레저와 연계(DR-15 ④ 상업 근거) | 없음 — 단 DR-15 파이프라인(Policy 단계)에 게이트가 들어가야 한다 |
| nexa-dir2 | machine-id(Windows만) · fs | `alg=p256` CNG 어댑터 · exe 옆 `data\license.key` · 발급기는 같은 `nexa-license-tool`(p256 서명) — **X-15의 `nexa-lic` 별도 개발이 불필요해진다** | 외부 crate 0 유지 가능(ed25519 feature 끔 · std만) · Linux/macOS 미지원은 dir2 자체가 Windows 전용이라 무관 |

⚠️ dir2 X-15가 잡은 형식(exe 옆 `license.key` · 이름·이메일·에디션·HWID 옵션)은 이 라이브러리 형식으로 **대체**된다 — dir2 TODO에 정정 등재 필요(그쪽 저장소 작업).

---

## 11. 4차 질문 3건 — 발급 코드 위치 · 장치 인증 vs 사용자 인증 · 백업/복원(사용자 09-14)

### 11-1. "키 생성 규칙이 라이브러리에 있으면 누구나 발급하는 것 아닌가"

- **정책의 안전은 코드가 아니라 루트 비밀키 보관에 있다.** 키 생성에는 "규칙"이 없다 — OS 난수로 만든 무작위 키쌍이며, 유출될 알고리즘 비밀이 없다(Ed25519는 공개 표준). 다른 키로 서명한 파일은 **공개키만 내장한 앱이 거부**한다. 루트 비밀키는 어느 저장소에도 없고 발급 PC 봉투(DPAPI)에만 있다(D-27).
- 그래도 **발급 코드(서명 · 키 생성 · 발급기)는 공개 라이브러리에서 뺐다**(09-14 반영 · `issuer` feature 삭제) — 비공개 `nexa-license-server`의 `nexa-license-tool`만 가진다. 공개 라이브러리는 **검증 전용** + 사전 서명된 테스트 픽스처(테스트 루트 키는 실 루트와 무관 · 앱 테스트는 픽스처 파일만 읽는다). 원스톱 "keygen 재료"를 주지 않기 위한 조치이지, 안전의 근거는 아니다.
- 남는 실제 위협은 처음부터 명기한 그것 하나 — **앱 바이너리 안의 공개키를 바꿔치기하는 패치**([23 §2-3](23-license-activation.md)). 모든 로컬 인증의 공통 한계이며 난독화로 막지 않는다(DR-1).

### 11-2. 장치 인증 vs 사용자 인증 — 설계 차이

| 항목 | **Device(장치 인증)** | **User(사용자 인증)** |
|---|---|---|
| 묻는 질문 | **"이 PC인가"** | **"이 사람인가 + 이 사람이 등록한 PC인가"** |
| 증명 재료 | 기기 ID 1개(`machine=`) | 라이선스 ID + `licensee`(이름·이메일) + 등록 기기 `machine=`…`machine.5=` |
| 누가 쓰나 | **그 PC의 모든 OS 계정**(공용 워크스테이션·랩) | 본인만 — 기술적 제한은 **기기 열거**(5대)로, 사람 식별은 약관(오프라인에서 사람을 검증할 수단은 없다 · 시장 관행 동일) |
| 파일 저장 위치 | **기기 공용 폴더** — Windows `%ProgramData%\nexa\<product>\` · macOS `/Library/Application Support/nexa/<product>/` · Linux `/etc/nexa/<product>/`(쓰기 불가면 사용자 폴더 폴백 + 안내) | **사용자 프로필 폴더** `user_config_dir(<product>)/license/`(dir2는 exe 옆 `data\`) |
| 활성화 주체 | PC 관리자(요청 코드는 그 PC에서 · 설치 권한) | 사용자 본인(기기마다 요청 코드 → 기기 추가 재발급) |
| **다른 PC로 가져가면** | **거부 `Invalid(Machine)`** — "이 라이선스는 다른 PC용". 이동 = 재발급(연 2회 · 이전 기기 폐기 대장) | 등록된 5대 안 = 그대로 사용 · 밖 = 거부 + "기기 추가(재발급 연 5회)" 안내 |
| 같은 PC 앱 재설치 · OS 계정 추가 | 기기 ID 불변 → **그대로**(공용 폴더에 남아 있음) | 그대로(각 사용자 폴더) |
| OS 재설치(기기 ID 변경) | 재발급 필요(정직한 한계 · 셀프 서비스) | 기기 1대분 교체 재발급 |
| 조직 서버 대응 | `seat_mode=device` · 리스 = `machine` | `seat_mode=named` · 리스 = `user` + `machine` |
| 파일 `kind=` | `device` | `user` / `team-seat` |
| 판정 순서(앱) | 사용자 폴더 파일 → **기기 공용 파일** → 리스 → Free(둘 다 있으면 사용자 파일 우선) | 같음 |

두 방식의 **검증 코드는 하나**다(체인·서명·기기 일치). 다른 것은 ① 파일이 사는 폴더(공용 vs 개인) ② 기기 열거 수(1 vs 5) ③ 활성화 주체·재발급 횟수 ④ 서버 좌석 모드. 이 넷은 `Kind`에서 갈라진다(`Kind::max_machines()` · 저장 폴더는 호출측이 `Kind`로 고른다).

### 11-3. 인증 백업·복원 — 가능하고, **"백업 안 해도 남은 폴더로 복원"이 기본**(사용자 09-14)

- 라이선스 파일 = 자격 그 자체 · 안에 비밀이 없다(공개키 검증) → **복사가 곧 백업**. 앱 기능(T-46): `nsql license export <경로>` / `import <파일>` · GUI 설정 라이선스 탭 "백업…/복원…"(파일 대화상자) · 백업 묶음 옵션 = 라이선스 + `settings.conf` + 프로필(⚠️ 프로필 비밀번호 봉투는 DPAPI 기기 키라 **다른 PC에서는 열리지 않는다** — 복원 화면에 정직하게 표시 · [21 §5](21-connection-profiles.md)).
- **자동 복원**: 앱 시작 시 알려진 위치를 전부 훑는다 — 사용자 폴더 → 기기 공용 폴더 → exe 옆 → 이전 버전 폴더명. 유효한 파일이 있으면 **조작 없이 채택**(앱 재설치 · OS 계정 그대로 · 설정 폴더 잔존 = 그냥 복원). 이미 어느 정도 되는 이유: 라이선스가 설정 폴더에 살고 설정 폴더는 앱을 지워도 남는다.
- **복원의 안전은 파일이 아니라 기기 ID 검증이 맡는다** — 그래서 "남은 폴더로 복원"과 "Device는 다른 PC에서 제약"이 충돌하지 않는다. Device 파일을 폴더째 다른 PC로 옮기면 `Invalid(Machine)` · User 파일은 등록 5대 안에서만 · 리스는 `machine` 불일치로 거부.
- 백업도 폴더도 없는 경우: **라이선스 ID + 구매 이메일로 재발급**(발급 대장). 앱은 "라이선스 ID 복사" 버튼을 두고 구매 메일에도 ID가 있으므로 "인증 정보 백업"의 실체는 ID 보관이다.
- 조직 리스는 백업 대상이 아니다(서버가 재발급 · 오프라인 리스도 관리자가 재발급).
- 결정 사항 없음(관례) · 작업 = **T-46**.

## 12. 오프라인 발급기 CLI `nexa-license-tool` 사양(**확정용** · 사용자 09-27 "위치·구현 방법·입력·메커니즘·출력·전달·적용을 순차로 정리해 일괄 개발")

### 12-1. 위치·형태

| | 결정 |
|---|---|
| 저장소 | **비공개 `SosomLab/nexa-license-server`** `crates/nexa-license-tool`(§9-1 · 서버와 같은 저장소 · 이 기기엔 아직 clone 없음) |
| 의존 | `nexa-license`(공개 · path `../nexa-license/crates/nexa-license` · `features = ["issuer"]`) — 서명·키 생성 코드는 **issuer feature 뒤에만**(공개 저장소 규칙 "검증 전용") |
| 형태 | 단일 바이너리 CLI(Rust · 외부 crate = `ed25519-dalek`·`sha2`·`getrandom`만) · 대화식 프롬프트 없음(스크립트 가능) · 종료 코드로 결과 |
| 실행 기기 | SosomLab 발급 PC 1대(오프라인 가능) · 비밀키는 그 PC 밖으로 안 나간다(D-27) |

### 12-2. 메커니즘(23 §1-2 · §2)

```
사용자 PC  nsql license request ──▶ NEXAREQ1.<base32 기기ID 20B>.<base32 메타>  (이메일 본문 한 줄)
발급 PC    nexa-license-tool issue --request … --kind user --licensee … ──▶ 검증(형식·중복) → Doc 조립(23 §2-1 키) →
           canonical("nexa/license/v1") 위에 Ed25519 서명(루트 비밀키 · 봉투에서 풀어 메모리에만) → nexa-sql.license 파일 + 대장 1행
사용자 PC  nsql license install <파일> / GUI 드롭 ──▶ 공개키(ROOT_V1)로 검증 · 기기 일치 · 기한 → Licensed
```
- 서명 대상·정규화·base32·`Kind`/`SeatMode`/`Alg`는 **이미 `nexa-license`에 있다**(`format::Doc::canonical` · `base32` · `types`). 발급기가 새로 만드는 건 `sign`(issuer) · `request` 디코드 · 대장뿐.
- 키 회전: 공개키 2개 동시 내장(ROOT_V1·V2) → 새 키로 발급해도 옛 앱이 검증(D-27).

### 12-3. 입력값 정의(`issue` 명령)

| 인자 | 필수 | 의미 · 검증 |
|---|:--:|---|
| `--request <코드>` (1~5회 · User 5대) | ✅(Team/Org 서버 발급 제외) | `NEXAREQ1` 접두 · base32 20B 기기 ID · 메타(os·app·d·n)는 표시·대장용 |
| `--kind device|user|team|org` | ✅ | `types::Kind` |
| `--licensee "<이름 또는 조직>"` · `--email` | ✅ · 선택 | 파일 `licensee=` · 대장 |
| `--tier free|pro|org` **또는** `--features a,b,c` | ✅(택1) | tier = 프리셋 → 낱개 features로 풀어 기록(23 §2-1 "등급이 아니라 낱개") · 프리셋 표 = §13-3(D-41 확정 뒤 고정) |
| `--seats N` · `--seat-mode named|concurrent` | Team/Org | §4-2 |
| `--updates-until YYYY-MM-DD` | 선택 | 기본 = 발급일 + 1년(D-25 영구+업데이트) |
| `--expires YYYY-MM-DD` | 체험/Org 구독만 | 비우면 영구 |
| `--max-major N` | 선택 | 비우면 제한 없음 |
| `--id` | 선택 | 기본 자동 `NSL-<연도>-<6자리 순번>`(대장에서 채번) |
| `--key <봉투 파일>` · `--key-pass-env NAME` | ✅ | 비밀키 봉투(§12-6) · 암호는 환경 변수/프롬프트(인자 금지) |
| `--out <폴더|파일>` | 선택 | 기본 `./issued/<id>/nexa-sql.license` |
| `--ledger <tsv>` | 선택 | 기본 `./issued/ledger.tsv` |
| `--note` | 선택 | 대장 비고(주문 번호 등) |

다른 명령: `keygen --out <봉투>`(루트 키쌍 생성 · 공개키 `.pub` 출력 → 라이브러리 `keys.rs`에 박는다) · `verify <파일>`(발급기에서 자기 검증 · 앱과 같은 코드) · `decode-request <코드>`(메타 보기) · `reissue --id … --add-request …`(기기 추가 · 대장에 같은 id 새 판) · `ledger list|find`.

### 12-4. 출력값 정의

| 산출 | 내용 |
|---|---|
| `nexa-sql.license` | 23 §2-1 형식 그대로(`format=nsl1` · `product` · `id` · `licensee` · `kind` · `tier`(표시용) · `features` 낱개 · `machine`/`machine.N` · `seats`/`seat_mode` · `issued` · `updates_until` · `expires` · `max_major` · `sig`). 텍스트 · LF · UTF-8 · 이메일 본문에 붙여도 되게 짧게 |
| `ledger.tsv` 1행 | `id · issued · kind · licensee · email · features · machines(base32 접두 8자) · updates_until · expires · note · request 메타(os/app)` — **기기 ID 원문은 대장에도 접두만** |
| stdout | 요약 표 1개 + 파일 경로 · 종료 코드 0 / 2(인자) / 3(요청 코드 무효) / 4(키 봉투 실패) |
| (선택) `mail.txt` | 고객 전달용 본문 템플릿(적용 방법 3줄 + 파일 첨부 안내) |

### 12-5. 전달·적용

- 전달 = **이메일 첨부**(1차 · D-29 수동) · 파일이 곧 라이선스라 별도 활성화 서버 없음 · 포털은 2차.
- 적용 = `nsql license install <파일>`(CLI) 또는 GUI 설정 ▸ 라이선스 ▸ 파일 열기/드롭(23 §1-3) → `<설정 폴더>/license/nexa-sql.license` 원자적 복사 · `nsql license status`로 확인. 기기 추가 = 새 요청 코드 → `reissue` → 같은 파일 교체.
- 회수(오프라인이라 불가)는 대장 표시 + `max_major`/`updates_until`로 자연 소멸 — 정직한 한계(23 §2-3).

### 12-6. 비밀키 봉투(D-27 구체화 · 확정 필요)

| 안 | 내용 | 비고 |
|---|---|---|
| ⓐ 암호 기반 봉투(권장 · 3-OS 동일) | `sha2` 기반 KDF(PBKDF2-HMAC-SHA256 · 반복 60만) → 키 → XOR-스트림+HMAC 봉투(외부 crate 0) · 암호는 발급자 머릿속 + 종이 백업 | OS 무관 · 발급 PC가 어느 OS든 같음 |
| ⓑ OS 키체인(Windows DPAPI · macOS Keychain) | `nsql-vault`식 기기 봉투 | 그 PC에 묶임 · 백업은 ⓐ로 |
| 백업 | 봉투 파일 + 암호를 **오프라인 2곳** | 키 분실 = 재발급 불가 → V2 회전 |

### 12-7. 확정 필요 항목(순서대로 답하면 일괄 개발)

| # | 항목 | 권장 |
|---|---|---|
| **L-1** | 발급 PC OS와 봉투 방식(§12-6 ⓐ/ⓑ) | ⓐ(3-OS 동일) |
| **L-2** | tier 프리셋 = §11-3 게이트 표(D-41)로 고정 | D-41 확정과 동시 |
| **L-3** | ID 체계(`NSL-2026-000001`) · 대장 위치(발급 PC 로컬 · 비공개 저장소엔 **안** 올림) | 권장안 |
| **L-4** | 기본 기간 — updates 1년 · 체험 14일(D-28) · Org 구독 1년(§2) | 권장안 |
| **L-5** | 전달 채널 = 이메일 수동 + `mail.txt` 템플릿(한/영) | 권장안 |
| **L-6** | 재발급 정책(D-35 연 5회 · 기기 교체 시 옛 기기 제거) | 권장안 |
| **L-7** | `nexa-license-server` 저장소 생성 시점(지금 · 비공개) 및 이 기기 clone | 지금 |

### 12-8. 개발 범위(확정 뒤 일괄 · T-241)

1. `nexa-license`: `sign`(issuer feature · dalek 2.x) · `request` encode/decode · `keys.rs`(ROOT_V1 자리) · `verify`(D-40 뒤 · 앱과 발급기가 같은 코드) · 테스트 픽스처 키쌍.
2. `nexa-license-tool`: `keygen` · `issue` · `reissue` · `verify` · `decode-request` · `ledger` · 봉투(§12-6) · 시험 = 23 §5 전부(발급→설치→check · 변조 · 다른 기기 · CRLF · 동시 설치).
3. nexa-sql `nsql-license` 얇은 층(T-32'): `nsql license request|install|status|remove` · GUI 라이선스 탭 · `check(Feature)` 게이트 위치(23 §4-2) · 배지.

## 13. 무료 · 유료 · 기업의 차별점(제안 · **확정용** · 사용자 09-27 "기본 기능/성능은 무료 · 편의 기능을 조금 불편하게 · 기업은 무조건 구매 · 기업 특화 기능")

### 13-1. 원칙 셋(사용자 지시를 규칙으로)

1. **무료 = 제품의 정체성 전부**: 접속 4종 · 실행 · 결과 그리드(편집 포함) · 스크립트 엔진·변수 · CLI(run/export/import) · 편집기 전부 · 탐색기 · **성능 상한 없음**(행 수·크기·속도 제한 금지 — "성능으로 차별하지 않는다").
2. **유료(Pro) = 없어도 되지만 없으면 조금 불편한 편의** — 잠긴 기능마다 **대체 경로가 존재**해야 한다(표의 "대체" 열이 비면 게이트 후보에서 뺀다). 잠긴 자리는 사라지지 않고 **"Pro · 대체: …" 안내**로 남는다(원성 최소).
3. **기업 = 기능이 아니라 사용 조건으로 구매**: PolyForm NC(DR-2)가 이미 상업 사용 = 유료를 정한다. 앱은 이를 **감지하지 않는다**(전화홈 없음 · 폐쇄망 우선) — 대신 ⓐ 무료 상태 배지 "Non-commercial use only" ⓑ 기업이 실제로 필요로 하는 **운영 기능을 Org 티어에만** 두어 구매 유인을 만든다(§13-4).

### 13-2. 경쟁 도구의 무료/유료 경계(2025~26 · 공식 문서 기준 · ⚠ = 2차 출처)와 사용자 반응

| 도구 | 무료 | 유료가 여는 것 | 반응(요지) |
|---|---|---|---|
| TablePlus | 전 기능 · **탭 2 · 창 2 · 필터 2 상한** | 상한 해제 · Team | "불편하지만 쓸 만하다" — 상한식 차별의 대표 · 원성 적음 |
| DBeaver | CE 전부(오픈소스) | PRO: NoSQL·클라우드 드라이버 · AI · ER/시각 쿼리 빌더 · git · 디버거 · **Team Edition(중앙 접속 공유·SSO/LDAP)** | 개인은 CE로 충분 → PRO는 사실상 **기업 기능**으로 판다 |
| DataGrip | 비상업 무료(2025-10) | 상업 사용 자체 | "일할 때 사면 된다" — 사용 조건 기반의 선례(우리와 같은 방향) |
| DbVisualizer | Free(기본 SQL·탐색) | Pro: 시각 쿼리 빌더 · ER · xlsx 등 고급 export/import · 모니터 · 차트 · 결과 편집 확장 | 개인 개발자 구매율 높음 · "Free로 시작해 Pro로" |
| Beekeeper | Community | Ultimate: Oracle/Cassandra 등 유료 드라이버 · JSON 편집 · 매직 · 플러그인 · 워크스페이스 공유 | 드라이버 잠금엔 원성 · 편의 잠금은 수용 |
| Navicat | Lite(비상업) | Premium/Enterprise 전부 | 좌석·deactivate 원성 |
| HeidiSQL · SQL Developer · SSMS | 전부 무료 | — | 비교표에서 "왜 돈 내나"의 기준선 |

읽히는 것: **드라이버·접속·실행을 잠그면 원성**, **상한식(TablePlus)과 편의 잠금(DbVisualizer)은 수용**, **기업 기능(SSO·중앙 배포·감사)은 기업만 산다**(DBeaver Team).

### 13-3. 제안 — 게이트 표(23 §4-3 권장안을 이번 원칙으로 재편 · D-23 정정안)

| 기능(이미 있는 것 기준) | 무료 | Pro | Org 전용 | 대체 경로(무료) |
|---|:--:|:--:|:--:|---|
| 접속·실행·그리드·편집·CLI·스크립트·변수·탐색기·편집기·북마크(기본)·검색·프로젝트(기본) | ✅ | | | — |
| **동시 DBMS 연결 수**(공유 `session.max_shared` 지금 8 · 전용 `session.max_private` 8) | 공유 **2** · 전용 2 | 8/8(상한 32/64) | | 접속 해제 뒤 다른 서버 · 인스턴스 2개 |
| **편집 탭 수**(지금 상한 키 없음 → 새 키 `editor.max_tabs`) | **5** | 무제한 | | 탭 닫기 · 프로젝트로 나눔 |
| **편집 용량**(큰 파일 모드 59 §5 · `file.large_ask_mb` 50 · L2 = 평문·읽기 전용) | **2 MB 이상 = 읽기 전용**(L2 강제 · 실행은 가능) | 기본값(50 MB 물음) | | `nsql run` 파일 실행(무료) · 파일 나누기 |
| **결과 다중 탭 상한** `grid.result_tabs_max`(지금 8) | **2** | 8~64 | | 결과를 파일로 저장 · 재실행 |
| **동시 창/인스턴스**(세션 창·보조 창은 무료) | 창 1 | 다중 | | 인스턴스 2개 실행 |
| xlsx/parquet export(T-8) | | ✅ | | csv → Excel |
| **GUI Import 창**(대량 적재) | | ✅ | | `nsql import` CLI(무료) |
| SSH 터널(T-3) | | ✅ | | `ssh -L` |
| 드라이버 **확장 관리자**(설치·SxS) | | ✅ | | 수동 설치(폴더에 복사) |
| 스키마/데이터 비교(19) | | ✅ | | 다른 도구 |
| 프로젝트 **작업 환경 자동 복원·미저장 탭 복구**(70) | | ✅ | | 수동 저장 |
| 북마크 **그룹·니모닉·CLI 공유** | | ✅ | | 기본 북마크(토글·이동)는 무료 |
| 코드 완성 **문법 참조(.sqlg)·메타 카드**(82 · 76) | 키워드·객체 이름 완성은 무료 | 절별 완성·상세 카드 | | 기본 완성 |
| LOB 값 창 **이미지 미리보기·파일 넣기** | 텍스트/HEX | ✅ | | 파일로 저장 뒤 보기 |
| 서식 있는 복사(HTML) | | ✅ | | 텍스트 복사 |
| 실행 카드·트랜잭션 로그 **보존·내보내기** | 세션 내 | ✅ | | 로그 창 복사 |
| **감사 로그 내보내기**(누가·언제·어떤 문장 · 44 QM 확장) | | | ✅ | — |
| **중앙 접속 프로필 배포**(읽기 전용 프로필 묶음 · 21 §7) | | | ✅ | — |
| **PROD 정책 강제**(`tx.prod_*`·`run.prod_confirm`를 조직 설정으로 잠금 · 금지 문장 목록) | | | ✅ | — |
| **좌석·리스 서버**(§4 · SSO/AD 로그인명 좌석 · floating) | | | ✅ | — |
| **폐쇄망 설치 번들**(Oracle IC 포함 오프라인 설치본 · 64) · LTS 고정 | | | ✅ | — |
| 우선 지원(SLA) | | | ✅ | — |

- 성능 항목(행 수 상한 `grid.max_rows` · 페치 속도 · 향상 모드 · 메모리)은 **절대 게이트하지 않는다**. 상한식 셋(연결 수 · 편집 탭 수 · 편집 용량)은 "성능"이 아니라 **작업 규모의 편의**로 본다 — 초과해도 **막지 않고 읽기 전용/닫기 안내**로 넘어가며(TablePlus 방식), 실행·조회는 그대로다. 편집 용량 상한은 사용자 지시("이상은 읽기 전용")대로 큰 파일 모드 L2를 낮은 문턱에서 켜는 것이라 새 코드가 거의 없다(`file.large_*`에 라이선스 층이 기본값을 덮어씀).
- `features=`는 낱개 문자열(23 §2-1)이라 이 표는 **파일 형식 변경 없이** 조정 가능 — 출시 뒤 반응으로 옮길 수 있다.

### 13-4. 기업 특화 기능 — 시장에서 실제로 구매를 결정하는 것(추천 순)

1. **감사·추적**: 실행 이력 내보내기(사용자·시각·세션·문장·행 수) · 금지 문장/PROD 확인 정책 강제 — 금융·제조 DBA의 1순위(DBeaver Team·Toad DBA 수요).
2. **중앙 접속 관리**: 접속 프로필을 관리자가 배포(비밀번호 없이 · 21 §7 볼트와 결합) · 회수 — DBeaver Team Edition의 핵심 판매 포인트.
3. **신원·좌석**: OS 로그인명/AD 기반 좌석(D-36) · floating(§4-2) · 관리자 페이지 — JetBrains License Vault 급 기대치.
4. **폐쇄망**: 오프라인 라이선스(이미) + Oracle Instant Client 동봉 설치본 + 판 고정(LTS) — 폐쇄망 DBA가 1순위 고객(§1-1 ⑥).
5. (후보) 데이터 마스킹 뷰 · 팀 쿼리 공유(git · 19) · 사설 LLM 엔드포인트 AI — 반응을 보고.

### 13-5. 미등록판의 등록 안내 + 광고 슬롯(**설계만** · 사용자 09-27 "광고를 넣고 싶진 않지만 기술 구조는 · 테스트 전용 · 실사용자 노출 없음")

**안내(nudge)** — 미등록(Free) 상태에서 ⓐ 기동 직후 1회 ⓑ 그 뒤 **12시간마다 1회**(`license.nudge_hours` 기본 12 · 0 = 끔은 Pro만) **모달이 아닌 카드**(실행 카드 스택 자리 · 닫기 = Esc/×)로 "등록하면 좋은 점" 소개. 내용 = §13-3에서 그 사용자가 **실제로 부딪힌 상한/잠금**을 위에(예: 오늘 탭 5 상한에 닿았으면 그것부터) → 개인화된 이유 · 버튼 = [요청 코드 복사] [라이선스 파일 열기] [나중에]. 원칙: 작업을 막지 않음 · 입력 포커스를 빼앗지 않음(61 §2) · 노출 기록은 로컬(`NSQL_HOME/nudge.json` · 마지막 시각·횟수)만 · 전화홈 0.

**광고 슬롯(테스트 전용)** — 30 §1-2 확장점 규칙대로 **포트 + 레지스트리 + 설정 선택** 세 조각:

| 조각 | 내용 |
|---|---|
| 포트 `PromoSource` | `fn next(&self, ctx: &NudgeCtx) -> Option<Creative>`(제목 · 본문 · 이미지(nexa-gfx 디코더 · 로컬 파일) · 링크 텍스트 · 만료) — 앱은 이 포트만 안다 |
| 구현 | `NonePromo`(기본 · 항상 None) · `LocalPromo`(번들 시험 소재 `tests/fixtures/promo/*.toml`) · `RemotePromo`는 **만들지 않는다**(네트워크 0 · 26 §8) |
| 선택 | HIDDEN 설정 `promo.provider = none|local` — **Release 빌드는 `none` 이외를 거부**(feature `promo-test`로 `LocalPromo` 자체를 컴파일에서 제외 → 배포 바이너리에 소재·코드가 없음) |
| 무효화 | `Licensing` 상태가 `Licensed`가 되는 순간 nudge 타이머·PromoSource 모두 해제(설정·기동 시 재판정) · 카드가 떠 있으면 즉시 닫음 |
| 카드 안 표시 | 소재는 카드의 **하단 슬롯** 한 곳 · "등록하면 표시되지 않습니다" 문구 고정 · 클릭 = 외부 링크(OS 브라우저) — 테스트에서는 링크를 열지 않고 로그만 |
| 시험 | 자체 시험 기동 명령 `nudge.show` · `nudge.dump:<파일>`(노출 시각·소재 id·무효화 여부) · 12시간 규칙은 `NSQL_NUDGE_CLOCK` 가짜 시계로 · Release에서 `promo.provider=local` = 거부 로그 |

정직한 한계: 오프라인 앱이라 노출 측정·과금은 없다(테스트 목적이므로 문제없음) · 실제 노출을 하려면 26 §8 네트워크 규칙과 개인정보 검토가 먼저다.

### 13-6. 확정 필요(D-41~D-47)

| # | 결정 | 권장 |
|---|---|---|
| **D-41** | 게이트 표 §13-3 확정(항목 가감) — 특히 상한식(연결 2/2 · 탭 5 · 2 MB 읽기 전용 · 결과 탭 2 · 창 1) 도입 여부 | 도입(TablePlus식 · 원성 적음) |
| **D-42** | Org 전용 기능 우선순위 — 감사 로그 → 중앙 프로필 → 정책 강제 → 좌석 서버 | 이 순서 |
| **D-43** | 잠긴 기능의 UX — 메뉴에 남기고 "Pro · 대체: …" 안내(권장) vs 숨김 | 남김 |
| **D-44** | 무료 배지 문구 "Non-commercial use only" 상시 표시 · 기업 감지는 하지 않음(전화홈 0) 확인 | 상시 · 감지 없음 |
| **D-45** | 체험(D-28 14일)이 Pro만인지 Org 기능까지인지 | Pro만 · Org는 평가 라이선스 별도 발급 |
| **D-46** | 상한식 수치 — 동시 연결 2/2 · 편집 탭 5 · 편집 용량 2 MB · 결과 탭 2 · 창 1 | 권장안 · 출시 뒤 조정 가능(features 낱개) |
| **D-48**(사용자 09-27 확정) | **호환 원칙** — 발급된 파일은 불변 · `features=*`(초기 버전 전 기능)는 이후 추가된 기능도 포함 · 앱이 모르는 키는 무시 · 영역/기능 구분이 체계화되면 **새 발급분부터** 낱개 features · 큰 개편 = `format=nxl2` + 두 판 동시 검증 + 대장 기반 **무상/유상 재발급 프로세스** | 확정 · §12 `reissue`가 그 통로 |
| **D-47** | 등록 안내 카드 주기 12 h · 광고 슬롯은 **테스트 전용**(Release = `none` 고정 · `LocalPromo` 컴파일 제외) 확인 | 권장안 |
