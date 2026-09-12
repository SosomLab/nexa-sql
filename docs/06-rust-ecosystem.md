# 06 · Rust 생태계 조사 — DB 드라이버 · GUI 프레임워크 · 배포

> 작성 2026-09-12 · 웹 조사(에이전트) · crates.io/docs.rs/GitHub 기준. 미확인 항목은 "확인 필요".
> ★ 이 문서의 결론이 [10 결정 기록](10-decision-record.md) DR-2(언어)·DR-4(드라이버 계층)·DR-5(GUI)의 근거다.

## Part A — DB 드라이버/크레이트

### A-1. Oracle

**결론: 2026년 현재 순수 Rust(pure-Rust) Oracle 드라이버가 존재하며, 그것도 Oracle 공식이다.** 2026-08 Oracle 드라이버 팀이 `rust-oracledb`(crates.io `oracledb`)를 베타로 공개했다.

| 항목 | `oracle` (kubo/rust-oracle) | `oracledb` (oracle/rust-oracledb, 공식) |
|---|---|---|
| 최신 버전/일자 | 0.6.3 / 2025-01-02 ([crates.io](https://crates.io/crates/oracle)) | 26.0.0-beta.3 / 2026-09-08 ([crates.io](https://crates.io/crates/oracledb)) |
| 저장소 상태 | 마지막 push 2025-03-23, open issues 28, ★229 ([GitHub](https://github.com/kubo/rust-oracle)) — 안정적이나 활동 둔화 | repo 생성 2026-07-20, 마지막 push 2026-09-11, open issues 6 ([GitHub](https://github.com/oracle/rust-oracledb)) — 매우 초기 |
| 구현 방식 | FFI: ODPI-C vendoring. ODPI-C는 `libclntsh`를 **런타임 dlopen** → 빌드 시 Oracle 헤더 불필요, 실행 시 Instant Client Basic/Basic Light 필요 ([ODPI-C](https://odpi-c.readthedocs.io/en/latest/user_guide/installation.html)) | 순수 Rust, Thin 모드 전용(TNS/TTC 직접 구현). Instant Client 불필요 ([공식](https://oracle.github.io/rust-oracledb/)) |
| TLS | Oracle Net(TCPS) — Instant Client가 처리, wallet | `rustls 0.23` 내장. mTLS wallet 범위 확인 필요 |
| 비동기 | 동기 전용 → `spawn_blocking` | 동기 API. async 지원 확인 필요 |
| 바인드 변수 | 위치/이름 바인드, IN/OUT/IN-OUT | 지원 |
| REF CURSOR | `oracle::sql_type::RefCursor`, OUT 바인드 후 `stmt.bind_value(1)`로 회수. Implicit results 지원 ([docs.rs](https://docs.rs/oracle/latest/oracle/sql_type/struct.RefCursor.html)) | `Cursor` 구조체 존재. OUT REF CURSOR API 형태 확인 필요 |
| PL/SQL | 익명 블록·프로시저 호출 | "Full SQL and PL/SQL execution" |
| DBMS_OUTPUT | 전용 헬퍼 없음 → `ENABLE`/`GET_LINES` 직접 호출 | 확인 필요 |
| Statement cache | ODPI-C 캐시 | auto-tuning 캐시, result cache, compressed fetch |
| LOB | CLOB/BLOB/NCLOB 스트림 | `Lob` locator |
| Pool | `oracle::pool::Pool` | Pool + DRCP |
| 특이 | — | **Arrow 배열 fetch/insert**(`arrow-array 59` optional), JSON, VECTOR |
| 지원 DB | Client 11.2+ | DB 12, 18, 19, 21, 26 |
| 라이선스 | UPL-1.0 OR Apache-2.0 | UPL-1.0 OR Apache-2.0 |
| 갭 | Instant Client 배포 부담, 동기 전용 | 베타(API 변경 가능), docs 47%, OCI 전용 기능(AQ 등) 없음 |

기타 순수 Rust 시도: `oracle-rs`(stiang, ★26, [GitHub](https://github.com/stiang/oracle-rs)), `MuhDur/rust-oracledb`(python-oracledb thin 포팅, WIP), `sibyl`(OCI 기반, blocking/async, [GitHub](https://github.com/quietboil/sibyl)).
**실무 권고**: 당장은 kubo `oracle`(성숙, REF CURSOR/LOB/풀 완비)로 시작하고, 공식 `oracledb`가 GA되면 교체할 수 있도록 **드라이버 추상 레이어**를 둔다. Instant Client는 macOS ARM64(19.x, 23ai, 26ai — [Oracle 문서](https://docs.oracle.com/en/database/oracle/oracle-database/26/mxcli/index.html), [다운로드](https://www.oracle.com/database/technologies/instant-client/macos-arm64-downloads.html)), Windows x64(ARM64 확인 필요), Linux x64/aarch64 제공.

### A-2. SQL Server

- **`tiberius`** (prisma): crates.io 최신 0.12.3 / 2024-07-19. 저장소는 2026-09-04 rustls 0.23·`azure_identity` 0.20 범프 등 **유지보수 재개**([GitHub](https://github.com/prisma/tiberius)). 순수 Rust TDS 7.2+, 런타임 독립, TLS feature 선택. 인증: SQL 로그인, **Windows SSPI(Windows 전용)**, **Kerberos(`integrated-auth-gssapi`, Unix libgssapi)**, **Entra ID는 `AuthMethod::AADToken`으로 외부 토큰 전달** ([docs.rs](https://docs.rs/tiberius/latest/tiberius/enum.AuthMethod.html)). MIT/Apache-2.0. ([05 §6](05-mssql-tools.md)의 `tiberius-ng` 포크도 참조.)
- **Microsoft 공식 `mssql-tds`**: 0.1.0 / 2026-09-10 crates.io(MIT, `microsoft/mssql-rs`) ([crates.io](https://crates.io/crates/mssql-tds)). 외부 PR 미수용. `mssql-tiberius-bridge`가 tiberius 호환 API 제공 ([crates.io](https://crates.io/crates/mssql-tiberius-bridge)).
- **`odbc-api`** 29.0.0 / 2026-07-19, MIT, 매우 활발 ([GitHub](https://github.com/pacman82/odbc-api)). **`arrow-odbc`** 25.3.0 — ODBC 결과 → Arrow `RecordBatch` ([crates.io](https://crates.io/crates/arrow-odbc)).

### A-3. PostgreSQL / MySQL / SQLite / DuckDB / ClickHouse / MongoDB / Redis / 범용

| 크레이트 | 최신 / 일자 | 구현 | 런타임 | TLS | 라이선스 | 비고 |
|---|---|---|---|---|---|---|
| `tokio-postgres` | 0.7.18 / 2026-06-12 | 순수 Rust | tokio(`postgres` 동기 래퍼) | native-tls/rustls/openssl | MIT/Apache | 사실상 표준 |
| `sqlx` | 0.9.0 / 2026-05-21 | PG/MySQL 순수, SQLite FFI | tokio/async-std | rustls/native-tls | MIT/Apache | **MSSQL은 0.7에서 제거(SQLx Pro)**. `Any`는 PG/MySQL/SQLite만 ([docs](https://docs.rs/sqlx/latest/sqlx/any/index.html)) |
| `mysql_async` | 0.37.1 / 2026-09-01 | 순수 Rust | tokio | 양쪽 | MIT/Apache | MariaDB 호환 |
| `rusqlite` | 0.40.2 / 2026-08-08 | FFI(`bundled`) | 동기 | n/a | MIT | 단일 바이너리 적합 |
| `duckdb` | 1.10505.0 / 2026-07-22 | FFI(C++ · bundled 빌드 큼) | 동기 | n/a | MIT | Arrow 출력 |
| `clickhouse` | 0.15.2 / 2026-08-28 | 순수(HTTP+RowBinary) | tokio | 양쪽 | MIT/Apache | 공식 |
| `mongodb` | 3.9.1 / 2026-09-10 | 순수 Rust | tokio(+sync) | rustls/openssl | Apache-2.0 | 공식 |
| `redis` | 1.7.0 / 2026-09-05 | 순수 Rust | 다중 | 양쪽 | BSD-3 | 1.x 안정 |
| `connectorx` | 0.4.5 / 2026-01-18, ★2,649 | 소스 sqlite/pg/mysql/mssql/**oracle(kubo 경유)** → arrow/polars | 동기+병렬 | — | MIT | Python 중심, 무거움 |
| `connector_arrow` | 0.12.2 / 2026-09-11 | sqlite/duckdb/pg/mysql/mssql → Arrow, **Oracle 미지원** | 동기 | — | MIT | 라이브러리 지향 |
| `arrow` | 59.3.0 / 2026-09-01 | — | — | — | Apache-2.0 | |

**"드라이버 매니저(download-on-demand)" 접근의 Rust 재현**
- DBeaver의 JDBC 온디맨드는 JVM 전제. `jni`/`j4rs`로 JDBC를 끌어오면 JRE 동봉(100MB+) → 경량 목표와 정면 충돌 → 비권고.
- **네이티브 C 클라이언트를 `libloading`으로 런타임 로드**하는 방식은 실현 가능하고 선례 있음: ODPI-C(→ `oracle`)가 정확히 이 방식이고, ODBC 드라이버 매니저 자체가 동적 로드.
- ★ 실용 설계: **1차 계층 = 순수 Rust 드라이버**(PG, MySQL, SQLite, DuckDB, ClickHouse, Mongo, Redis, SQL Server(tiberius/mssql-tds), Oracle(공식 thin GA 시)), **2차 계층 = ODBC 폴백(`odbc-api`)** 으로 Tibero/Altibase/CUBRID/기타. Oracle Instant Client는 "사용자 지정 경로 로드"로 배포 라이선스 부담 회피.

### A-4. 국산 DBMS (Tibero / Altibase / CUBRID)

- **Tibero**: `tbCLI`가 ODBC 3.51 기반, `libtbcli`가 ODBC 드라이버로 등록 가능 ([Tibero 문서](https://docs.tibero.com/tibero/en/topics/development/tbcli-guide/tbcli-and-odbc)). Rust 크레이트 없음 → **`odbc-api` 경유가 유일한 현실적 경로**.
- **Altibase**: ODBC, JDBC, ACI(C API) ([매뉴얼](https://github.com/ALTIBASE/Documents/blob/master/Manuals/Altibase_7.1/eng/API%20User's%20Manual.md)). Rust 없음 → ODBC.
- **CUBRID**: CCI([GitHub](https://github.com/CUBRID/cubrid-cci)), ODBC 3.52(CCI 기반), JDBC. `bindgen`으로 CCI 직접 바인딩 가능하나 별도 작업. Linux/macOS ODBC 범위 확인 필요.
- 요약: **세 DBMS 모두 ODBC(또는 CCI FFI)로만 가능**, 벤더 클라이언트는 사용자 설치.

### A-5. 통합 결과셋 추상화 — Arrow를 범용 인메모리 그리드 포맷으로

- 찬성: `arrow-odbc`, 공식 `oracledb` Arrow feature, `duckdb` Arrow 출력, `connector_arrow`가 이미 존재 → "모든 소스 → `RecordBatch`" 파이프라인의 절반 이상이 기성품. 컬럼형이라 10만 행 메모리 효율·정렬/필터(`arrow::compute`)·CSV/Parquet 내보내기가 따라옴. DataFusion을 붙이면 클라이언트측 SQL 재처리도 가능.
- 주의: (1) Oracle `NUMBER(38)`, `TIMESTAMP WITH TIME ZONE`, `INTERVAL`, SQL Server `DATETIMEOFFSET`/`MONEY`, DuckDB `HUGEINT`, MySQL `SET`/`ENUM`, LOB/REF CURSOR의 Arrow 매핑 규칙을 직접 정의해야 함. (2) "NULL과 원본 리터럴 문자열 보존"이 중요 → 편집 그리드는 Arrow 배치 + 오버레이(변경분 diff). (3) 스트리밍 fetch는 1k~10k행 배치 누적.
- ⚠️ **계열 규율(외부 crate 0 지향)과의 충돌**: `arrow`는 의존 트리가 크다(수십 크레이트). 결정은 D-2([10](10-decision-record.md)) — 자체 컬럼형 모델 vs Arrow.

## Part B — 데스크톱 SQL IDE용 GUI 프레임워크

| 프레임워크 | 최신 | 성숙도 | 텍스트 에디터 | 대형 그리드 | 도킹 | IME(한국어) | a11y | 바이너리/메모리 | 리스크 |
|---|---|---|---|---|---|---|---|---|---|
| **Tauri 2** | 2.11.5 / 2026-07-01 | 안정 | Monaco/CM6 최상급 | AG Grid/TanStack | dockview | 플랫폼 웹뷰 = 최상. Linux v2 IME 창 위치 버그([#11412](https://github.com/tauri-apps/tauri/issues/11412)) | 최상 | 번들 3–10MB, idle 40–80MB; Monaco+10만행이면 200–350MB(추정) | WebView2 의존, WebKitGTK 편차([discussion](https://github.com/orgs/tauri-apps/discussions/10026)) |
| **egui/eframe** | 0.36.2 / 2026-09-08 | 성숙 | `TextEdit`+layouter. 멀티커서/폴딩 없음 | `egui_table` 0.10 | `egui_dock` 0.21.1 | 0.35 IME 개선. **기본 폰트에 CJK 없음 → 한글 폰트 번들 필수**([2025 survey](https://www.boringcactus.com/2025/04/13/2025-survey-of-rust-gui-libraries.html)) | AccessKit 필수 | 10–20MB, RSS 수십 MB | 즉시 모드 복잡 에디터 상태 부담 |
| **iced** | 0.14.0 / 2025-12-07 | 활발(COSMIC) | `text_editor`+`iced_highlighter`(syntect) | 0.14 `table`/`grid` | `pane_grid` | **0.14에서 IME 최초 지원** → 한국어 실측 필요 | 사실상 없음 | 작음 | 릴리스 간격 1년+ |
| **Slint** | 1.17.1 / 2026-07-07 | 1.x 안정 | `TextEdit`만 | `StandardTableView` | 없음 | 조합 중 글리프 누락 보고 | 양호 | 작음 | DSL, 로열티프리는 `AboutSlint` 표시 의무([라이선스](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md)) |
| **GPUI** | 0.2.2 / 2025-10-22; `gpui-ce` 0.2.2 / 2026-08-28; `gpui-component` 0.6.1 / 2026-09-09 | Zed 1.15 Windows 정식(DX11 필수) | Zed 에디터가 증명(앱 강결합) | gpui-component Table | gpui-component Dock | Zed 실사용 수준. Windows 한국어 IME 전환키 이슈([#40335](https://github.com/zed-industries/zed/issues/40335)) | **없음** | 중간 | crates.io 릴리스 정체, pre-1.0 breaking, 문서 빈약 |
| **Dioxus** | 0.7.10 / 0.8 alpha | desktop = wry 웹뷰 | 웹뷰 | 웹 | 웹 | 웹뷰 | 웹뷰 | Tauri 유사 | Blitz 네이티브는 IME 미완([roadmap](https://github.com/DioxusLabs/blitz/issues/119)) |
| **Xilem/Masonry** | 0.4.0 / 2025-10-29 | **alpha** | Parley | 초기 | 없음 | 진행 중 | AccessKit | 작음 | 프로덕션 부적합 |
| **Makepad** | 1.0.0 / 2025-05-13 | 니치 | 자체 에디터 데모 | 자체 | 자체 | 조합창 구석 표시 | 취약 | 작음 | 셰이더 DSL |
| **Floem**(Lapce) | 0.2.0 / 2024-11-14, repo push 2026-06 | 정체 | Lapce 참고 | — | Lapce 패널 | Windows IME 미활성 보고(확인 필요) | 없음 | 작음 | 유지보수 리스크 |
| **Relm4/gtk4-rs** | 0.11.0 / 2026-04-08 | 안정 | GtkSourceView | GtkColumnView | libadwaita | GTK IME 우수 | Linux 우수 | GTK 런타임 동봉 | 3-OS 룩 불균일 |
| **cxx-qt** | 0.10.0 / 2026-08-24 | 안정 | QScintilla | QTableView 최상 | KDDockWidgets | 최상 | 최상 | Qt 런타임 큼 | 팀 기피, LGPL/상용 |

보조 근거: Rust 프로덕션 에디터 = Zed(GPUI), Lapce(Floem). TUI SQL 클라이언트 `rainfrog`(ratatui) 참고.

### 추천 매트릭스 (1–5, 5=최적)

| 우선순위 | Tauri 2 | egui | iced | Slint | GPUI |
|---|---|---|---|---|---|
| (1) 단일 경량 바이너리, RSS<150MB @10만행 | 2 | 5 | 5 | 5 | 4 |
| (2) 한국어 IME | 5 | 3 | 3 | 3 | 4 |
| (3) 최상급 SQL 에디터 | 5 | 2 | 2 | 1 | 4 |
| (4) 도킹 | 5 | 4 | 3 | 2 | 4 |
| (5) 3-OS 동등성 | 3 | 5 | 5 | 5 | 3 |
| (6) 팀 적합성(자체 래스터라이저·WebView 기피) | 3 | 5 | 4 | 3 | 4 |
| **합계** | 23 | 24 | 22 | 19 | 23 |

**에이전트 권고**: egui 1순위 · GPUI 2순위 · Tauri 2는 MVP 대안. 자체 코드 에디터 구현이 최대 리스크.
**★ 우리 판단([10 DR-5](10-decision-record.md))**: 팀은 이미 **egui에 해당하는 자체 층(`nexa-ui`: 래스터·컨트롤·토큰)을 보유**하고 3개 제품을 출시했다. 위 표에서 egui가 얻는 점수는 전부 `nexa-ui`도 얻고, "(3) 에디터 자체 구현"은 egui를 써도 똑같이 필요하다. 따라서 **프레임워크를 새로 들이지 않고 `nexa-ui`를 확장**한다. 부족한 것은 **텍스트 셰이핑**(한글 조합·CJK 폭)이며 이것만 크레이트 도입 대상(`rustybuzz`/`swash`/`cosmic-text` 비교 — D-3).

## Part C — 배포(Distribution)

- **Oracle Instant Client 재배포**: "OTN Development and Distribution License Terms for Instant Client" — **미수정 상태로 앱과 함께 재배포 허용**. 조건: 최종 사용자에게 동등 약관 동의, Oracle 고지 유지, 프로그램 자체 요금 부과 금지, Oracle 면책 ([Oracle 라이선스](https://www.oracle.com/downloads/licenses/instant-client-lic.html) — 조회 시 403, 원문 재확인 필요; [Cloudera 인용본](https://www.cloudera.com/legal/terms-and-conditions/oracle-instant-client-redistribution-license-agreement.html)). 실무: "설치 시 사용자가 직접 다운로드"(권장) 또는 EULA 동봉. 공식 `oracledb` thin GA 시 문제 소멸.
- **Microsoft ODBC Driver 18 재배포**: EULA 2.a.i REDIST 목록 배포 가능, 앱이 "significant primary functionality" 추가·면책 조건 ([LICENSE18](https://download.microsoft.com/download/1/7/c/17c9447c-dfa6-49e0-abdf-90095a85d986/odbc/eula18/LICENSE18.TXT)). tiberius/mssql-tds 사용 시 불필요.
- **코드 서명 비용**: Apple Developer 연 99 USD(공증 무료). Windows OV 연 219 USD~, EV 685 USD, 하드웨어 토큰 의무. **Azure Trusted Signing → 2026-01 "Azure Artifact Signing" GA**, Basic 월 9.99 USD ([Azure](https://azure.microsoft.com/en-us/products/artifact-signing)). **Public Trust 자격: 조직은 대한민국 포함 가능, 개인은 미국·캐나다만** ([FAQ](https://learn.microsoft.com/en-us/azure/artifact-signing/faq)) → SosomLab 법인 여부에 따라 결정(D-6). 계열 공통 T-48(clip)과 같은 문제.

## 핵심 요약
1. Oracle: **공식 순수 Rust thin 드라이버(`oracledb` 26.0.0-beta.3)** 등장 + 성숙한 ODPI-C 기반 `oracle` 0.6.3 현역. SQL Server: Microsoft 공식 `mssql-tds` 0.1.0(2026-09-10) 공개 + `tiberius` 유지보수 재개.
2. PG/MySQL/SQLite/DuckDB/ClickHouse/Mongo/Redis는 활발한 순수 Rust 드라이버. 국산 DBMS 3종은 ODBC 폴백만.
3. Arrow `RecordBatch`는 기술적으로 최적이나 의존 트리 비용이 크다(D-2).
4. GUI: 새 프레임워크 없이 **`nexa-ui` 확장** — 셰이핑만 도입(D-3).
5. 배포: 순수 Rust 드라이버 중심이면 Oracle/MS 클라이언트 재배포 문제 대부분 회피. Windows 서명은 법인이면 Azure Artifact Signing 월 9.99 USD.
