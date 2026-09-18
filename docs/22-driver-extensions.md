# 22 · 드라이버 확장(GitHub 최신 다운로드 · SxS 다중 버전 · 관리 UI) · DBeaver식 접속 대화상자

> 사용자 요구(09-13, 순차):
> ① *"다양한 데이터베이스를 지원하려면 DBeaver 형태(접속 설정 대화상자)가 맞다. Golden 화면(로그인 리스트)은 1번."*
> ② *"DBeaver는 드라이버를 필요할 때 추가 Download한다. 우리도 GitHub에서 다운로드해서 확장 사용하는 방식으로."*
> ③ *"최신 버전이 다운로드되도록."*
> ④ *"DBMS 버전별로 Client/드라이버가 다른(하위 호환) 버전이 필요한 경우가 있는지 확인하고, 필요하면 SxS처럼 여러 버전을 로컬에 보관 · 사용자가 목록을 보고 삭제도 할 수 있게 설계."*
>
> 결정 = [10 DR-23·DR-24](10-decision-record.md). **이 문서는 설계다 — 구현은 §7 순서(백로그 T-27~T-31)로 착수한다.** 기존 자산: [09 §4-3](09-editor-and-packages.md)(stdio JSON-RPC 드라이버 플러그인) · [06 §A](06-rust-ecosystem.md)(드라이버 지형) · [21](21-connection-profiles.md)(프로필 저장소 — 드라이버 버전 고정 필드가 여기 붙는다) · nexa-dir `docs/17`(Ed25519 서명 검증).

---

## 1. 접속 대화상자 — DBeaver 형태 + Golden 로그인 리스트

사용자 스크린샷 3장에서 채택할 것과 버릴 것:

| 요소 | 출처 | 채택 |
|---|---|---|
| **좌측 트리**(Connection settings › General · Metadata · Errors and timeouts · Data Transfer / Data Editor / SQL Editor) | DBeaver | ✅ 골격 — 1차는 `Connection settings`(Main · Advanced · Driver properties 탭)만 채우고 나머지는 자리만 |
| **Main 탭**: Connect by `Host` / `URL` · Host · Port · Database(+ Oracle: Service Name/SID 선택 · TNS · Custom 탭) · Authentication(Username/password 콤보 → Username · Password · **Save password** 체크) · Role(Oracle: Normal/SYSDBA/SYSOPER) · Local Client(Oracle 클라이언트 선택 콤보) | DBeaver | ✅ 그대로. **Save password = [21] 봉투 저장 여부**. Local Client 콤보 = §4 SxS 버전 선택 |
| **Driver name: Oracle [Driver Settings] [Driver license]** 하단 줄 | DBeaver | ✅ `Driver Settings` → §5 드라이버 관리자 · 미설치면 버튼이 **"Download …"** 로 바뀐다(DBeaver의 "Download driver files" 대화상자 대응) |
| **Test Connection …** / OK / Close | DBeaver | ✅ `nsql conn test`와 같은 경로(워커에서 접속 후 끊기 · 경과 시간 표시) |
| **Login List**(이름 · Pin · Username · Database · 마지막 사용) + New/Edit/Delete/Filter/Import·Export | Golden | ✅ 대화상자 **왼쪽 또는 첫 화면**에 프로필 목록으로. 이름 = [21] 프로필 이름 · Pin = 정렬 고정 · 더블클릭 = 접속 |
| Read Only 체크 | Golden | ✅ 프로필 필드 `read_only`(세션 `SET TRANSACTION READ ONLY`/`ApplicationIntent=ReadOnly`) — 2차 |
| SSH · 프록시 탭 | DBeaver | 📐 `nsql-net` 터널(T-3) 이후 |

**프로필 모델 확장**([21] `ConnectSpec` + 파일 키): `driver=<id>@<version>`(§4 · 없으면 최신 설치본) · `save_password`(봉투 유무로 표현 — 별도 키 불요) · `read_only` · `pin` · `last_used`(선택 · 목록 정렬용). 파일 형식은 그대로 key=value이므로 구버전이 읽어도 깨지지 않는다(미지 키 보존 — nexa-conf 규약).

**구현 위치**: `crates/nexa-sql/dialog/connect.rs`(호스트) — nexa-ctl `Combo`·`TextBox`·`Checkbox`·`Tree`·`Button`으로 그린다(컨트롤 17종 안에 다 있음 · 새 컨트롤 불요). 지금 최소 창의 [이름 칸 · 접속 칸 · Save]는 이 대화상자가 들어오면 **접속 칸 = 현재 프로필 표시 + `…` 버튼**으로 축소된다.

## 0. 09-15 개정(DR-29) — 전송은 프로세스가 아니라 **in-process 동적 라이브러리**

사용자 요구 *"로딩 시간·불필요한 메모리·속도 지연이 없는 구조"* · *"exe 분리가 아니라 동적 라이브러리/플러그인"*. 실측(release · 09-15): `nsql` 기동 ≈ 20ms(sqlite 실행 포함 24ms) · GUI 사설 메모리 8.5MB · 드라이버 전역 초기화 0(MSSQL tokio 런타임·Oracle ODPI-C는 **첫 접속 때** 만들어짐). 정적으로 링크된 드라이버는 OS 요구 페이징 덕에 **쓰지 않으면 RAM에 올라오지 않는다** → 내장 4종은 정적 유지가 최적(분리 = 비용만 추가).

| 방식 | 기동 | 메모리 | 호출 지연 | 격리 | 쓰임 |
|---|---|---|---|---|---|
| 정적 링크(현재) | 0 | 사용분만(페이징) | 0 | 없음 | 내장 Oracle·MSSQL·PG·SQLite |
| **cdylib(C ABI · dlopen 지연 로드)** | 첫 접속 시 ~1ms | 로드한 드라이버만 | 0(직접 호출) | 없음(패닉은 `catch_unwind`) | **확장 드라이버**(다운로드 · SxS · 무재설치 갱신) — §2의 "프로세스"를 이것으로 읽는다 |
| stdio JSON-RPC 프로세스 | 스폰 50~100ms | 별도 프로세스 | 왕복당 직렬화 | 완전 | 격리가 꼭 필요할 때만(같은 ABI를 프로세스 호스트가 감쌈 · 선택 모드) |

구현 순서(T-27 개정): `nsql-abi`(버전 있는 C vtable · `Value/ExecRequest/ExecResult` 바이너리 코덱 — RPC 프로세스에도 재사용) → 드라이버 크레이트 `crate-type = ["rlib","cdylib"]` + `export_driver!` → `nsql-drivers` 지연 로더(`<exe>/drivers/` · `NSQL_DRIVER_DIR` · 외부 crate 0: `LoadLibrary`/`dlopen` 직접 선언) → 다운로드·SxS(T-28·29)는 그대로.

## 2. 드라이버 = 확장 — GitHub Releases에서 내려받는 별도 프로세스(→ §0: 기본 전송은 in-process cdylib · 프로세스는 격리 선택 모드)

### 2-1. 왜 "다운로드"가 필요한가 (단일 바이너리 원칙 DR-1과의 조화)
- 본체는 **올 러스트 단일 바이너리**(DR-1). 순수 Rust 드라이버(SQLite · MSSQL tiberius · PG · MySQL · Oracle thin GA 후)는 **내장**(feature) — 다운로드 대상이 아니다.
- 다운로드가 필요한 것은 **본체에 넣을 수 없거나 넣기 싫은 것**: ⓐ 벤더 네이티브 클라이언트가 필요한 드라이버(Oracle ODPI-C + Instant Client · Tibero/Altibase/CUBRID ODBC) ⓑ 라이선스가 본체와 섞이면 안 되는 것(GPL · 벤더 EULA) ⓒ 크기가 큰 것 ⓓ 버전을 **여러 개 나란히** 두어야 하는 것(§4) ⓔ 서드파티가 만드는 드라이버(JDBC 래퍼 등).
- DBeaver는 Maven Central에서 jar를 받아 `%APPDATA%\DBeaverData\drivers\maven\`에 버전별로 둔다([참고](https://dbeaver.com/docs/dbeaver/Driver-Manager/)). 우리는 **GitHub Releases**가 그 자리다(사용자 지정).

### 2-2. 형태 = stdio JSON-RPC 프로세스 ([09 §4-3](09-editor-and-packages.md) 확정)
```text
nexa-sql / nsql ──spawn──▶ drivers/<id>/<ver>/nsql-driver-<id>(.exe)   ← 확장 1개 = 실행 파일 1개(+ 동봉 라이브러리)
          ◀── stdin/stdout JSON-RPC 2.0 (길이 접두 프레임) ──▶
메서드 = nsql_core::Session 포트 1:1 — connect · execute · fetch_cursor · commit · rollback · set_option · cancel · describe(카탈로그)
값 인코딩 = nsql_core::Value(JSON · BLOB은 base64 · 대용량 결과는 fetch_cursor 페이징)
```
- **왜 cdylib이 아닌가**: Rust ABI 불안정 · 크래시 격리 없음 · **한 프로세스에 Oracle 클라이언트 하나만**(§4-1) → 프로세스여야 SxS가 된다. 언어 무관(Java JDBC 래퍼도 확장이 된다). 라이선스 격리(GPL 드라이버도 본체 오염 없음).
- WASM(`wasmi` · DR-16)은 **소켓·네이티브 라이브러리 로드가 안 되므로 드라이버에는 부적합** — 코드 플러그인(TextCommand 등)에만 쓴다.
- 내장 드라이버와 확장 드라이버는 **같은 `Box<dyn Session>`** — `nsql-drivers` 레지스트리가 `Dialect`/`driver=` 값으로 내장 vs `nsql-driver-rpc`(프록시 세션)를 고른다. UI·CLI는 차이를 모른다(DR-7).

### 2-3. 배포 저장소·자산 이름·매니페스트
```text
저장소  github.com/SosomLab/nexa-sql-drivers            (드라이버 전용 · 태그 = <id>-v<semver> · 확장별 독립 릴리스)
       또는 확장별 저장소 nexa-sql-driver-<id>          (서드파티는 자기 저장소 — 색인이 URL을 가리킨다)
자산    nsql-driver-<id>-<ver>-<os>-<arch>.{zip|tar.zst}   os = windows|macos|linux · arch = x86_64|aarch64
        nsql-driver-<id>-<ver>-<os>-<arch>.sha256
        nsql-driver-<id>-<ver>-<os>-<arch>.sig             Ed25519(SosomLab 배포 키 · 본체에 공개키 내장 · nexa-dir 17 설계 재사용)
색인    github.com/SosomLab/nexa-sql-drivers/releases/latest 의 자산 `index.json`
        { "drivers": [ { "id": "oracle-oci", "name": "Oracle (OCI · Instant Client 동봉)", "dialect": "oracle",
                         "repo": "SosomLab/nexa-sql-drivers", "tag_prefix": "oracle-oci-v",
                         "requires": { "server": ">=11.2.0.4" }, "license": "…", "sxs": true }, … ] }
확장 안  nexa-driver.json  { "id", "version", "dialect", "protocol": 1, "exec": "nsql-driver-oracle-oci",
                            "client": { "kind": "oracle-instant-client", "version": "19.25" },
                            "compat": { "server_min": "11.2.0.4", "server_max": "21c" } }   ← §4 호환 표의 원천
```

## 3. "최신 버전" 다운로드 규칙 (사용자 요구 ③)

| 규칙 | 내용 |
|---|---|
| **기본 = 최신** | 설치 시 `GET /repos/<repo>/releases`에서 `tag_prefix`가 맞는 **최신 non-prerelease** 태그를 고른다(GitHub `releases/latest`는 저장소 전체 최신이라 확장별 태그 접두로 다시 거른다). 프리릴리스는 `--pre` 옵션 시에만 |
| **버전 지정** | `nsql driver install oracle-oci@19.25` — SxS(§4)용. `@latest` = 기본 |
| **갱신 확인** | 앱 시작 시 하루 1회(설정 `driver.check_updates` · 기본 켬 · 오프라인이면 조용히 건너뜀) 색인과 각 확장의 최신 태그를 비교 → 상태줄/드라이버 관리자에 "새 버전 x.y.z" 배지. **자동 교체는 안 한다** — 사용자가 Update를 누른다(접속 중인 세션이 쓰는 파일을 갈아치우지 않기 위해 · §5 삭제 보호와 같은 이유) |
| **Update = 새 버전을 옆에 설치**(SxS) + 프로필의 `driver=` 미지정 항목은 자동으로 최신을 쓴다. 고정(`@ver`)된 프로필은 그대로 → 사용자가 옮긴다 |
| **다운로드 검증** | ① `.sha256` 일치 ② `.sig` Ed25519 검증(본체 내장 공개키 · 실패 = 설치 거부, 우회 옵션 없음) ③ 압축 해제는 임시 폴더 → 검증 후 `drivers/<id>/<ver>/`로 **원자적 rename**(중단돼도 반쪽 설치 없음) |
| **네트워크** | HTTPS 클라이언트는 `rustls` + `webpki-roots`(원장에 이미 ② 계층 항목 · 트리에 tiberius가 끌어온 `rustls`·`ring` 있음) — 자체 HTTP/1.1 최소 구현(리다이렉트 · Content-Length · chunked). GitHub API는 익명 60회/시간 한도 → 색인은 캐시(`drivers/index.json` · ETag) · 토큰은 옵션(사설 저장소용 `NSQL_GITHUB_TOKEN`) |
| **오프라인 설치** | `nsql driver install --file <zip>` — 폐쇄망(사용자 실무 환경 가능성). 검증 규칙 동일 |
| **프록시** | `HTTPS_PROXY` 환경변수 존중(CONNECT 터널) — 1차 |

## 4. DBMS 버전별 호환 — SxS가 필요한가 (사용자 요구 ④ 조사)

### 4-1. 조사 결과

| DBMS | 클라이언트/드라이버 ↔ 서버 하위 호환 | SxS 필요 |
|---|---|---|
| **Oracle** | Oracle 지원 매트릭스(Doc 207303.1): **서버 23ai ← 클라이언트 19c·21c·23ai만**. 반대로 **구형 서버 11.2.0.4·12.1.0.2는 19c 클라이언트까지**(21c/23ai 클라이언트는 12.1 이하 미지원). 즉 한 조직에 11g/12c 레거시와 23ai가 공존하면 **Instant Client 두 판(19c + 23ai)이 동시에 필요**하다 — Toad도 19.3 + 23.7을 함께 번들한다([04 §Toad](04-oracle-tools.md)). ★ 결정적 제약: ODPI-C(kubo `oracle`)는 **프로세스당 Oracle 클라이언트 라이브러리를 한 번만 로드**(`dpiContext_createWithParams(oracleClientLibDir)` 첫 호출이 고정 · python-oracledb `init_oracle_client()`도 1회 규칙) → **접속별로 다른 버전을 쓰려면 드라이버가 별도 프로세스**여야 한다(§2-2가 SxS의 전제). 공식 순수 Rust thin(`oracledb`) GA 후에는 클라이언트 자체가 사라져 SxS 필요가 줄지만, thin이 지원하는 서버 하한(공식: 12·18·19·21·26)도 있으므로 OCI 확장은 레거시(11g)용으로 남는다. **09-13 스파이크**: beta.3이 사용자 19c에 Instant Client 없이 157ms 접속 ✓ — 그러나 OUT 바인드·REF CURSOR API 부재로 세션 변수 엔진에 아직 부적합(D-22) | **★ 예(1급)** |
| **SQL Server** | TDS는 하위 호환이 넓다 — tiberius = TDS 7.2+(SQL Server 2008+) 한 드라이버로 2008~2025·Azure. 갈리는 지점은 **TLS**: 2008/2008R2/2012 미패치 서버는 TLS 1.0/1.1만 → rustls(1.2+ 전용)로 못 붙는다(native-tls는 OS 정책 따라감) · TDS 8.0 strict(2022+) · Entra 인증. → "레거시 TLS 허용" **변종 빌드 1개**로 충분(SxS 아님 · 확장 `mssql-legacy-tls`) | 낮음(변종 1) |
| **PostgreSQL** | 와이어 프로토콜 v3(7.4~17) 단일 · SCRAM(10+)은 드라이버가 이미 지원 | 아니오 |
| **MySQL/MariaDB** | 프로토콜 호환 · `caching_sha2_password`(8.0)·MariaDB 분기는 드라이버 옵션 | 아니오 |
| **SQLite** | 파일 형식 안정 · bundled 한 판 | 아니오 |
| **Tibero · Altibase · CUBRID(ODBC)** | 벤더 ODBC 클라이언트가 **서버 메이저별로 다르다**(Tibero 6 ↔ 7 클라이언트 · Altibase 6/7). ODBC 드라이버 관리자 자체가 이름별 다중 등록을 지원하지만 우리는 확장 폴더에 벤더 클라이언트를 버전별로 두고 `driver=`로 고른다 | **예(3급)** |
| JDBC 래퍼 확장 | JDK + 드라이버 jar 버전 조합 — 확장이 자체 관리 | 확장 몫 |

**결론**: SxS는 **Oracle OCI와 ODBC 계열에 실제로 필요**하고, 그 전제(프로세스 격리)는 §2-2와 일치한다. 나머지는 단일 최신판 + 변종 1~2개면 된다. 따라서 저장소 구조는 **모든 확장에 버전 폴더를 두되**(균일한 관리 UI), `sxs: false`인 확장은 Update 시 구버전을 자동 정리 제안한다.

### 4-2. 로컬 보관 구조 (Windows SxS 유추 — 버전 폴더 · 참조 카운트 · 고아 정리)
```text
<사용자 설정 폴더>/nexa-sql/drivers/          ([21]과 같은 뿌리 · 여러 인스턴스 공유 · 원자적 rename 설치)
├─ index.json (+ .etag)                        색인 캐시
├─ oracle-oci/
│   ├─ 19.25.0/ nexa-driver.json · nsql-driver-oracle-oci.exe · instantclient/(oci.dll …) · LICENSE
│   └─ 23.7.0/  …
├─ mssql-legacy-tls/0.1.2/ …
└─ tibero-odbc/7.2.1/ …
```
- **선택 규칙**: 프로필 `driver=<id>@<ver>` 고정 → 그 버전. `driver=<id>` 또는 미지정 → 설치된 것 중 **최신** · 없으면 대화상자가 "Download <id> 최신" 제안. 접속 실패가 **서버 버전 불일치**(ORA-28040 등)면 오류 메시지에 "호환 버전 x.y 설치" 힌트(`compat` 표에서).
- **참조 카운트**: 프로필들이 `driver=`로 가리키는 버전 · 현재 열린 세션이 쓰는 버전(프로세스 살아 있음)을 관리자가 계산해 보여준다.
- **여러 인스턴스**: 설치는 임시 폴더 → rename이라 다른 인스턴스가 반쪽을 볼 일이 없다. 삭제는 다른 인스턴스가 쓰고 있을 수 있으니 **실행 파일 잠금 확인 후 실패 시 "다음 시작 때 삭제" 표식**(`.trash` 이동) — Windows에서 실행 중 파일은 삭제되지 않는다.

## 5. 관리 — 목록 · 설치 · 갱신 · **삭제** (사용자 요구 ④ UI)

### 5-1. CLI `nsql driver`
```text
nsql driver list [--all]            설치본: ID · 버전 · 방언 · 클라이언트(예 Instant Client 19.25) · 크기 · 사용 프로필 수 · 최신 여부(⬆ x.y.z)
                                    --all = 색인의 미설치 확장까지
nsql driver search [<키워드>]        색인 검색
nsql driver install <id>[@ver] [--pre] [--file <zip>]
nsql driver update [<id>|--all]     새 버전을 옆에 설치(고정 프로필은 그대로)
nsql driver rm <id>@<ver> [--force] 삭제 — 프로필이 참조하면 목록을 보여주고 거부(--force = 그 프로필들의 driver= 를 최신으로 옮기고 삭제)
nsql driver prune                   어떤 프로필도 안 쓰고 최신도 아닌 버전 일괄 삭제(확인 프롬프트)
nsql driver path                    drivers/ 폴더
```
### 5-2. GUI 드라이버 관리자 (DBeaver Driver Manager 대응 · 접속 대화상자 `Driver Settings`에서도 열림)
```text
┌ Drivers ───────────────────────────────────────────────────────────────────────────┐
│ [검색…]                                        [Download…] [Update all] [Prune]    │
│ ID · 이름            버전       클라이언트          크기    프로필  최신    상태       │
│ ▾ oracle-oci  Oracle(OCI)                                                          │
│     23.7.0             Instant Client 23.7   112MB   3      ✓                     │
│     19.25.0            Instant Client 19.25   98MB   1(erp-11g)      [Delete]     │
│ ▸ mssql-legacy-tls     0.1.2                  6MB    0      ⬆0.1.3   [Update]     │
│ ▸ tibero-odbc          7.2.1   Tibero 7 client 40MB   0             [Delete]     │
│ (색인 · 미설치) altibase-odbc · cubrid-odbc · jdbc-bridge …            [Download]  │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ 선택: oracle-oci 19.25.0 — 호환 서버 11.2.0.4~21c · 사용 프로필: erp-11g · 경로 …     │
└──────────────────────────────────────────────────────────────────────────────────────┘
```
- **Delete** 규칙 = CLI `rm`과 동일(참조 프로필 있으면 대화상자로 목록 + "프로필을 최신으로 옮기고 삭제" 선택). 열린 세션이 쓰는 버전은 Delete 비활성(툴팁에 세션 이름).
- 트리·표는 nexa-ctl `Tree` + 컬럼(dir2 `widgets/columns` 이관 U-2 전에는 고정 폭 텍스트).

## 6. 보안·라이선스
- 서명 검증은 **필수·우회 불가**(사용자 폴더의 실행 파일을 네트워크에서 받아 실행하는 경로이므로). 배포 키는 본체 소스에 공개키 상수 · 개인키는 릴리스 파이프라인 시크릿(nexa-dir 17 · clip 릴리스 파이프라인 계승).
- 서드파티 확장(다른 저장소)은 **그 저장소의 공개키를 사용자가 승인**(첫 설치 때 지문 표시 · `trusted-keys.conf`에 저장) — 승인 전 실행 없음.
- Oracle Instant Client 동봉은 OTN 라이선스 조건 확인(D-1) — 조건이 막히면 확장은 **"사용자 다운로드 안내 + 경로 지정"** 모드로 동작(§4 `client.kind`로 외부 경로 허용).
- 확장 프로세스는 본체와 같은 사용자 권한 — 1차. 권한 강등(seccomp·Job Object)은 clip `nclip-imgdec` 선례로 2차.

## 7. 단계 (백로그 · 순서가 중요 — 프로토콜 없이 다운로드는 의미 없음)

| 순서 | ID | 내용 | 검증 |
|:--:|---|---|---|
| 1 | **T-27** | `nsql-driver-rpc` — stdio JSON-RPC 프로토콜 v1(프레임 · 메서드 · Value 인코딩) + 프록시 `Session` + **참조 구현 = 내장 SQLite를 프로세스로 감싼 `nsql-driver-sqlite-rpc`**(테스트용) | 기존 nsql-run 테스트를 RPC 세션으로 그대로 통과 |
| 2 | **T-28** | `nsql-ext` 크레이트 — `drivers/` 레이아웃 · `nexa-driver.json` · 버전 선택 규칙 · 참조 카운트 · 삭제/휴지통 · `nsql driver list/rm/prune/path` | 단위 테스트(임시 폴더) |
| 3 | **T-29** | GitHub 다운로드 — 색인 · 최신 태그 해석 · HTTPS(rustls) · sha256 · Ed25519 검증 · 원자적 설치 · `install/update/search` · 갱신 배지 · 오프라인 `--file` | 실기: `SosomLab/nexa-sql-drivers` 첫 릴리스(sqlite-rpc로 파이프라인 검증) |
| 4 | **T-30** | Oracle OCI 확장 — 현 `nsql-driver-oracle`(kubo)을 RPC 프로세스로 분리 · Instant Client 19/23 두 판 SxS · 프로필 `driver=` · ORA-28040 힌트 | integration(Oracle Free 23ai) + 구형 서버는 사용자 실기 |
| 5 | **T-31** | GUI — DBeaver식 접속 대화상자(§1) · 로그인 리스트 · 드라이버 관리자(§5-2) | 사용자 실기 |

**본체에 남는 내장 드라이버**: SQLite · MSSQL(tiberius) · (M4) PG · MySQL · Oracle thin(GA 후). 내장/확장은 사용자에게 같은 "드라이버" 목록에 보이고, 내장은 `built-in` 배지 · 삭제 불가.

## 8. 열린 결정
- **D-19** 드라이버 저장소 구성 — 단일 `nexa-sql-drivers`(확장별 태그 접두) vs 확장별 저장소. 권장 = 단일(색인·CI 한 곳) + 서드파티는 별도.
- **D-20** Oracle OCI 확장의 Instant Client 동봉 여부(D-1과 통합) — OTN 조건 확인 후.
- **D-21** 갱신 확인 기본값(켬 권장 · 폐쇄망 배포는 설정으로 끔).

## 9. 접속 폼의 "바뀜" 표시와 Save 유도 — 설계(사용자 09-19 · 구현 = T-131)

**목표**: 어떤 칸이 저장본과 다른지 한눈에 · Save를 눌러야 함을 놓치지 않게 · 바꾼 것을 잃는 순간(다른 프로필 불러오기 · 창 닫기)에만 묻는다(DR-30 규칙 그대로).

| 항목 | 결정 | 이유 |
|---|---|---|
| 기준본 | `fill()`로 불러온 스펙(+이름 · 저장 비밀번호 여부)을 **스냅숏**으로 보관 · New는 빈 스냅숏 | "바뀜" = 지금 값 ≠ 스냅숏(칸별 비교 · 공백 정규화) |
| 칸 표시 | 입력란 **왼쪽 안쪽 2px 강조색 띠** + 라벨 끝 `•`(예: `Host •`) | 편집기의 줄 변경 표시(`editor.diff_marks`)와 같은 어휘 — 새 기호를 배우지 않는다 · 테두리 색을 바꾸는 방식은 포커스 링과 겹친다 |
| 콤보(DB 종류)·체크(Save password) | 같은 띠(컨트롤 왼쪽) | 텍스트박스와 동일 규칙 |
| Save 버튼 | 바뀐 칸이 하나라도 있으면 **강조색(Accent) + 라벨 `Save •`** · 툴팁 "저장하지 않은 변경 n개" · 없으면 기본 | 툴바 Commit 배지와 같은 문법(대기 = 강조) |
| 상태줄(폼 아래) | 바뀐 동안 `● 변경됨 — Save로 저장` 한 줄(Test/Connect 결과가 오면 그 결과가 우선 · 결과 뒤에도 바뀜이 남아 있으면 `· 미저장`) | 상태는 한 번에 하나 규칙 유지 |
| 목록 | 폼이 바뀐 프로필의 이름 뒤 `*`(편집기 탭의 더러움 표시와 같음) | 목록만 보고도 알 수 있게 |
| 잃는 순간 | 다른 프로필 불러오기 · New · 창 닫기 · 삭제 때 바뀜이 있으면 **저장 / 버림 / 취소** 팝업(`open_tx_guard`와 같은 부품) · Test/Connect는 폼 값으로 그냥 시도(저장 불필요) | 묻는 것은 잃을 때만 |
| 되돌리기 | 라벨 `•` 클릭 = 그 칸만 스냅숏으로 · 폼 우클릭 "변경 취소" = 전부 | 실수 복구 한 번에 |
| 초기화 | Save 성공 · 불러오기 · New → 스냅숏 갱신 · 띠 전부 꺼짐 | — |

**부품**: nexa-ctl `TextBox::set_modified(bool)`(왼쪽 띠 · `Combo`·`Checkbox`에도 같은 세터) · 버튼은 기존 `ToolTone::Accent`/라벨 교체 · 팝업은 기존 확인 팝업 재사용. 판정은 순수 함수 `dirty_fields(snapshot, form) -> Vec<Field>`(MC/DC: 칸별 · 공백만 다른 경우 · 비밀번호 저장 체크만 바뀐 경우).

**탭 표식 색(같은 날 결정)**: S(공유) = 초록(연결됨 · 평상시) · P(전용) = 강조색 파랑(예외적·격리된 연결 = "이 탭만 다르다") · 미연결/끊김 = 흐림(+ 끊김 확인은 빨강 플러그).

## 10. 로그인 **필수 항목** 식별 — 조사·설계(사용자 09-19 · 확인 대기 → 구현 = T-133)

**요구**: 접속 폼에서 "무엇을 채워야 접속/저장이 되는가"를 사용자가 **보고 알 수 있게** · 빠졌을 때 **어느 칸인지** 짚어 준다. 지금은 `from_parts`가 호스트/파일 경로만 검사하고 나머지는 드라이버 오류 문장("ORA-01017" 등)으로 돌아와 어느 칸이 문제인지 알 수 없다.

### 10-1. 조사 — 다른 도구는 어떻게 하나

| 도구 | 필수 표시 | 빠졌을 때 | 비고 |
|---|---|---|---|
| Azure Data Studio | 라벨 뒤 **빨간 `*`**(Server · Authentication type · 인증별 User/Password) | **Connect 비활성** + 칸 아래 "This field is required" | 폼 규칙이 가장 명시적 |
| DBeaver | 표시 없음 | Test/Finish 때 대화상자("Host is required" 류) · 드라이버 오류 그대로 | 필수 집합은 드라이버 descriptor(`<parameter>`)가 가짐 |
| DataGrip | 표시 없음 · 칸 비면 URL 미리보기가 어긋남 | Test Connection 오류 풍선 | URL이 곧 검증 |
| SSMS | Server name만 · 표시 없음 | Connect 실패 대화상자 | 인증 종류가 User/Password 필요 여부를 정한다 |
| TablePlus · Navicat | 표시 없음 | 드라이버 오류 | — |
| 웹 폼 관례(WCAG · GOV.UK · Material) | 필수 `*` **또는** 대부분 필수면 **선택 칸에 "(optional)"** | 제출 시 첫 오류 칸으로 포커스 + 칸 옆 사유 | "비활성 버튼 + 이유 없음"은 나쁜 패턴(왜 못 누르는지 모른다) |

**결론**: 명시적 표기(Azure Data Studio) + 제출 시 지목(웹 관례). 버튼은 **비활성화하지 않는다** — 누르면 빠진 칸을 표시하고 첫 칸으로 포커스(사용자의 다음 동작이 한 번의 입력으로 이어진다 · CLAUDE.md 팝업 규칙과 같은 정신).

### 10-2. 필수 집합 — 방언 × 동작

"필수"는 **동작마다 다르다**: Save는 이름이 필요하고 비밀번호는 없어도 되며(저장 안 함 체크) · Test/Connect는 자격이 필요하다.

| 방언 | Test / Connect | Save | 선택(optional) | 근거 |
|---|---|---|---|---|
| Oracle | Host · Database(서비스/SID) · User · Password | Name · Host · Database · User | Port(기본 1521) · Schema | 서비스 없이는 접속 불가 · OS 인증(`/`)은 미지원 |
| PostgreSQL | Host · User · Password | Name · Host · User | Port(5432) · Database(비면 libpq 규칙 = 사용자 이름) · Schema | `trust` 인증은 비밀번호 없이 되지만 **인라인 규칙(52 §4)과 같게 필수** — 예외는 T-132 |
| SQL Server | Host · User · Password | Name · Host · User | Port(1433) · Database(로그인 기본 DB) | Windows 인증 미지원(mac/linux) |
| SQLite | Database(파일 경로 · `:memory:`) | Name · Database | 전부 | 자격 없음 |

원장은 **한 곳**: `nsql_core::Dialect::login_fields() -> &'static [LoginField]`(`LoginField { field, required_for: Connect | Save | Both, default: Option<&str> }`) — 폼·CLI(`nsql connect`의 인자 검사)·확장 드라이버 매니페스트(§2-3 `fields`)가 같은 표를 본다. 폼에 하드코딩하지 않는다([30](30-architecture-patterns.md) 포트+레지스트리).

### 10-3. 표시·동작 설계

| 항목 | 결정 |
|---|---|
| 라벨 | 필수 칸은 라벨 뒤 **`*`**(예: `Host *`) — 방언을 바꾸면 따라 바뀐다(Oracle Database `*` · PG Database 없음). `*`는 항상 · `•`(바뀜)와 겹치면 `Host * •` |
| 빈 필수 칸 | 평소에는 표시 없음(입력 중에 빨갛게 하지 않는다). **동작(Test/Connect/Save)을 누른 순간** 빠진 칸에 **경고색 띠**(바뀜 띠 자리 · 경고 > 바뀜) + 상태줄 `Required: Host, Password` + **첫 빠진 칸으로 포커스** · 그 칸에 글자를 넣으면 띠 즉시 해제 |
| 버튼 | 비활성화하지 않는다(이유를 알 수 있게) — 행 버튼(목록 아이콘)은 종전대로 `row_ready`(비밀번호 있음 ∨ SQLite)만 |
| 플레이스홀더 | 선택 칸은 `(optional)`을 뒤에 — Port는 기본값 숫자를 그대로 |
| 비밀번호 | Connect/Test 필수(SQLite 제외) · Save는 아님(저장 안 함 = 접속 때 세션 비밀번호/T-132) |
| CLI | `nsql connect`/`--target` 검사도 같은 표 → "Password required — …"(이미 워커에서 · 09-19) |
| 판정 | 순수 함수 `missing_fields(dialect, action, form) -> Vec<Field>`(MC/DC: 방언 4 × 동작 2 × 칸별 빈 값) |

**부품**: nexa-ctl `TextBox::set_warning(bool)`(경고색 띠 · `set_modified`와 같은 자리 · 우선순위 경고 > 바뀜) · 라벨 `*`는 폼이 그린다 · 포커스 이동은 기존 `own_focus`.

**열린 결정(사용자 확인)**: D-115 PostgreSQL `trust`·Oracle OS 인증처럼 비밀번호 없는 접속을 허용하는 스위치를 둘 것인가(권장: 두지 않음 · T-132 비밀번호 변수/모달로 대신) · D-116 `*` 대신 "(optional)" 표기만 쓸 것인가(권장: `*` — 필수 칸이 적고 방언마다 달라 `*`가 더 명확).
