# 64. DBMS별 종속 설정 · 드라이버를 내장할 것인가 확장으로 뺄 것인가

> 사용자 09-21(Windows 89차): ① "Oracle은 Instant Client가 필수 — 경로를 찾아 직접 설정하면 그 버전으로 실행되게 · 별도 설정 카테고리 · 자동 탐지된 결과는 읽기 전용 · 직접 지정하면 바꿀 수 있는 것만 편집 · `tnsnames.ora` 등 파생 정보는 읽기 전용 · 다른 DBMS도 같은 틀로" ② "지금 지원하는 DBMS 가운데 Oracle처럼 별도 설정이 필요한 대상을 정리" ③ "DBMS 지원 모듈을 지금처럼 Native로 할지 Extension으로 할지 기술 검토 · 필요할 때만 추가해 쓰는 방식으로 성능·속도·용량을 개선할 수 있는지 설계·리뷰".
>
> 앞선 결정: [DR-27~29](10-decision-record.md) · [22 §0](22-driver-extensions.md)(내장 드라이버 = 정적 링크 유지 · 확장 드라이버 = in-process 동적 라이브러리) — 이 문서는 그 결정을 **이번 실측으로 다시 검토**하고 Oracle 클라이언트 설정을 기록한다.

## 1. 지금 지원하는 DBMS — 무엇이 따로 필요한가

| DBMS | 드라이버 | 실행에 **따로 필요한 것** | 선택 사항(있으면 쓰는 것) | 상태 |
|---|---|---|---|---|
| **Oracle** | `oracle` 크레이트 = ODPI-C → **OCI 네이티브 라이브러리** | ★ **Oracle Instant Client**(Windows `oci.dll` · Linux `libclntsh.so` · macOS `libclntsh.dylib` — 이 PC의 23.9 Basic = 407 MB · 핵심 `oraociei.dll` 307 MB) — 없으면 접속 자체가 안 된다(DPI-1047) | `TNS_ADMIN`의 `tnsnames.ora`(별칭 접속) · `sqlnet.ora`(암호화·지갑) · Windows는 VC++ 재배포 패키지 | ✅ 내장 + **설정 ▸ DBMS ▸ Oracle**(§2) |
| SQL Server | `tiberius`(순수 Rust TDS) | 없음 | 서버가 TLS를 강제하면 인증서 신뢰(지금 = `mssql.encrypt` full/login) · Windows 통합 인증(SSPI)·Kerberos는 **미지원**(쓰려면 OS 구성 요소가 필요해진다) | ✅ 내장 |
| PostgreSQL | `postgres`(순수 Rust) | 없음 | `~/.pgpass` · `pg_service.conf` · `sslrootcert`는 **읽지 않는다**(libpq의 기능 — 비밀번호는 볼트가 맡는다) · TLS 미지원(서버가 `hostssl`만 허용하면 접속 불가 → 필요해지면 `rustls` 추가) | ✅ 내장 |
| SQLite | `rusqlite` `bundled`(SQLite C 소스를 앱에 컴파일) | 없음 | 확장 모듈 로드(`load_extension`)는 꺼 둠 | ✅ 내장 |
| MySQL · MariaDB | (없음) | — | 순수 Rust 드라이버(`mysql`)면 따로 필요한 것이 없다 | ☐ 미구현 — `Dialect::Mysql`과 능력표만 있다 |
| ODBC(Tibero · Altibase · CUBRID …) | (없음) | ★ **ODBC 드라이버 관리자 + 벤더 ODBC 드라이버 + DSN**(Windows 내장 · macOS/Linux는 unixODBC/iODBC 설치) | 벤더별 클라이언트 설정 파일(`tbdsn.tbr` 등) | ☐ 미구현 — Oracle 다음으로 "따로 설정"이 필요한 후보 |

**결론**: 지금 빌드에서 별도 설치·설정이 필요한 것은 **Oracle 하나**다. 앞으로 같은 틀이 필요한 것은 ODBC 계열(드라이버 관리자·DSN 탐지)이고, 순수 Rust 드라이버(SQL Server · PostgreSQL · MySQL)는 네트워크 보안 옵션(TLS 인증서 · 통합 인증)을 넣을 때 그 DBMS의 분류에 키가 생긴다.

## 2. 설정 ▸ DBMS 그룹 — 자동 탐지 / 직접 지정 / 읽기 전용 파생 정보

```
DBMS
 ├ Oracle      oracle.client_mode(auto|manual) · oracle.client_dir · oracle.tns_admin      ← 편집(뒤의 둘은 manual일 때만 · DEPENDS)
 │             (판단 근거 = 위 두 폴더 항목의 설명 아래 한 줄 · `<키>#note`) · oracle.info_library · info_version   ← 읽기 전용(INFO_KEYS)
 │             oracle.info_tnsnames · info_sqlnet                                          ← 읽기 전용
 │             (09-21: info_client_dir · info_tns_admin 제거 — 자동 방식에서 위의 입력 칸이 같은 경로를 보여 준다 · 설명은 "목적: …")
 │             oracle.live.*(실행 중 로그 — 접속 분류에서 옮김)
 ├ SQL Server  mssql.encrypt · mssql.cancel(옮김) · mssql.info_driver(읽기 전용 안내)
 ├ PostgreSQL  pg.refcursor_expand(옮김) · pg.info_driver
 └ SQLite      sqlite.info_driver
```

### 2-1. 규칙
- **자동**(기본): 지금 환경에서 찾는다 — `NSQL_ORACLE_CLIENT_DIR` → `ORACLE_HOME`(Windows `bin` · 그 밖 `lib` · 그 폴더 자체) → OS 라이브러리 검색 경로(Windows `PATH` · Linux `LD_LIBRARY_PATH` · macOS `DYLD_LIBRARY_PATH`) → 잘 알려진 설치 자리(Windows `C:\oracle\instantclient*` · `C:\instantclient*` · macOS `~/lib` · `/usr/local/lib` · `/opt/homebrew/lib` · `~/Downloads/instantclient*` · Linux `/opt/oracle/instantclient*` · `/usr/lib/oracle/<버전>/client64/lib`). 찾은 결과는 **읽기 전용 칸**으로 보이고 폴더·TNS_ADMIN 입력란은 잠긴다.
- **직접 지정**: 폴더와 `TNS_ADMIN`만 편집할 수 있다. **그 폴더만 쓴다** — 라이브러리가 없으면 다른 클라이언트를 조용히 쓰지 않고 접속이 실패한다(DPI-1047 · 실서버 테스트가 이 결함을 잡았다: 처음 구현은 PATH의 클라이언트로 넘어갔다). 거기서 정해지는 라이브러리 파일 · `tnsnames.ora`(별칭 수와 앞 8개) · `sqlnet.ora` 경로는 읽기 전용.
- `TNS_ADMIN`: 설정(직접 지정) → 환경 변수 `TNS_ADMIN` → `<클라이언트 폴더>/network/admin` → `ORACLE_HOME/network/admin`. 환경 변수로 정해진 것은 OCI가 스스로 읽으므로 넘기지 않고, 설정·클라이언트 폴더에서 정해진 것만 ODPI-C에 넘긴다.
- **보인 것 = 로드되는 것**: 탐지가 찾은 폴더를 ODPI-C 초기화(`oracle_client_lib_dir`)에 그대로 넘긴다(OS 검색 경로에서 찾은 경우만 종전처럼 ODPI-C의 기본 탐색).
- **ODPI-C는 프로세스에서 한 번만 초기화된다** → 첫 Oracle 접속 뒤에 바꾼 값은 앱을 다시 시작한 뒤 적용된다. "로드된 클라이언트 버전"은 첫 접속 뒤에만 나온다(물어보려고 라이브러리를 로드하지 않는다 — 300 MB짜리를 설정 창 때문에 올리지 않는다).
- 탐지는 **파일 시스템만** 읽는다(네트워크 0 · 라이브러리 로드 0 · 설정 창을 열 때와 Oracle 키가 바뀔 때만).
- CLI도 같은 설정을 쓴다(`nsql run`/`shell` — 첫 접속 전에 넘긴다).

- **폴더 칸**(09-21 추가): 직접 지정 = 입력란에 **직접 입력**하거나 **"찾아보기…"**(폴더 전용 대화상자 — 파일은 보이지 않는다 · nexa-dlg `PickerMode::Folder`)로 고른다 · 자동 = 그 칸에 **탐지된 경로가 수정 불가(흐림)**로 보이고 없으면 공백(저장값은 그대로 — 직접 지정으로 돌아오면 다시 보인다).

- **파일 칸은 존재 여부만**(09-21): 라이브러리 파일 · `tnsnames.ora` · `sqlnet.ora` = `있음 — 경로` / `없음 — 어디에 무엇이 없는지`(자동·직접 지정 공통) · 갱신 버튼은 없다 — 설정 창이 다시 활성화될 때 스스로 다시 본다(외부 편집 반영 · 폴링 0).

### 2-2. 구현
`nsql-driver-oracle/src/client.rs`(`detect_with` — 환경 변수 읽기를 주입받아 테스트가 가짜 환경·임시 폴더로 돈다 · `tns_aliases`) · 드라이버 `set_client_config`/`loaded_version`/`init_client` · `nsql-drivers::{oracle_client_info, set_oracle_client}`(값만 — 문구는 호스트가 `Msg`로) · `nsql-settings` `INFO_KEYS`/`is_info`(저장하지 않는 계산 값 · `set` 거부) + `DEPENDS`(`Eq("manual")`) + `CATEGORY_TREE`의 `GrpDbms` · `prefs_win.rs` `set_info`(정보 카드는 늘 잠김 · 경로 카드는 입력란을 카드 폭만큼) · 통합 테스트 `tests/oracle_client_manual.rs`(별도 프로세스 — 틀린 폴더 = 실패 · 맞는 폴더 = 접속 · 버전).

**새 DBMS에 같은 틀을 붙이는 법**: ① 드라이버에 `detect`(파일 시스템만) ② `nsql-drivers`에 값 구조체 ③ 설정에 `<dbms>.…_mode` + 편집 키(`DEPENDS`) + `<dbms>.info_*`(`INFO_KEYS`) ④ 호스트 `dbms_info_values`에 문구 ⑤ `CATEGORY_TREE`의 DBMS 그룹에 분류 하나.

## 3. 드라이버: 내장(Native) ↔ 확장(Extension) — 기술 검토

### 3-1. 실측(Windows · Release · 이 PC)

**실행 파일 크기에 드라이버가 기여하는 몫**(CLI `nsql.exe` · Release · `nsql-drivers`의 Cargo feature 조합을 바꿔 가며 빌드 · 09-21):

| 조합 | `nsql.exe` | SQLite만 대비 |
|---|---|---|
| SQLite만 | 4.43 MB | — |
| + Oracle | 4.77 MB | **+0.34 MB**(ODPI-C 바인딩 — 무게는 앱 밖의 Instant Client 407 MB) |
| + PostgreSQL | 5.05 MB | **+0.63 MB** |
| + SQL Server | 5.80 MB | **+1.37 MB**(tiberius + tokio + rustls) |
| + SQL Server + PostgreSQL | 6.35 MB | +1.92 MB |
| 전부(지금 배포) | 6.69 MB | **+2.26 MB** |

→ 네 드라이버를 전부 빼도 줄어드는 것은 **2.3 MB**(GUI 실행 파일 10.3 MB 기준 약 22 %)다. 확장으로 빼면 공유하던 의존성(tokio · TLS)이 드라이버마다 복제돼 **합계는 오히려 커진다**.


| 항목 | 값 | 출처 |
|---|---|---|
| GUI 기동 상주(드라이버 4종 링크) | 11.7 MB | [26 §7-5](26-performance-architecture.md) |
| SQLite 접속 뒤 | 10.1 MB | 같은 표 |
| 기동(창이 보이기까지) | ≈ 155 ms | 같은 표 |
| 드라이버 전역 초기화 | 0 — tokio 런타임(SQL Server) · ODPI-C(Oracle)는 **첫 접속 때** 만들어진다 | 22 §0 |

### 3-2. 무엇이 좋아지고 무엇이 그대로인가

| 관점 | 내장(정적 링크 · 지금) | 확장(필요할 때 내려받는 동적 라이브러리) | 판정 |
|---|---|---|---|
| **기동 속도** | 쓰지 않는 드라이버 코드는 실행되지 않는다(전역 초기화 0) | 같다 — 기동 때 아무것도 로드하지 않으면 | 차이 없음 |
| **상주 메모리** | OS 요구 페이징 — 쓰지 않는 코드 페이지는 RAM에 올라오지 않는다 | 로드한 것만 | 차이 없음(측정값이 말한다: 4종을 링크하고도 11.7 MB) |
| **쿼리 속도** | 직접 호출 | cdylib = 직접 호출(값 코덱 1회 복사) · 프로세스(RPC) = 왕복마다 직렬화 + 프로세스 경계 | 내장 ≥ cdylib ≫ 프로세스 |
| **설치 용량** | 실행 파일 하나에 전부 | 본체가 작아지고 쓰는 것만 받는다 | **확장이 이긴다** — 단, 크기의 대부분은 드라이버가 아니라 벤더 클라이언트(Oracle 407 MB)다 |
| **갱신** | 드라이버 하나 고쳐도 앱 전체 배포 | 드라이버만 교체 | 확장이 이긴다 |
| **여러 버전 나란히** | 불가 | 가능 — 단 Oracle은 **한 프로세스에 클라이언트 하나**라 프로세스 격리가 있어야 한다 | 확장(프로세스 모드) |
| **라이선스 격리** | 본체와 한 몸 | GPL·벤더 EULA 드라이버를 본체와 분리 | 확장 |
| **안정성** | 드라이버 패닉 = 앱 종료(지금은 없음) | cdylib = 같다(`catch_unwind`로 줄임) · 프로세스 = 격리 | 프로세스 모드만 이긴다 |
| **보안** | 서명된 본체 하나 | 내려받은 코드를 앱 권한으로 실행 → 서명 검증 · 색인 고정이 필수 | 내장이 단순 |
| **개발·시험 비용** | 포트 하나(`Session`) · CI 한 벌 | + 버전 있는 C ABI(`nsql-abi`) · 값 코덱 · 3-OS × 2-아키텍처 × 드라이버별 빌드·서명·호환성 표 | 내장이 훨씬 싸다 |
| **첫 사용 경험** | 설치 즉시 접속 | 첫 접속 전에 다운로드(오프라인·사내망에서 막힌다) | 내장 |

### 3-3. 결론 — "전부 확장"도 "전부 내장"도 아니다

1. **순수 Rust 드라이버(SQLite · SQL Server · PostgreSQL · 앞으로 MySQL)는 내장을 유지한다.** 확장으로 빼서 얻는 것은 설치 용량뿐이고(기동·메모리·속도는 그대로 — §3-2), 그 용량도 메가바이트 단위다(§3-1 표). 대가로 ABI·코덱·빌드 매트릭스·서명·오프라인 설치 문제가 생긴다. **DR-29 유지.**
2. **Oracle도 드라이버 코드는 내장을 유지한다.** 무거운 것은 우리 코드가 아니라 Instant Client(407 MB)이고, 그것은 이미 앱 밖에 있다 → 이번에 넣은 것처럼 **"찾기 · 지정"을 제품 기능으로** 만드는 쪽이 사용자에게 실제로 도움이 된다. 다음 단계 = Instant Client **내려받기 도우미**(Oracle 배포 조건 확인 뒤 — 재배포 가능 여부가 관건 · 아니면 공식 내려받기 페이지로 안내).
3. **확장으로 가야 하는 것은 따로 있다**(22 §2-1의 ⓐ~ⓔ): 벤더 ODBC가 필요한 DBMS(Tibero · Altibase · CUBRID) · GPL 등 라이선스를 섞을 수 없는 드라이버 · 서드파티가 만드는 드라이버 · **Oracle 클라이언트 여러 버전을 나란히** 써야 하는 경우(프로세스 모드). 이들은 본체에 넣을 수 없거나 넣기 싫은 것이라 확장이 유일한 길이다.
4. **용량이 정말 문제라면 확장이 아니라 빌드 구성으로 푼다**: 드라이버는 이미 Cargo feature다(`nsql-drivers` — `sqlite`·`oracle`·`mssql`·`pg`). 배포판을 둘로 — **표준판**(전부) · **경량판**(SQLite + 고른 것) — 내는 데는 코드 변경이 없다(CI 매트릭스 한 줄). 확장 시스템 없이 "필요한 것만"을 얻는다.

### 3-4. 확장 드라이버를 만들 때의 설계(22 §0의 구체화 — 지금 하지 않는다 · 순서만 고정)

```
nsql-core::Session(포트) ──┬── 내장 어댑터(정적 링크)                         ← 지금
                           ├── nsql-driver-dyn(프록시 세션) ── dlopen ── <드라이버>.dll/.so/.dylib   ← 확장 1순위(in-process · 지연 로드)
                           └── nsql-driver-rpc(프록시 세션) ── 프로세스 ── 같은 ABI를 감싼 호스트    ← 격리·SxS가 필요할 때만
```
- **ABI**: `nsql-abi` = 버전 있는 C vtable(`abi_version` · `open` · `execute` · `fetch_next` · `commit` · `rollback` · `set_option` · `cancel` · `caps` · `close`) + `Value`/`ExecRequest`/`ExecResult`의 바이너리 코덱(길이 접두 · 복사 1회). Rust ABI는 쓰지 않는다(버전 간 불안정). **`Caps`(T-152)가 이미 값 타입**이라 그대로 실린다 — 확장 드라이버는 능력표만 채우면 엔진·러너를 고치지 않고 붙는다.
- **지연 로드**: 첫 접속 때 `LoadLibrary`/`dlopen`(외부 crate 0) · 기동 때는 색인(JSON)만 읽는다 → 기동·상주 메모리에 영향 0.
- **배포·보안**: GitHub Releases 색인 + SHA-256 + Ed25519 서명(본체에 공개키) · 서명 없는 드라이버는 "개발자 모드"에서만 · 사내망용 **오프라인 설치**(폴더 지정 · `NSQL_DRIVER_DIR`).
- **설정 연동**: 확장 드라이버도 §2의 틀(자동 탐지 / 직접 지정 / 읽기 전용 정보)을 그대로 쓴다 — 드라이버가 `detect()`를 ABI로 내보내고 설정 창의 DBMS 그룹에 분류가 생긴다(확장 분류가 설치되면 보이고 제거하면 사라지는 기존 규칙 · `EXTENSION_CATEGORIES`).
- **순서**: ① `nsql-abi` + 코덱(내장 드라이버로 왕복 테스트) ② SQLite를 시험 삼아 cdylib로도 빌드해 프록시 세션 검증(성능 비교 = 정적 대비 호출 지연) ③ 첫 실제 확장 = ODBC ④ 프로세스 모드(Oracle SxS).

### 3-5. 위험·열린 질문
- Instant Client 재배포 조건(OTN 라이선스) — 내려받기 도우미를 만들기 전에 확인(D-143 후보).
- macOS: 내려받은 동적 라이브러리의 격리 속성(quarantine)·공증 — 서명만으로는 로드가 막힐 수 있다 → 확장 드라이버는 macOS에서 먼저 시험.
- cdylib에서 `tokio` 런타임·TLS 스택이 드라이버마다 중복된다 → 확장 드라이버 하나의 크기가 내장일 때의 기여분보다 커진다(공유하던 의존성이 복제된다). "용량 절감" 기대치를 낮춰 잡을 것.

## 4. 경량 배포판 — 정의 · Instant Client 없이 가능한가(사용자 09-21)

### 4-1. 정의
**경량 배포판(Lite)** = "내려받아 실행하면 **다른 것을 설치하지 않고** 접속된다"를 보장하는 배포판. 기준 셋:
1. **외부 네이티브 의존 0** — 실행 파일이 벤더 클라이언트·런타임·ODBC 드라이버 관리자를 요구하지 않는다(들어 있는 드라이버는 전부 순수 Rust이거나 앱에 컴파일돼 있다).
2. **단일 실행 파일** — 설치본이든 압축 파일이든 드라이버 때문에 따라오는 파일이 없다.
3. **같은 기능 · 같은 설정 파일** — 표준판과 코드·설정·프로필 형식이 같고 Cargo feature 조합만 다르다(설정 창의 DBMS 그룹은 들어 있지 않은 드라이버를 "이 빌드에는 드라이버가 들어 있지 않습니다"로 보여 준다 — 이미 구현).

| 배포판 | 들어 있는 드라이버 | 외부 의존 | `nsql.exe` 크기(실측) |
|---|---|---|---|
| **표준판**(지금) | SQLite · SQL Server · PostgreSQL · **Oracle(OCI)** | Oracle에 접속할 때만 Instant Client | 6.69 MB |
| **경량판 A — "클라이언트 없는 판"** | SQLite · SQL Server · PostgreSQL | **없음** | 6.35 MB |
| 경량판 B — 최소 | SQLite + 고른 하나 | 없음 | 5.05~5.80 MB |
| **경량판 C — Oracle thin 포함**(목표) | A + Oracle **thin**(순수 Rust) | **없음** | A + 드라이버 몫(미측정) |

### 4-2. Instant Client 없이 Oracle에 접속할 수 있는가
- **지금 드라이버로는 불가**: `oracle` 크레이트 = ODPI-C = OCI를 부르는 얇은 층이다. ODPI-C에는 thin 모드가 없다(python-oracledb·node-oracledb의 thin은 각 언어로 프로토콜을 다시 구현한 것).
- **가능한 길 = Oracle 공식 순수 Rust 드라이버 `oracledb`**(crates.io · Oracle 유지 · "No Oracle Client libraries required" · 지원 서버 12·18·19·21·26). 09-13 스파이크([22 §4-1](22-driver-extensions.md)): 사용자 19c에 Instant Client 없이 157 ms 접속 ✓ — 그러나 **OUT 바인드·REF CURSOR API가 없어** 세션 변수 엔진(DR-8)에 쓸 수 없었다(D-22). 09-21 확인: crates.io 최신 = 여전히 `26.0.0-beta.3`(같은 판) · 소스에 `out_bind_data`가 보여 **다시 스파이크할 가치가 있다**(T-159).
- 그래서 경량판의 단계: **지금 = A**(Oracle 제외 · 코드 변경 0 · CI 매트릭스 한 줄) → **thin이 OUT 바인드·REF CURSOR를 갖추면 C**. C에서도 OCI 드라이버는 표준판에 남긴다 — thin의 서버 하한(12c) 아래의 11g와 OCI 전용 기능(지갑·Kerberos·`sqlnet.ora` 고급 옵션) 때문이다. 두 드라이버는 같은 `Dialect::Oracle` · 같은 `Caps`를 쓰고 설정 `oracle.driver = oci|thin`(표준판에서만 뜻이 있다)으로 고른다.

## 5. 드라이버의 동적 적재·해제(Dynamic Module Loading) 검토 — "쓸 때만 메모리에 · 접속을 끊으면 내린다"(사용자 09-21)

### 5-1. 실측 — 접속하고 끊을 때 프로세스에 무엇이 남는가
CLI `nsql shell`(Release) 한 프로세스에서 SQLite 기준 → `CONNECT <프로필>` → 조회 1회 → `DISCONNECT` → 7초 대기. Private = 그 프로세스만의 메모리 · 이미지 = 매핑된 실행 모듈 합(파일 기반 — 쓰는 쪽만 RAM에 올라온다).

| 단계 | Oracle(OCI · Instant Client 23.9) | SQL Server | PostgreSQL |
|---|---|---|---|
| 접속 전 | Private 1.3 MB · 모듈 21 · 이미지 25 MB | 1.3 MB · 21 · 25 MB | 1.4 MB · 21 · 25 MB |
| 접속 뒤 | **10.5 MB · 모듈 57 · 이미지 355 MB** | 1.5 MB · 23 · 26 MB | 1.5 MB · 23 · 26 MB |
| 조회 뒤 | 10.5 MB | 1.5 MB | 1.5 MB |
| **해제 뒤** | **9.4 MB · 모듈 57 · 이미지 355 MB**(그대로) | 1.5 MB | 1.4 MB |

- **순수 Rust 드라이버는 내릴 것이 없다**: 접속해도 +0.1~0.2 MB이고 끊으면 세션이 drop되며 돌려준다. 동적 라이브러리로 빼서 `FreeLibrary` 해도 얻는 것은 **0.2 MB 미만** — 반대로 드라이버마다 tokio·TLS가 복제돼 디스크와 로드 시 메모리가 늘어난다.
- **Oracle만 남는다**: 첫 접속에서 Instant Client DLL 36개가 매핑되고(이미지 +330 MB · 실제 RAM은 워킹셋 +17 MB) Private +9.2 MB가 **접속을 끊어도 남는다**. 이것이 "해제"의 유일한 실익이다.

### 5-2. 왜 in-process로는 Oracle을 내릴 수 없는가
- ODPI-C는 Oracle 클라이언트를 **프로세스에서 한 번 로드하고 내리지 않는다**(`dpiContext`는 파괴할 수 있어도 OCI 라이브러리의 언로드는 지원하지 않는다 — OCI가 프로세스 전역 상태·스레드·`atexit` 처리기를 남긴다). 우리 드라이버를 cdylib로 빼서 그것을 `FreeLibrary` 해도 **그 밑의 `oci.dll`·`oraociei.dll`은 남는다**(강제로 내리면 다음 접속에서 죽는다).
- 일반론: Rust cdylib의 언로드는 위험하다 — 스레드 로컬 소멸자 · 백그라운드 스레드(tokio) · `static` 상태 · 패닉 훅이 언로드된 코드 주소를 가리키면 즉사한다. macOS는 `dlclose`가 사실상 아무것도 내리지 않는 경우가 많다(Objective-C·TLV가 있으면 영구 상주). **"로드는 지연, 언로드는 하지 않는다"가 업계 관례**다(DBeaver = JVM 클래스로더에 남김 · VS Code = 확장 호스트 **프로세스**를 죽여서 회수).

### 5-3. 결론 — 메모리를 실제로 돌려받는 길은 **프로세스 격리** 하나
| 방식 | 로드 시점 | 해제 뒤 메모리 | 호출 비용 | 판정 |
|---|---|---|---|---|
| 정적 링크(지금) | 요구 페이징 — 쓰는 코드만 | 순수 Rust = 회수됨 · Oracle = 9.4 MB 남음 | 0 | 순수 Rust 드라이버에 최적 |
| cdylib 지연 로드 + 언로드 | 첫 접속 | 순수 Rust = 이득 < 0.2 MB · Oracle = **여전히 남음** | 0 | 언로드 위험만 추가 — **하지 않는다** |
| **드라이버 호스트 프로세스**(`nsql-driver-host` — 접속이 0이 되면 종료) | 첫 접속(프로세스 기동 50~100 ms) | **전부 OS로 돌아간다**(Oracle 9.4 MB + 매핑 330 MB) | 결과 페치마다 직렬화 + 파이프(큰 결과에서 체감) | **Oracle 전용 선택 모드**로 가치 있음 |

권고: ① 순수 Rust 드라이버는 지금 그대로(동적 적재·해제 없음 — 이미 "쓸 때만" 올라오고 끊으면 돌려준다) ② Oracle은 **선택 모드 `oracle.isolation = process`**(기본 `in_process`)를 확장 드라이버 단계([§3-4](#3-4-확장-드라이버를-만들-때의-설계22-0의-구체화--지금-하지-않는다--순서만-고정)의 ④ 프로세스 모드)에서 만든다 — 같은 모드가 **메모리 회수 · 여러 버전의 Instant Client 나란히 · 드라이버 충돌 격리** 셋을 한 번에 해결한다 ③ 그 전까지의 값싼 개선: Oracle 접속이 0이 된 뒤 유휴 회수(`memtrim`)가 워킹셋을 줄이므로 **RAM 점유는 이미 작다**(남는 Private 9.4 MB가 전부) — 설정 창의 "로드된 클라이언트 버전" 설명에 "앱을 다시 시작하면 내려간다"를 이미 적었다.

