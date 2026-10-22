# 121 · 지원해야 하는 데이터베이스 — 분석과 우선순위 (134차 win · 2026-10-21 · 검토)

> 요구(사용자 10-21): "지원해야 하는 데이터베이스를 분석하고 우선순위를 정리". 원천 = DR-11(등급 1·2·3급) · DR-28(PG 1급 승격) · [00 §5](00-foundation-report.md) · [64 §1](64-dbms-clients-and-driver-packaging.md) · [108](108-dbms-syntax-and-objects-survey.md)(방언 6 조사) · [03](03-competitive-landscape.md)(경쟁 도구 지원 목록). 상태 = **검토**(결정 D-336~D-339 대기).

## 0. 한 줄 결론

지금 = **내장 드라이버 4**(Oracle OCI/Thin · SQL Server · PostgreSQL · SQLite) + **방언만 있는 둘**(MySQL · ODBC = 드라이버 없음). 다음 순서 권장 = **P1 MySQL/MariaDB 내장**(방언·카탈로그·문법이 이미 있어 드라이버만 · 순수 Rust · 전 세계 수요 1~2위) → **P2 ODBC 일반 드라이버**(한 번에 Tibero·Altibase·CUBRID·DB2·HANA·Goldilocks = 국산·레거시 전부 3급으로 열림) → **P3 Tibero 전용 방언**(ODBC 위에 Oracle 호환 방언 = 국내 공공·금융) → **P4 DuckDB**(임베디드 분석 · 파일 DB = SQLite와 같은 자리) → **P5 ClickHouse/클라우드 DW**(수요 확인 뒤). NoSQL(Mongo·Redis)은 범위 밖 유지(DR-11).

## 1. 기준(가중치)

| 기준 | 가중 | 뜻 |
|---|---|---|
| A. 사용자 실수요 | 3 | 사용자(SosomLab · BI/SCM 컨설팅)가 실제로 붙는 서버 = Oracle(BISCM) · SQL Server(M4PLAN) · PG(Repository · Supabase) · 고객사 환경 |
| B. 시장 수요 | 2 | 국내 기업·공공(Oracle · SQL Server · Tibero · MySQL/MariaDB · PG 증가) · 세계(PG · MySQL · SQL Server · Oracle · SQLite · MongoDB) · 경쟁 도구 공통 지원 집합(03) |
| C. 드라이버 경로 | 2 | 순수 Rust crate 있음(DR-3 예외 원장) > C 라이브러리 FFI(OCI처럼 지연 적재) > ODBC > HTTP API |
| D. 구현 비용 | 2 | 방언·카탈로그·문법·완성·Caps가 이미 있나(108) · 러너 관용(세션 변수 · SELECT INTO · OUT) · 시험 자산 |
| E. 아키텍처 적합 | 1 | 세션 모델(52) · 카탈로그 포트 · 그리드 편집 키 규칙(87) · 벌크(89)와 맞물리는가 |

## 2. 후보 분석

| DB | A 실수요 | B 시장 | C 드라이버 경로 | D 비용(남은 일) | E | 합(가중) | 등급·판정 |
|---|---|---|---|---|---|---|---|
| **Oracle** | 3 | 3(국내 1위) | ✅ OCI(FFI) + Thin(`oracledb`) | 완료 | ✅ | — | **1급 · 완료** |
| **SQL Server** | 3 | 3 | ✅ `tiberius` | 완료 | ✅ | — | **1급 · 완료** |
| **PostgreSQL** | 3 | 3 | ✅ `tokio-postgres` | 완료 | ✅ | — | **1급 · 완료**(DR-28) |
| **SQLite** | 2 | 3(임베디드 1위) | ✅ `rusqlite` | 완료 | ✅ | — | **2급 · 완료** |
| **MySQL / MariaDB** | 1(지금 고객 없음) | 3(세계 1~2위 · 국내 웹/스타트업) | ✅ 순수 Rust(`mysql` 동기 또는 `mysql_async` · MariaDB 같은 프로토콜) | **낮음** — 방언·카탈로그(`information_schema`)·문법(`mysql.sqlg`)·완성·Caps 이미 있음(108 §6) · 남은 것 = 드라이버 crate 어댑터 + 러너 관용(세션 변수 `SET @x` · `SELECT … INTO @x` · OUT 인자) + E2E | ✅ | **3·6·2·2·1 = 14** | **P1 · 2급 내장** |
| **ODBC(일반)** | 2(고객사 레거시 접속 요청 가능) | 2 | `odbc-api`(Rust · 시스템 ODBC 관리자 = Windows 내장 · mac/Linux unixODBC) | **중간** — 방언 `Dialect::Odbc`·108 §7 있음 · 드라이버 어댑터 + 카탈로그 = `SQLTables/SQLColumns` 일반 경로 + 타입 매핑 · 3-OS 패키징(unixODBC 의존) | 읽기·조회·export 공통 기능(3급 = 그리드 편집은 키 규칙 한정) | 6·4·3·2·1 = **16**(개방 효과 포함) | **P2 · 3급 틀**(한 번에 여러 DB) |
| **Tibero**(TmaxSoft) | 1(공공·금융 고객 가능) | 2(국내 공공 표준급) | ODBC/JDBC/tbCLI(C) — 순수 Rust 없음 → **ODBC 위** | 낮음(ODBC 뒤) — **Oracle 호환 방언**이라 Oracle 방언 재사용 + 차이표(108 §1 템플릿) | ✅ | 3·4·2·3·1 = **13** | **P3 · 3급(ODBC + Oracle 호환 방언)** |
| Altibase · CUBRID · Goldilocks | 0 | 1~2(국내 공공·통신) | ODBC | 낮음(ODBC 뒤 · 방언 차이표만) | ✅ | 0·3·2·3·1 = 9 | 3급 · ODBC 일반으로 자동 커버(전용 방언은 요청 시) |
| **DuckDB** | 1(분석 파일 · CSV/Parquet) | 2(급성장 · 분석가) | ✅ `duckdb` crate(C 라이브러리 번들 ≈ +20 MB!) 또는 동적 적재 | 중간 — SQLite 자리(파일 = 서버 하나) 재사용 · PG 호환 방언 | ✅ | 3·4·2·2·1 = 12 | **P4 · 2급 후보**(크기 때문에 **확장 드라이버**(22 §0 cdylib) 1호 적합) |
| ClickHouse | 0 | 2(분석 · 로그) | HTTP/native(`clickhouse` crate) | 중간(방언 새로) | 부분(트랜잭션 없음) | 0·4·2·1·0 = 7 | P5 · 수요 확인 뒤 |
| Snowflake · BigQuery · Redshift(클라우드 DW) | 0 | 2 | HTTP REST(OAuth · 키) | 높음(인증·비동기 작업 모델 다름) | 부분 | 0·4·1·0·0 = 5 | P5 · 보류 |
| IBM Db2 · SAP HANA | 0 | 1(대기업 레거시) | ODBC/CLI | 낮음(ODBC 뒤) | ✅ | 0·2·2·3·1 = 8 | 3급 · ODBC로 커버 |
| MongoDB · Redis 등 NoSQL | 0 | 3 | — | SQL 클라이언트 범위 밖 | ✗ | — | **범위 밖 유지**(DR-11) |

합 = A×3 + B×2 + C×2 + D×2 + E×1(완료 항목 제외). 순서 = MySQL 14 → ODBC 16(개방 효과) → Tibero 13 → DuckDB 12 → 그 밖.

## 3. 우선순위(권장안)

| 순위 | 대상 | 등급 | 왜 | 범위(1차) | 비용 추정 |
|---|---|---|---|---|---|
| **P1** | **MySQL / MariaDB 내장 드라이버** `nsql-driver-mysql` | 2급 → 1급 승격 가능 | 세계 수요 최상위 · 방언·카탈로그·문법·완성이 **이미 있음**(108 §6) · 순수 Rust · 경쟁 도구 전부 지원(03) | 접속(TLS 포함) · 질의/페치/취소 · 세션 변수 관용 · 카탈로그 전부 · 그리드 편집(PK 키 규칙) · 벌크(`LOAD DATA LOCAL` 또는 다중행 INSERT) · CLI 동등 · E2E(docker 컨테이너 · integration.yml 추가) | 중(드라이버 어댑터 + 러너 관용 + 시험) · 크기 +1~1.5 MB(120 §3와 함께 검토) |
| **P2** | **ODBC 일반 드라이버** `nsql-driver-odbc`(**내장** · 사용자 확정 10-21) | 3급 틀 | 한 번에 Tibero·Altibase·CUBRID·Db2·HANA·Access·Excel까지 **읽기·조회·export** 개방 · 고객사 레거시 대응 | 접속 문자열/DSN · 질의/페치 · `SQLTables/SQLColumns/SQLPrimaryKeys` 카탈로그 · 타입 매핑 · 트랜잭션 · 3-OS(unixODBC 패키징 의존 명시) · 그리드 편집 = PK 있을 때만 | 중 · 크기 +0.5 MB |
| **P3** | **Tibero**(**확장 드라이버** · 사용자 확정 10-21 · 1차 = 내장 ODBC 위 방언 팩(Oracle 상속 + 차이표) · 2차 = tbCLI cdylib 선택) | 3급 → 2급 | 국내 공공·금융 표준급 · Oracle 방언 90 % 재사용 · 차이표만 | 방언 상속(Oracle) + 카탈로그 뷰 차이 + 문법 차이표 · 실서버 검증 = 사용자 환경 필요 | 낮(ODBC 뒤) |
| **P4** | **DuckDB**(**확장 드라이버** · 사용자 확정 10-21) | 2급 후보 | 분석 파일(CSV/Parquet) 즉시 질의 · 급성장 | cdylib 확장(22 §0 · 크기 +20 MB를 본체에 안 넣음) · SQLite와 같은 파일-서버 모델 | 중(확장 드라이버 틀 = DR-29 설계 선행) |
| **P5** | ClickHouse · 클라우드 DW · Db2/HANA 전용 방언 | — | 수요 확인 뒤 · Db2/HANA는 ODBC로 이미 접속 가능 | — | — |
| 범위 밖 | NoSQL | — | DR-11 유지 | — | — |

**등급 정의 재확인(DR-11)** = 1급: 전 기능(편집·그리드 편집·벌크·탐색기 전수·DDL·운영 가드) · 2급: 편집·그리드·export + 탐색기 주요 폴더 · 3급: 공통 기능(접속·질의·결과·export·탐색기 테이블/컬럼) · 플러그인: 확장 드라이버.

## 4. 각 순위의 선행·연동

- P1 MySQL: 드라이버 crate 선택(DR-3 원장 등재 · 동기 `mysql` crate가 세션 모델(52)과 맞음 · TLS = rustls → **120 S-2의 TLS 단일화와 같은 공급자**로) · `Caps` 채움(`bulk_load` · `tsql_print` 등) · `integration.yml`에 MySQL 컨테이너 · 108 §6 T-15 현황 갱신.
- P2 ODBC: 패키징 33(Linux unixODBC 런타임 의존 · mac = Homebrew `unixodbc` 안내) · 설정 ▸ DBMS에 DSN 목록 · 64 §1 표 갱신.
- P3 Tibero: 사용자 또는 고객사 Tibero 검증 환경 필요(없으면 문서만 선행).
- P4 DuckDB: DR-29(내장 정적 유지 · 확장 = cdylib) 설계를 **먼저 구현**(22 §0 · 64 §3-4) → DuckDB가 1호.
- 크기(120)와 충돌 = 드라이버를 더할수록 exe가 커진다 → P1부터 **120의 패키지별 `opt-level="s"`·TLS 단일화**를 같이 적용하고, P4부터는 확장 드라이버로.

## 5. 결정 대기

| # | 결정 | 권장 |
|---|---|---|
| D-336 | 다음 지원 순서 = P1 MySQL/MariaDB(내장) → P2 ODBC(내장) → P3 Tibero(확장) → P4 DuckDB(확장) → P5 수요 확인 | ✅ **사용자 확정 10-21**(묶음 = D-342) · 순서는 권장안 · ★ **10-22 사용자 재확인** = ODBC(내장) 완료 → Tibero · DuckDB **확장 모듈**(GitHub Releases에서 **자동 다운로드** · DR-24) |
| D-337 | MySQL 등급 = 2급 시작 → 1급 승격(그리드 편집·벌크·운영 가드까지) | 1급 목표(방언 자산이 이미 1급 수준) |
| D-338 | ODBC = 3급 틀로 먼저 · 전용 방언(Tibero 등)은 요청·검증 환경 있을 때 | ✅ |
| D-339 | DuckDB = 확장 드라이버 1호(본체 크기 보호) | ✅ |

## 6. 할 일 후보(결정 뒤 TODO 등재)

> 10-22 정정: MySQL 드라이버 = **T-387**(처음 예약한 T-382는 같은 날 실행 파일 크기 축소 보류 묶음에 쓰였다 · 결정 = D-344).

| # | 내용 |
|---|---|
| T-387 ✅ 1차(10-22) | `nsql-driver-mysql`(crate 선택 · 어댑터 · 러너 관용 · 카탈로그 · 벌크 · CLI · E2E · CI 컨테이너) · 시험 서버 = GitHub 코드스페이스 `nsql-mysql-135`(사용자 10-22 = 평소 **중지** · T-387 완료 뒤 **삭제**) |
| T-383 | `nsql-driver-odbc`(odbc-api · 카탈로그 일반 경로 · 3-OS 패키징 의존) |
| T-384 | Tibero 방언(Oracle 상속 + 차이표 · 108 §1 템플릿) |
| T-385 | 확장 드라이버 틀(22 §0 · cdylib · 지연 dlopen) → DuckDB 1호 |
| T-386 | 위키 "지원 데이터베이스" 페이지(등급표 · 전제 조건 · 64 §1 요약 · 영어 우선) |

## 7. 기술 방식 — 주요 DBMS는 **내장** · 추가 드라이버는 **GitHub 저장소에서 다운로드**(★ 사용자 확정 10-21)

> 사용자 10-21: "다양한 데이터베이스를 지원하려면 주요 DBMS는 내장으로 제공하고, 사용자가 추가로 쓰려는 드라이버는 GitHub 저장소를 통해 다운로드하도록 지원". 원천 결정 = DR-24(GitHub Releases 다운로드 · 관리자) · DR-29(확장 = in-process cdylib C ABI · 지연 dlopen · 프로세스는 격리 선택) · [22 §0·§2](22-driver-extensions.md) · [64 §3-4](64-dbms-clients-and-driver-packaging.md) · 확장 매니저 [50 §1](50-extension-system.md)(index.json · SHA-256 · Ed25519 서명 · `extensions.repositories`). 여기서는 그 결정을 **드라이버 배포 체계** 한 장으로 묶는다.

### 7-1. 두 묶음

| 묶음 | 무엇 | 어떻게 | 크기·기동 |
|---|---|---|---|
| **내장(built-in)** | Oracle(OCI/Thin) · SQL Server · PostgreSQL · SQLite · **MySQL/MariaDB(P1)** · **ODBC 일반(P2)**(★ 사용자 확정 10-21 = ODBC까지 내장) | 정적 링크(`nsql-drivers` feature) · 설치본 하나(DR-27) · 네트워크 0 · ODBC는 OS 드라이버 관리자 의존(Windows 내장 · mac/Linux unixODBC 런타임 = 패키징 33에 의존 명시) | exe에 포함(120의 축소 손잡이로 관리 · MySQL +1~1.5 MB · ODBC +0.5 MB) · 기동 0 |
| **확장(downloadable)** | **Tibero**(1차 = 내장 ODBC 위 방언 팩 · 2차 = tbCLI cdylib) · **DuckDB**(cdylib ≈ +20 MB) · 그 밖 = Altibase/CUBRID/Goldilocks 방언 팩 · ClickHouse · 클라우드 DW · 벤더 네이티브 클라이언트가 필요한 것 · 커뮤니티 드라이버 | **GitHub Releases 자산** → 앱 안 드라이버 관리자가 내려받아 `<NSQL_HOME>/drivers/<id>/<ver>/`에 배치 → **첫 접속 때 지연 적재**(`LoadLibrary`/`dlopen`) · 방언 팩은 적재할 코드 없이 `dialect.json`만 | exe 불변 · 기동 때 색인(JSON)만 읽음 = 상주 0 · 적재한 드라이버만 메모리 |

원칙 = 본체는 올 러스트 단일 바이너리(DR-1)를 지키고, "넣을 수 없거나 넣기 싫은 것"(벤더 C 클라이언트 · 라이선스 조건 · 큰 번들 · 수요 적은 DB)만 확장으로.

### 7-2. 드라이버 패키지 형식(확장 하나 = GitHub 저장소 하나)

```
<저장소 Release 자산>
  nsql-driver-<id>-<ver>-windows-x86_64.zip   ┐  각 OS·아키텍처별 zip(cdylib + 동봉 라이브러리 + manifest)
  nsql-driver-<id>-<ver>-macos-aarch64.zip    │
  nsql-driver-<id>-<ver>-linux-x86_64.zip     ┘
  sha256sums.txt · sha256sums.txt.sig          서명(Ed25519 · 발행자 키)
  driver.json                                  매니페스트 = id · 이름 · 설명 · 버전 · abi_version · 방언 기반(dialect_base: oracle|postgres|mysql|generic) ·
                                               OS 목록 · 동봉 파일 · 요구(예 unixODBC · 벤더 클라이언트 경로 설정 키) · 최소 앱 버전 · 능력표(Caps) · 설정 스키마
```

- **코드 형태** = `crate-type = ["cdylib"]` Rust 크레이트(권장 · 템플릿 저장소 `nexa-driver-template` 제공 · `nsql-abi` crate 의존) — C/C++/Zig도 같은 C ABI를 내보내면 됨 · WASM은 드라이버에 부적합(소켓·네이티브 로드 불가 · 22 §2-2).
- **ABI** = `nsql-abi`(버전 있는 C vtable: `abi_version · detect · open · execute · fetch_next · commit · rollback · set_option · cancel · caps · catalog_* · close`) + `Value/ExecRequest/ExecResult` 바이너리 코덱(64 §3-4). 호환 규칙 = 주 버전 같을 때만 적재 · 아니면 "드라이버 갱신 필요".
- **방언** = 매니페스트의 `dialect_base`로 기존 방언(문법 `.sqlg` · 완성 · 카탈로그 질의 틀)을 **상속**하고 차이표(108 §1 템플릿)를 드라이버 안 `dialect.json`으로 덮어쓴다 → Tibero = Oracle 상속 · DuckDB = PostgreSQL 상속 · ODBC 일반 = generic.

### 7-3. 색인·저장소·보안

| 항목 | 규칙 |
|---|---|
| 공식 색인 | `SosomLab/nexa-drivers` 저장소의 Release 자산 `index.json`(드라이버 id · 저장소 URL · 최신 버전 · OS별 자산 URL · sha256 · 서명 · 등급 · 최소 앱 버전) · **Ed25519 서명**(nexa-license 키 체계 재사용 · 공개키 본체 내장 · 92 R1~R4) |
| 사용자 추가 저장소 | 설정 `drivers.repositories`(= 확장 `extensions.repositories`와 같은 틀 · D-89 ② = 공식 + 사용자 URL · **서명 필수**) · 서명 없는 드라이버 = 개발자 모드(`drivers.dev_dir` 폴더 · 핫 리로드 · 로그 층)에서만 |
| 검증 | 다운로드 → sha256 → 서명 → zip 풀기 → `driver.json` 읽기 → `abi_version`·OS·최소 앱 버전 검사 → 등록. 실패 = 배치 안 함 + 사유 |
| 네트워크(26 §8) | 관리자 열기·⟳·설치·갱신 확인(설정 주기 · 기본 하루 1회 · 끌 수 있음)에만 · 접속한 적 없는 서버에 지속 트래픽 0 · 진행률·취소 · 프록시 = OS 설정 |
| 오프라인·사내망 | zip을 받아 **폴더에 풀어 두면 끝**(`NSQL_DRIVER_DIR` 또는 설정 창 "폴더에서 설치…") · 서명 검증은 동일 |
| SxS | `drivers/<id>/<ver>/` 버전별 공존 · 프로필이 버전을 고정할 수 있음(`driver=<id>@<ver>`) · 기본 = 최신 · 삭제 = 관리자 |

### 7-4. 앱 쪽 구조

```
nsql-core::Session(포트)
  ├─ 내장 어댑터(정적)                                    ← Oracle · MSSQL · PG · SQLite · MySQL(P1)
  ├─ nsql-driver-dyn(프록시 세션) ── dlopen ── <id>.dll/.so/.dylib   ← 확장 기본(in-process · 지연 · 호출 지연 0)
  └─ nsql-driver-rpc(프록시 세션) ── 프로세스 ── 같은 ABI를 감싼 호스트  ← 격리 모드(설정 `drivers.<id>.isolate` · 크래시 분리 · SxS가 꼭 필요할 때)
```
- UI·CLI·러너·카탈로그·그리드 편집은 **차이를 모른다**(같은 `Box<dyn Session>` · `Caps`로 능력만 묻는다 · 61 §1-6).
- 접속 창·프로필 = 서버 유형 목록에 설치된 확장 드라이버가 **등급 표식과 함께** 나타남(미설치 = "설치…" 링크 → 관리자) · CLI `nsql driver list|install|remove|update|path`.
- 관리자 UI = 확장 패널(50 §14)에 **종류 = 드라이버** 탭 추가(목록 · 검색 · 설치 · 갱신 · 삭제 · 버전 · 요구 사항 · 상세 = T-374 방식 백그라운드).
- 패닉·멈춤 = in-process 모드에서는 `catch_unwind`(Release `unwind` 전환 = D-329와 같은 결정) + 호출 상한(107 ①) · 격리 모드에서는 프로세스 재시작.

### 7-5. 순서(DR-29 · 64 §3-4 그대로 · 결정 뒤 TODO)

① 내장 = MySQL/MariaDB(P1) → ODBC 일반(P2) 드라이버 크레이트 → ② `nsql-abi` + 코덱(내장 드라이버로 왕복 시험 · 성능 = 정적 대비 호출 지연 측정) → ③ SQLite를 cdylib로도 빌드해 `nsql-driver-dyn` 프록시 검증 → ④ 색인·관리자(확장 매니저 재사용 · 서명 · 오프라인 설치) + 템플릿 저장소 `nexa-driver-template` + 드라이버 저장소 CI(3-OS cdylib 빌드 · Release 자산 · sha256 · 서명) + **방언 팩 형식**(코드 없는 확장 = `driver.json` + `dialect.json`) → ⑤ 첫 확장 = **Tibero 방언 팩(P3)** → ⑥ **DuckDB cdylib(P4)** → ⑦ 격리 모드(`nsql-driver-rpc`)는 필요할 때.

### 7-6. 결정

| # | 결정 | 권장 |
|---|---|---|
| D-340 | 드라이버 배포 체계 = 내장(1·2급 주요 DBMS) + GitHub Releases 다운로드 확장(cdylib C ABI · 서명 색인 · 지연 적재 · SxS · 오프라인 설치 · 격리 모드 선택) | ✅ **사용자 확정 10-21**(방식 = DR-24·DR-29 그대로) |
| D-341 | 공식 색인 저장소 이름 = `SosomLab/nexa-drivers` · 템플릿 = `nexa-driver-template` · 서명 키 = nexa-license 체계 재사용 | 권장 |
| D-342 | **내장 = Oracle · SQL Server · PostgreSQL · SQLite · MySQL/MariaDB · ODBC** / **확장 드라이버 = Tibero · DuckDB**(ClickHouse·클라우드 DW·Db2/HANA 전용 방언도 확장 · ODBC 일반 접속은 내장으로 이미 가능) | ✅ **사용자 확정 10-21**("MySQL/MariaDB ODBC까지 내장 · Tibero · DuckDB를 확장 드라이버로") |
