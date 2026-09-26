# 23 · 정품 인증(로컬 라이선스 파일) · 기능 게이트 설계

> **요청**(사용자 09-14): *"로컬(PC에 대한 인증) 인증 정보를 전송하고 라이선스 파일을 받아 적용하는 방식으로 정식 버전임을 증명하고, 정식 버전인 경우만 사용할 수 있는 기능을 설정"*. 절차·기술·적용 기준·기능 판정 방법을 설계하고 사용자 결정이 필요한 항목을 **D-23~D-31**로 뽑는다.
> **선행**: [13 라이선스](13-licensing.md) §3(집행 — nexa-dir 17 재사용 방침 · Ed25519 · 오프라인 1차) · DR-2(PolyForm NC) · DR-22(`nsql-vault` 기기 키 · DPAPI) · nexa-beep `ed25519-dalek` 원장 선례(09-06). `../nexa-dir` 원본은 이 기기에 없어 **원칙만 계승**하고 형식은 이 문서가 SSOT.
> **상태**: 📐 설계. 코드는 D-23·D-24·D-25 답 뒤 T-32~T-35.
> **후속(09-14 2차)**: 라이선스 종류 4단(Device·User 5대·Team·Organization) · 사내 인증 서버 · **공유 라이브러리 `nexa-license`(형제 저장소) + 서버 비공개 저장소 분리** = [25](25-license-tiers-and-server.md). 이 문서의 `nsql-license`는 25 §9-1의 얇은 앱 층이 된다.

---

## 0. 한 장 요약

```
[사용자 PC]                                  [SosomLab · 오프라인 발급기]
 1. nsql license request  ──(요청 코드 · 텍스트)──▶  2. 주문·결제 확인
    machine=XXXX-XXXX-…                                   nsql-license-tool issue
    (OS 기기 ID의 해시 · 하드웨어 원문 없음)               --machine XXXX… --tier pro --updates-until 2027-09-14
                                                          ⇒ nexa-sql.license (Ed25519 서명)
 4. nsql license install nexa-sql.license ◀──(파일)──  3. 이메일/다운로드로 전달
    서명 ✓ · 기기 일치 ✓ · 기한 ✓ → <설정 폴더>/license/
 5. 실행 때마다 로컬 검증만(네트워크 0) → LicenseState
 6. 기능 진입점에서 license.check(Feature::X) → Allowed / Denied(사유)
```

- **네트워크 없이 완결**된다(폐쇄망 사내 DBA가 1순위 고객 — 온라인 인증은 2차, [13 §3](13-licensing.md)).
- **앱에는 공개키 32B만** 들어간다. 비밀키는 저장소·CI·앱 어디에도 없다.
- 라이선스 파일은 **PC(기기 ID) 1대에 묶인다**. 파일을 다른 PC로 복사하면 기기 불일치로 거부.
- 판정은 `nsql-license` 크레이트 한 곳 — UI·CLI는 `Feature` 열거형으로 묻기만 한다. `nsql-core`는 라이선스를 모른다(의존 0 유지).

---

## 1. 절차

### 1-1. 요청 코드 만들기(사용자 PC · 오프라인)

| 단계 | GUI | CLI |
|---|---|---|
| 요청 코드 표시 | 설정 → **라이선스** 탭 → "요청 코드 복사" | `nsql license request` → stdout |
| 내용 | `NSQLREQ1.<base32(기기ID 20B)>.<base32(메타)>` 한 줄 | 같음 (`--file` 옵션으로 `.request` 파일 저장) |
| 메타 | 앱 버전 · OS 3자 · 요청 시각(일) · 사용자가 입력한 이름/이메일(선택) | 같음 |

**기기 ID(20B)** = `SHA-256("nexa-sql/license/machine-v1" ‖ OS 기기 식별자)[..20]`

| OS | 원천 | 비고 |
|---|---|---|
| Windows | `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` | OS 설치 시 생성 · 앱 재설치·설정 폴더 삭제에도 불변 · 관리자 권한 불요(읽기) |
| macOS | `IOPlatformUUID`(IOKit `IOPlatformExpertDevice`) | 하드웨어에 고정 · `ioreg -rd1 -c IOPlatformExpertDevice` 실행 결과 파싱(외부 crate 0) |
| Linux | `/etc/machine-id`(없으면 `/var/lib/dbus/machine-id`) | 배포판 공통 · 컨테이너는 이미지마다 다를 수 있음(정직한 한계) |

- **원문은 절대 전송하지 않는다** — 해시 한 겹 + 도메인 태그. 요청 코드로 하드웨어 원문을 되짚을 수 없다.
- nexa-beep P-3(*"하드웨어 식별자를 신원으로 쓰지 않는다"*)와 다른 선택인 이유: beep의 신원은 **사람 간 신뢰**라 위조·추적이 문제였고, 여기서는 **"이 PC인가"** 자체가 목적이며 값이 앱 밖으로 나가지 않는다. 다만 D-24에서 대안(`device.key` 무작위 키)과 비교한다.

### 1-2. 발급(SosomLab · 비밀키 보유 기기)

- 발급기 = **`nsql-license-tool`**(비공개 저장소 · 같은 `nsql-license` 크레이트의 `issue` 기능을 `issuer` feature로 켠 CLI). 비밀키는 발급 PC의 `nsql-vault`식 봉투(DPAPI)로 보관 · 오프라인 백업 1부(D-27).
- 입력: 요청 코드 · 구매자 이름/조직 · 등급(tier) · 기기 수 · 업데이트 기한. 출력: `nexa-sql.license` 텍스트 파일.
- 발급 대장(`issued.tsv` · 라이선스 ID · 기기 ID · 구매자 · 일자)은 발급기 로컬에만.

### 1-3. 적용(사용자 PC)

| 경로 | 동작 |
|---|---|
| GUI 설정 → 라이선스 → "라이선스 파일 열기…" · 파일을 창에 드롭 | 검증 → `<설정 폴더>/nexa-sql/license/nexa-sql.license`로 **원자적 복사**(nexa-conf `write_atomic`) → 상태 표시 |
| `nsql license install <파일>` | 같음 · 실패 시 사유와 종료 코드 3 |
| `nsql license status` | 등급 · 구매자 · 기기 일치 여부 · 업데이트 기한 · 파일 경로 |
| `nsql license remove` | 파일 삭제(확인 프롬프트) → 무료 상태 |

설정 폴더는 DR-22와 같은 `user_config_dir("nexa-sql")`(`NSQL_HOME` 우선) — **CLI·GUI·다중 인스턴스가 같은 파일**을 본다.

### 1-4. 실행 시 검증(매번 · 로컬)

```
load()  ──▶ 파일 없음 ────────────────────────▶ Free
        ──▶ 파싱·서명 실패 ────────────────────▶ Invalid(Signature)   ← Free로 취급 + 상태줄 경고
        ──▶ product ≠ "nexa-sql" ──────────────▶ Invalid(Product)
        ──▶ machine ≠ 이 PC ───────────────────▶ Invalid(Machine)     ← "다른 PC의 라이선스"
        ──▶ 빌드일 > updates_until ────────────▶ Outdated             ← 영구 모델: 기한 전 판만 정식(§3)
        ──▶ 만료형이고 now > expires ──────────▶ Expired
        ──▶ 그 외 ─────────────────────────────▶ Licensed{tier, features, licensee, until}
```

- 시각 원천 = **빌드에 박은 날짜**(`NSQL_BUILD_DATE` · `build.rs`) 우선, 시스템 시계는 만료형(체험판)에만. 영구 모델은 시계를 되돌려도 이득이 없다(빌드일이 기준).
- 캐시: 프로세스 시작 시 1회 + 파일 mtime 변경 시 재로드. 기능 호출마다 파일을 읽지 않는다.

---

## 2. 기술

| 요소 | 선택 | 근거 |
|---|---|---|
| 서명 | **Ed25519**(`ed25519-dalek` 2.x · `verify` 기본, `issuer` feature에서만 서명) | 제3자(앱)가 검증하고 비밀은 발급기에만 — 대칭 MAC 불가. nexa-beep 09-06 원장과 같은 판 · BSD-3 |
| 해시 | `sha2`(이미 `nsql-vault` 원장) | 기기 ID · PAE 없이 직접 서명 대상 구성 |
| 인코딩 | base32(Crockford · 자체 60줄) · 텍스트 봉투 | 사람이 복사·이메일로 옮기는 값(요청 코드)에 base64는 `+/=` 오타·줄바꿈 사고가 잦다. 외부 crate 0 |
| 파일 형식 | key=value 본문 + 서명 줄(§2-1) — nexa-conf 직렬화 재사용 | [13 §3](13-licensing.md)의 PASETO v4.public은 JSON 본문 전제 → 워크스페이스에 JSON 파서가 없다(core 의존 0). **서명 대상 구성(PAE)만 PASETO에서 차용**, 본문은 conf. D-26 |
| 저장 위치 | `<설정 폴더>/license/` | DR-22 폴더 · 다중 인스턴스 공유 |
| 기기 ID | OS 기기 식별자 해시(§1-1) | D-24 |
| 키 보관(발급기) | `nsql-vault` 봉투 + DPAPI · 오프라인 백업 | D-27 |

### 2-1. 라이선스 파일 형식 `nexa-sql.license`

```
# Nexa SQL license — do not edit
format=nsl1
product=nexa-sql
id=NSL-2026-000123
licensee=ACME Corp (kiros33@example.com)
tier=pro
features=export-xlsx,ssh-tunnel,driver-ext,compare
machine=6QJ3K-PN0X4-…(base32 20B · 다중 기기는 machine.2= …)
issued=2026-09-14
updates_until=2027-09-14
expires=                       ← 비우면 영구(체험판만 채움)
max_major=                     ← 비우면 제한 없음
sig=<base32 64B>               ← Ed25519( "nexa-sql/license/v1" ‖ 위 줄들(sig 제외 · 정렬 · LF) )
```

- 서명 대상 = 도메인 태그 ‖ `sig` 제외 전 줄을 **키 정렬 · `\n` 결합**한 바이트. 줄 순서·공백 차이로 검증이 깨지지 않게 정규화한다.
- `features`는 **등급이 아니라 낱개**로 적는다 — 등급이 바뀌어도 이미 발급한 파일의 의미가 변하지 않고, 고객별 특별 허용이 가능하다. 앱은 `tier`를 표시용, `features`를 판정용으로 쓴다.
- 앱이 모르는 키는 무시(전방 호환) · `format` 다르면 거부.

### 2-2. 요청 코드 형식

```
NSQLREQ1.6QJ3KPN0X4…(base32 20B).<base32(kv: os=win;app=0.0.1;d=2026-09-14;n=홍길동)>
```
사용자가 복사해 이메일 본문에 붙이는 한 줄. 발급기는 뒤 절만 참고(표시용) · 서명 대상은 기기 ID.

### 2-3. 신뢰 경계 — 정직하게

- 단일 네이티브 바이너리라 **공개키 교체·분기 패치는 막을 수 없다**(모든 로컬 인증의 공통 한계). 목표는 "정직한 고객이 쉽게 · 우회는 귀찮게"이지 DRM이 아니다. 난독화·안티디버깅은 **하지 않는다**(DR-1 단순성 · 사용자 신뢰).
- 파일 복사 우회 = 기기 ID로 차단. 기기 ID 위조 = OS 값 변경이므로 일반 사용자는 못 한다.
- 온라인 재검증·전화홈은 1차 범위 밖(폐쇄망 우선). 2차(Cloudflare Workers + D1 · [13 §3](13-licensing.md))는 **선택적** 온라인 발급 자동화이지 강제 확인이 아니다(D-29).

---

## 3. 적용 기준(라이선스 모델)

권장 = **영구 라이선스 + 업데이트 1년**(TablePlus $99 · DbVisualizer $199 선례 · [03 §1.3](03-competitive-landscape.md)):

| 항목 | 값(권장) | 판정 |
|---|---|---|
| 사용 기한 | 없음(영구) | `expires` 비움 |
| 업데이트 기한 | 발급 + 1년 | `빌드일 ≤ updates_until`인 판만 정식. 이후 판을 실행하면 `Outdated` → 무료 상태 + "갱신 필요" 안내(기존 판은 계속 정식) |
| 기기 | 1대(Basic) · 2대(Standard) | `machine`/`machine.2` |
| 등급 | `pro`(개인·상업) · `team`(N석 · 기기 N개 파일 N개 발급) | `tier` 표시 · `features` 판정 |
| 체험 | 14일 체험 파일(D-28) | `expires` 채움 · 시스템 시계 + 최초 실행일 기록으로 되돌림 완화 |

무료(비상업) 사용자는 파일 없이 **Free** — PolyForm NC가 허용하는 범위. 앱은 무료 사용자를 막지 않고 **상업 전용 기능만** 잠근다(§4).

---

## 4. 기능 게이트 — 판정 방법

### 4-1. 크레이트 `nsql-license`(의존: `ed25519-dalek` · `sha2` · `nexa-conf`)

```rust
pub enum Feature { ExportXlsx, SshTunnel, DriverExtensions, MultiConnection, SchemaCompare, /* … */ }
pub enum Tier { Free, Trial, Pro, Team }
pub enum LicenseState { Free, Licensed(License), Invalid(Reason), Outdated(License), Expired(License) }
pub struct License { pub id: String, pub licensee: String, pub tier: Tier, pub features: BTreeSet<Feature>, pub updates_until: Date, /* … */ }
pub enum Entitlement { Allowed, Denied(Denial) }          // Denial = { feature, reason, hint }

pub struct Licensing { state: LicenseState, path: PathBuf, mtime: Option<SystemTime> }
impl Licensing {
    pub fn open_default() -> Licensing;                     // 파일 없음 = Free (오류 아님)
    pub fn refresh(&mut self) -> bool;                      // mtime 바뀌면 재로드 · 변경 여부 반환
    pub fn state(&self) -> &LicenseState;
    pub fn check(&self, f: Feature) -> Entitlement;         // ★ 유일한 판정 함수
    pub fn install(&mut self, file: &Path) -> Result<License, InstallError>;
    pub fn remove(&mut self) -> io::Result<()>;
    pub fn machine_code() -> Result<String, MachineIdError>;
    pub fn request_code(meta: &RequestMeta) -> Result<String, MachineIdError>;
}
```

- **`Feature`는 코드에 열거**(문자열 아님). 새 유료 기능 = 열거형 1줄 + `features` 문자열 매핑 1줄 + 게이트 1곳. 파일의 모르는 feature 이름은 무시.
- `check`는 **순수 함수**(I/O 없음). 상태 갱신은 `refresh`가 명시적으로.
- `Free`·`Invalid`·`Outdated`·`Expired`는 판정상 전부 `features = ∅`. 차이는 **안내 문구**뿐.

### 4-2. 게이트 위치 규칙(4계층 · DR-7)

| 계층 | 규칙 |
|---|---|
| Core · Network · 어댑터 | **라이선스를 모른다.** 게이트 없음 — 테스트·CLI 자동화가 라이선스에 얽매이지 않는다 |
| Run(`nsql-run`) | 모른다 |
| **UI(GUI)** | **행위 진입점 1곳**(메뉴·버튼·단축키 핸들러) — `match lic.check(F) { Allowed => 실행, Denied(d) => 안내 대화상자(요청 코드 복사 버튼 포함) }`. 메뉴 항목은 **숨기지 않고** `Pro` 배지 표시(기능 존재를 알려야 구매가 생긴다) |
| **CLI(`nsql-cli`)** | 서브커맨드·옵션 파싱 직후 1곳 — `Denied` → stderr 안내 + **종료 코드 4**(스크립트가 구분) |
| 드라이버 확장([22](22-driver-extensions.md)) | 확장 manifest `requires=pro` → 설치·기동 시 게이트(확장 자체는 모른다) |

게이트는 **기능당 정확히 한 곳**. 깊은 곳에서 두 번 검사하지 않는다(우회 탐지 목적의 중복 검사는 DRM이며 하지 않는다).

### 4-3. 게이트 대상(권장안 — D-23에서 확정)

| 후보 | 게이트 | 이유 |
|---|:--:|---|
| 접속·실행·그리드·export csv/tsv/json/insert · 스크립트 엔진 · 연결 프로필 · 편집기 전부 | **무료** | 제품의 정체성. 막으면 비교표 탈락([03 §5](03-competitive-landscape.md)) |
| xlsx/parquet export(T-8) | Pro | 업무 산출물 = 회사 용도 신호 |
| SSH 터널(T-3) | Pro | 운영 접근 |
| 드라이버 확장 설치(T-28) · 벤더 클라이언트(OCI/ODBC) | Pro | 사내 레거시 DBMS = 상업 |
| 동시 다중 접속(창/탭 2개 이상) | Pro | TablePlus·Dataflare 선례 |
| 스키마/데이터 비교([19](19-compare-git-and-object-history.md)) | Pro | 팀 워크플로 |
| 오브젝트 시점 캐시·로컬 히스토리 | 무료(권장) | 데이터 안전 기능은 잠그면 원성 |

⚠️ [13 §3](13-licensing.md)은 *"무료 기능 제한을 두지 않는 방향 권장"* 이었다. 이번 요청으로 **"상업 전용 기능 게이트"로 정정**한다 — DR 표에 정정 항목으로 남긴다(16 §2-5).

### 4-4. GUI 상태 노출

- 상태줄 우측 `Free` / `Pro · ACME` / `⚠ 라이선스 무효(다른 PC)` 배지 → 클릭 = 설정 라이선스 탭.
- 설정 라이선스 탭: 요청 코드(복사) · 파일 열기 · 현재 상태 표 · 제거.
- i18n 키 `license.*`(영/한).

---

## 5. 테스트 기준

| 테스트 | 기대 |
|---|---|
| 발급 → 설치 → `check` | `Allowed`(features 내) · `Denied`(features 밖) |
| 서명 1비트 변조 · 키 순서 교란 · CRLF 저장 | 변조 = `Invalid(Signature)` · 순서/CRLF = **검증 통과**(정규화) |
| 다른 기기 ID | `Invalid(Machine)` |
| 빌드일 > `updates_until` | `Outdated` · features ∅ |
| 파일 없음 · 폴더 없음 | `Free`(오류 아님) |
| 모르는 키·모르는 feature | 무시하고 통과 |
| 동시 두 프로세스 설치 | 원자적 교체 · 둘 다 유효 파일을 본다 |
| 3-OS 기기 ID | 각 OS에서 20B · 두 번 호출 동일 · CI(윈·맥·리눅스) |

---

## 6. 사용자 결정 필요(D-23~D-31)

| # | 결정 | 권장 | 영향 |
|---|---|---|---|
| **D-23** | ★ **게이트 기능 목록**(§4-3 표 확정) · 무료 범위 | §4-3 권장안 | `Feature` 열거형 · 게이트 위치 |
| **D-24** | ★ **기기 묶음 원천** — ⓐ OS 기기 식별자 해시(권장) ⓑ `device.key` 무작위(설정 폴더 지우면 재발급 필요 · Windows만 복사 방지) ⓒ ⓐ+ⓑ 결합(가장 엄격 · 지원 비용↑) | ⓐ | 재설치·폴더 삭제 시 재발급 여부 |
| **D-25** | ★ **모델** — 영구+업데이트 1년(권장) vs 구독(만료형) · 기기 수(1/2) · 등급 이름 | 영구 · Basic 1대/Standard 2대 | `expires`/`updates_until` 의미 · 가격표 |
| **D-26** | 파일 형식 — 자체 key=value 봉투(권장 · JSON 의존 0) vs PASETO v4.public 엄수(13 §3 기존 문구) | 자체 봉투 | 13 §3 정정 |
| **D-27** | 발급 비밀키 보관 — 발급 PC DPAPI 봉투 + 오프라인 백업(권장) · 키 회전 정책(공개키 2개 동시 내장으로 무중단 회전) | 권장안 | `nsql-license-tool` 저장소 신설(비공개) |
| **D-28** | 체험판 — 14일 체험 파일 발급(요청 코드로 자동) vs 없음 | 있음(14일 · 기기당 1회는 발급기 대장으로) | 시계 되돌림 완화 로직 필요 |
| **D-29** | 온라인 2차(Cloudflare Workers 발급 자동화 · 이메일 자동 발송) 착수 시점 | v1.0 이후 · 수동 이메일로 시작 | 서버 비용·개인정보 |
| **D-30** | 집행 시점(기존 D-6) — v1.0 정식판부터 게이트 활성(권장) · 그 전 판은 배지만 | v1.0부터 | 릴리스 노트 |
| **D-31** | Team/사이트 라이선스 — 기기 묶음 없는 조직 키(도메인·좌석 수 자기 신고) 도입 여부 | v1.0에는 미포함 | 형식에 `machine` 생략 허용 |

---

## 7. 작업(TODO 등재)

| ID | 항목 | 의존 |
|---|---|---|
| **T-32** | `nsql-license` — 형식 파서·정규화·Ed25519 검증·`Feature`·`check` · 3-OS 기기 ID · 테스트 §5 — **09-27 앱 층 ✅**(라이브러리 몫은 `nexa-license` · 남음 = ROOT_KEYS) | D-24 D-26 |
| **T-33** | `nsql license request/status/install/remove` CLI · 종료 코드 4 — **09-27 ✅**(+`path` · 종료 코드 4는 게이트 입구 T-36) | T-32 |
| **T-34** | GUI 라이선스 창·상태줄 배지·Denied 안내 · i18n — **09-27 ✅**(`license_win.rs` · 배지 클릭/Help ▸ License… · 요청 코드 복사 · 파일 열기 · 제거 · Denied = 상태줄 + 창 안내) | T-32 · 설정 창 |
| **T-35** | `nexa-license-tool` — **09-27 ✅**(nexa-license 워크스페이스 · keygen/keys-rs/issue/reissue/verify/decode-request/ledger · 봉투 `nxk1`) | T-32 D-27 |
| **T-36** | 게이트 배선 — **09-27 ✅ 1차**(25 §13-3 표 12곳 · `lic_gate`/`entitled`/`cap` · Debug = `license.gates_dev` · 남음 = 확장 manifest `requires` · xlsx/SSH/비교는 기능 자체가 아직 없음) | T-32 D-23 |
