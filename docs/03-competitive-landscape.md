# 03 · 경쟁 제품 조사 ① — 크로스플랫폼 SQL 클라이언트 / DB IDE

> 작성 2026-09-12 · 웹 조사(에이전트) · 가격은 USD · 불확실 항목은 "확인 필요".
> 목적: nexa-sql(Rust · 경량 · 3-OS) 포지셔닝. Oracle 전용 도구는 [04](04-oracle-tools.md), SQL Server는 [05](05-mssql-tools.md).

## 1. 주요 데스크톱 클라이언트

### 1.1 DBeaver (Community / Lite / Enterprise / Ultimate / Team)
- 한줄 요약: JDBC 기반 "범용" DB 툴의 사실상 표준. 기능은 가장 넓지만 무겁다.
- 기술 스택: Java / Eclipse RCP(SWT). Community 26.2.0 (2026-08-30), Java 25 이상 필요, OpenJDK 번들 ([dbeaver.io/download](https://dbeaver.io/download/)).
- 지원 OS: Windows 10+, macOS 11+, Linux.
- 지원 DBMS: 기본 드라이버 100개 이상, JDBC/ODBC 드라이버가 있으면 사실상 모두 접속 가능. 드라이버는 Maven 저장소에서 자동 다운로드 ([github.com/dbeaver/dbeaver](https://github.com/dbeaver/dbeaver)).
- 라이선스·가격: Community는 Apache 2.0 무료. PRO는 Lite $113/년, Enterprise $255/년, Ultimate $510/년, Team $1,630/년. NoSQL·클라우드 인증·Schema Compare·AI 등은 PRO 전용 ([dbeaver.com/edition](https://dbeaver.com/edition/)).
- 강점: 메타데이터 편집기, 데이터 편집기(BLOB/CLOB 포함), ER 다이어그램, SQL 실행계획, 데이터 export/import/migration, SSH/프록시 터널, 커스텀 드라이버 편집기, 대시보드, 공간(GIS) 뷰어 ([dbeaver.com/features](https://dbeaver.com/features/)).
- 약점: JVM 기동 지연(수십 초~1분 보고), 유휴 메모리 340MB~600MB, 장시간 사용 시 2~5GB까지 증가 이슈 다수 (GitHub [#38117](https://github.com/dbeaver/dbeaver/issues/38117), [#39402](https://github.com/dbeaver/dbeaver/issues/39402), [#4590](https://github.com/dbeaver/dbeaver/issues/4590)). 기본 힙 -Xmx1024m을 수동 조정해야 하는 UX. Eclipse UI의 밀도·복잡도.
- 참고할 점: "드라이버를 앱과 분리해 온디맨드 다운로드"하는 Driver Manager 개념은 그대로 벤치마크할 가치가 있음(Rust에서는 정적 링크 드라이버 + 선택적 플러그인으로 대체). DBeaver의 약점(기동·메모리)이 곧 nexa-sql의 포지셔닝 근거.

### 1.2 JetBrains DataGrip
- 한줄 요약: IntelliJ 플랫폼 기반 "SQL IDE". 코드 인텔리전스가 최고 수준.
- 기술 스택: Java/Kotlin, IntelliJ Platform. 2026.1/2026.2 출시 ([whatsnew](https://www.jetbrains.com/datagrip/whatsnew/)).
- 지원 OS: Windows/macOS/Linux.
- 지원 DBMS: 50개 이상 (JDBC) ([features-overview](https://lp.jetbrains.com/features-overview/)).
- 라이선스·가격: 개인 $109/년(월 $10.90), 조직 $259/사용자/년, All Products Pack $289/년. 2025-10부터 비상업용 무료 티어 도입 보도 — 공식 페이지 재확인 필요 ([toolradar](https://toolradar.com/tools/datagrip/pricing), [trustradius](https://www.trustradius.com/products/jetbrains-datagrip/pricing)).
- 강점: 스키마 인식 자동완성·인스펙션·리팩터링, Schema Diff, Excel형 데이터 편집기, Live Template, 결과셋 비교, Local History + Git 통합. 2026.2에서 AI 에이전트용 skills(database-connection-management, text-to-sql, database-tools)와 MCP 툴 추가 ([whatsnew 2026.2](https://www.jetbrains.com/datagrip/whatsnew/)).
- 약점: 구독 전용, JVM 메모리 부담(대용량 결과셋에서 freeze 이슈 [DBE-14661](https://youtrack.jetbrains.com/issue/DBE-14661/DataGrip-using-way-too-much-memory), [DBE-17897](https://youtrack.jetbrains.com/issue/DBE-17897)), 첫 연결 시 introspection(스키마 인덱싱) 대기.
- 참고할 점: 세션 단위 트랜잭션 모드(auto/manual) 전환, 콘솔별 세션 분리, "결과셋 비교"는 차별 기능. AI 에이전트가 DB 툴을 쓰게 하는 MCP 노출은 2026년 트렌드.

### 1.3 TablePlus
- 한줄 요약: 네이티브 UI의 경량 상용 클라이언트. macOS에서 가장 빠르다는 평.
- 기술 스택: macOS는 Swift/Objective-C/C++, Windows는 C#/C++로 재작성, Linux는 beta/alpha ([tableplus.com/download](https://tableplus.com/download/), [amitmerchant](https://www.amitmerchant.com/tableplus-the-best-database-gui-for-linux/)). Electron/Qt 아님.
- 지원 OS: macOS 10.13+, Windows, Linux(beta), iOS.
- 지원 DBMS: MySQL, PostgreSQL, SQLite, SQL Server, Redshift, MariaDB, CockroachDB, Vertica, Redis, MongoDB, Oracle, Cassandra, BigQuery, ClickHouse, Turso 등 20+ ([tableplus.com](https://tableplus.com/)).
- 라이선스·가격: Basic $99 영구(1기기, 1년 업데이트, 갱신 $59), Standard $129(2기기), Team $79/seat(3석 이상) ([toolradar](https://toolradar.com/tools/tableplus/pricing)). 무료 버전은 탭/필터 개수 제한.
- 강점: 서브초 기동, 낮은 메모리, 인라인 편집, "Code Review"(변경분을 SQL로 미리보기 후 커밋), Safe Mode, 멀티탭/멀티윈도우, 플러그인(beta).
- 약점: 비오픈소스, Linux 완성도 낮음, ER 다이어그램·실행계획 시각화 등 IDE급 기능 부족, 플랫폼별 코드베이스 3개 유지 부담.
- 참고할 점: **변경 사항을 SQL 문으로 미리 보여주고 커밋(Ctrl+S)하는 UX**와 Safe Mode는 nexa-sql의 데이터 편집기에 그대로 채택할 가치가 큼. "경량 네이티브"의 시장 수요 증명. ★ 플랫폼별 코드베이스 3개라는 비용이 곧 "한 벌의 Rust 렌더러"의 근거.

### 1.4 Beekeeper Studio
- 한줄 요약: 가장 인기 있는 Electron 오픈소스 클라이언트(22k+ stars), GPL + 상용 이중 구조.
- 기술 스택: TypeScript / Electron / Vue. 최신 v6.0.5 (2026-08-24), v6.1 beta 진행 ([releases](https://github.com/beekeeper-studio/beekeeper-studio/releases)).
- 지원 OS: Windows/macOS/Linux.
- 지원 DBMS: PostgreSQL, MySQL, SQLite, SQL Server, Redshift, CockroachDB, MariaDB, TiDB, BigQuery, Redis, Oracle, Cassandra, ScyllaDB, Firebird, LibSQL/Turso, ClickHouse, DuckDB, SQL Anywhere, MongoDB, Trino, SurrealDB, DynamoDB, Snowflake 등 24종 ([pricing](https://www.beekeeperstudio.io/pricing/)).
- 라이선스·가격: Community는 GPLv3, Ultimate는 상용 EULA ([ultimate-and-gpl](https://www.beekeeperstudio.io/blog/ultimate-and-gpl)). Indie $9/월(연간), Pro $14, Business $18 — 구독 기간 내 버전 영구 사용권.
- 강점: 깔끔한 UX, SSH 터널, 쿼리 히스토리, Vim 모드, 플러그인, AI Shell, 6.1에서 read-only 모드·폴더가 무료화.
- 약점: Electron(기동 2~3초, 메모리 수백 MB), 일부 DB 엔진·인증(Kerberos, IAM)이 유료 게이트.
- 참고할 점: "코어 GPL + 확장 상용" 모델이 개인 개발자 시장에서 작동함을 입증. 문서화된 플러그인 API 존재.

### 1.5 DbGate
- 한줄 요약: 데스크톱+웹(Docker) 겸용 오픈소스 클라이언트. SQL/NoSQL 통합.
- 기술 스택: Electron / Svelte / Node.js. v7.2.6 (2026-09-01) ([dbgate.io](https://www.dbgate.io/)).
- 지원 OS: Windows/macOS/Linux + 브라우저(Docker).
- 지원 DBMS: MySQL, PostgreSQL, SQL Server, Oracle, SQLite, MariaDB, CockroachDB, ClickHouse, Firebird, DuckDB, MongoDB, Redis, Cassandra + Premium 전용(Redshift, CosmosDB, Firestore, libSQL/Turso, DynamoDB) ([github](https://github.com/dbgate/dbgate)).
- 라이선스·가격: 저장소 LICENSE는 GPL-3.0 표기(과거 MIT였던 것으로 알려짐 — 전환 시점 확인 필요). Premium $120/년, Team Premium $150/사용자/년.
- 강점: ER 다이어그램, 차트, 쿼리 디자이너, 데이터 비교/배포, 매크로, Grid grouping, 플러그인(테마·드라이버).
- 약점: Electron, UI 완성도는 Beekeeper/TablePlus 대비 투박하다는 평.
- 참고할 점: 데스크톱과 웹이 같은 코드베이스 — nexa-sql이 나중에 서버 모드(headless core + web UI)를 고려한다면 아키텍처 참고.

### 1.6 Azure Data Studio (퇴역)
- 상태: 2025-02-06 퇴역 발표, **2026-02-28 지원 종료** — 더 이상 업데이트·보안 패치 없음 ([learn.microsoft.com](https://learn.microsoft.com/en-us/sql/tools/whats-happening-azure-data-studio?view=sql-server-ver17)).
- 대체: VS Code + MSSQL 확장(Schema Compare, Schema Designer, Query Profiler, DACPAC/BACPAC, SQL Notebooks, Copilot). SQL Agent 등 관리 기능은 SSMS(Windows 전용). PostgreSQL은 별도 VS Code 확장, MySQL은 "발표 예정".
- 참고할 점: Electron 기반 자체 앱을 버리고 에디터 확장으로 통합한 사례. 범용 IDE 확장이 잡지 못하는 영역(멀티 DBMS, 관리 도구, 경량)이 독립 클라이언트의 생존 공간.

## 2. 기타 데스크톱/TUI/CLI 도구

| 도구 | 기술 | OS | DBMS | 라이선스/가격 | 비고 |
|---|---|---|---|---|---|
| **Navicat Premium 17** | C++/Qt (공식 명시 아님, 업계 통설 — 확인 필요) | Win/mac/Linux | MySQL, PostgreSQL, SQL Server, Oracle, MariaDB, SQLite, MongoDB, Redis, Snowflake + GaussDB/OceanBase | 영구 $1,599, 구독 $69.99/월 ([navicat.com](https://www.navicat.com/en/products/navicat-premium)) | 데이터 모델링·동기화·스케줄러 강력, 고가 |
| **Valentina Studio** | C++/Qt (wxWidgets에서 v6에 이전) ([blog](https://www.valentina-db.net/2015/05/02/valentina-6-first-look-betas-available/)) | Win/mac/Linux | MySQL, MariaDB, PostgreSQL, SQL Server, SQLite, MongoDB, ValentinaDB | Free / Single $79 / Pro / Universal (Pro 가격 확인 필요) | 리포트·폼 디자이너, 다이어그램 |
| **HeidiSQL** | Delphi 12.3 (Win), Lazarus/FreePascal 4.4으로 Linux·macOS **네이티브 빌드** 제공, v12.21 (2026-08-03), v13 preview ([heidisql.com/download](https://www.heidisql.com/download.php)) | Win, Linux/mac(신규 네이티브 + Wine/Snap) | MariaDB, MySQL, SQL Server, PostgreSQL, SQLite, Interbase/Firebird, ProxySQL, Redshift | GPL-2.0 무료 | 가볍고 빠름, MySQL 계열 최적 |
| **DbVisualizer** | Java/Swing, v26.2.2 ([dbvis.com/pricing](https://www.dbvis.com/pricing/)) | Win/mac/Linux | JDBC 50+ | Free / Pro $199 첫해, 갱신 $89 (영구 사용권) | 실행계획 시각화, 쿼리 빌더는 Pro |
| **SQuirreL SQL** | Java/Swing, 5.1.0 snapshot (2026-07) ([github](https://github.com/squirrel-sql-client/squirrel-sql-stable-releases/releases)) | Win/mac/Linux | JDBC 전부 | LGPL | 구식 UI, 플러그인 구조 |
| **Sqlectron** | Electron/React, v1.39.0 (2024-12-03) ([releases](https://github.com/sqlectron/sqlectron/releases)) | Win/mac/Linux | PostgreSQL, MySQL, SQL Server, SQLite, Cassandra | MIT | 사실상 저활동, core 저장소 archive |
| **Antares SQL** | Electron/Vue/TS ([github](https://github.com/antares-sql/antares)) | Win/mac/Linux(x64·ARM) | MySQL/MariaDB, PostgreSQL, SQLite, Firebird, DuckDB, SQL Server | MIT, "영원히 무료" | SSH, 히스토리 1,000건, 가짜 데이터 생성 |
| **Sequel Ace** | Objective-C→Swift 이전 중, v5.4.0 (2026-08-17), macOS 12+ ([github](https://github.com/Sequel-Ace/Sequel-Ace)) | macOS 전용 | MySQL/MariaDB | MIT | 크로스플랫폼 아님 |
| **Harlequin** | Python/Textual TUI ([harlequin.sh](https://harlequin.sh/)) | 터미널 | DuckDB 기본 + 어댑터(PostgreSQL, MySQL, SQLite, Snowflake, BigQuery…) | MIT | 카탈로그 트리, 히스토리, pip 설치 |
| **usql** | Go CLI, 2026-03 릴리스 ([github/xo/usql](https://github.com/xo/usql/releases)) | 터미널 | 빌드 태그로 수십 종 | MIT | psql 호환 명령·변수·백틱 |
| **lazysql** | Go/tview TUI, v0.5.6 (2026-08-22) ([github](https://github.com/jorgerojas26/lazysql)) | 터미널 | MySQL, PostgreSQL, SQLite, MSSQL, MongoDB | MIT, 4.3k★ | Vim 키, TOML 키바인딩 |
| **rainfrog** | Rust/ratatui TUI, v0.4.5 (2026-08-25) ([github](https://github.com/achristmascarl/rainfrog)) | 터미널(Android termux 포함) | PostgreSQL(1티어), MySQL, SQLite, Redshift, DuckDB, Oracle(실험) | MIT, 5.3k★ | 드라이버 크레이트: sqlx 기반으로 알려짐(README 미명시, 확인 필요) |

**웹/거버넌스 계열(직접 경쟁 아님)**: Bytebase(Go+Vue, MIT, 14.5k★, 변경 관리·접근제어 플랫폼 [github](https://github.com/bytebase/bytebase)), QueryPie(한국, DB 접근제어 프록시 ACP/DAC, CE 5인 무료 [querypie.com](https://www.querypie.com/products/database-access-controller)), Outerbase(2025-04 Cloudflare 인수, 클라우드 2025-10-15 종료, Outerbase Studio(구 LibSQL Studio)는 오픈소스 유지 [cloudflare blog](https://blog.cloudflare.com/cloudflare-acquires-outerbase-database-dx/)), Slashbase(Go+Next.js, beta v0.10, 정체 [github](https://github.com/slashbase/slashbase)), Drizzle Studio(`npx drizzle-kit studio` 로컬 웹 UI + 상용 Drizzle Gateway, 비공개 소스 [pkgpulse](https://www.pkgpulse.com/guides/drizzle-studio-vs-prisma-studio-vs-dbgate-2026)), Chat2DB(Java Spring Boot 백엔드 + React, 28k★, 40+ DB, 5.3.0부터 Apache 2.0 + 추가 조건의 source-available [github](https://github.com/OtterMind/Chat2DB)), DBConvert Studio(Windows 마이그레이션/동기화 툴, $599/년 — SQL 클라이언트 아님 [dbconvert.com](https://dbconvert.com/dbconvert-studio/)), Kangaroo(비공개 소스, v9.8.1, 프레임워크 미공개 — 확인 필요 [github](https://github.com/dbkangaroo/kangaroo)).

## 3. Rust / Tauri 기반 신규 클라이언트 (2023–2026)

| 프로젝트 | UI | Rust 드라이버 | DBMS | 라이선스 | 규모/비고 |
|---|---|---|---|---|---|
| **Tabularis** ([github](https://github.com/TabularisDB/tabularis)) | Tauri v2 + React 19 | 내장: sqlx(PostgreSQL/MySQL/SQLite). 그 외 16종은 **stdin/stdout JSON-RPC 2.0 플러그인**(언어 무관) | PG, MySQL, SQLite + ClickHouse, DuckDB, Mongo, Redis, BigQuery, Oracle, SQL Server(개발 중)… | Apache 2.0 | 4.9k★, v0.23.0. MCP 서버 내장, SQL 노트북, Visual EXPLAIN, 비주얼 쿼리빌더 |
| **QoreDB** ([github](https://github.com/QoreDB/QoreDB), [dev.to](https://dev.to/raphplt/i-built-a-30mb-database-client-in-rust-heres-what-it-cost-me-1pjb)) | Tauri 2.10 + React 19 + CodeMirror 6 | sqlx(PG/MySQL/SQLite), **tiberius + bb8**(SQL Server), mongodb, redis, duckdb(페더레이션) | 15 엔진 | 코어 Apache 2.0 + 프리미엄 BSL 1.1 | ~30MB 바이너리, 콜드스타트 <1s, 쿼리 인터셉터·감사로그, OS 키체인 |
| **Tablio** ([github](https://github.com/dasunNimantha/tablio)) | Tauri 2 + React | sqlx 풀(PG/Cockroach/MySQL/MariaDB/TiDB/SQLite), tiberius(MSSQL 단일 연결), scylla(Cassandra), russh(SSH), rust_xlsxwriter/calamine | 9종 | 오픈소스(라이선스 확인 필요) | `DatabaseDriver` trait 하위에 엔진별 모듈 |
| **rsql** ([github](https://github.com/rust-dd/rsql)) | Tauri v2 + React 19 + Monaco + glide-data-grid(WebGL) | tokio-postgres(simple_query 제로카피), deadpool-postgres, russh, libsql(로컬 저장), sonic-rs | PostgreSQL 전용(MySQL/SQLite/Redis 예정) | MIT | 76★. 2,000행 가상 페이징, EXPLAIN 시각화, PostGIS 지도, 내장 터미널 |
| **Tabular** ([github](https://github.com/tabular-id/tabular)) | **egui/eframe** (웹뷰 없음) | sqlx, mssql-client, redis, mongodb | PG, MySQL, SQLite, MSSQL, Mongo, Redis | AGPL-3.0 + 상용 | 단일 바이너리 30–70MB, WASM 플러그인, E2EE 동기화 |
| **Duckling** ([github](https://github.com/l1xnan/duckling)) | Tauri 2 + React + Monaco | duckdb 중심(기타 확인 필요) | DuckDB, SQLite, PG, MySQL, ClickHouse, Doris, Parquet/CSV | MIT | 580★, 분석용 뷰어 성격 |
| **Dataflare** ([dataflare.app](https://dataflare.app/)) | Tauri(awesome-tauri 등재), 비공개 소스 | 미공개 | 27+ (PG, MySQL, SQLite, ClickHouse, DuckDB, Redis, S3/R2…) | 무료(2연결) / $49 / $89 영구 | AI 쿼리, SQL Preview, 읽기전용 모드 |

관찰: Rust 진영은 (a) **sqlx**가 PG/MySQL/SQLite 공통 기반, (b) SQL Server는 **tiberius**(+bb8/deadpool), (c) Cassandra는 **scylla**, (d) SSH는 **russh**, (e) 로컬 상태 저장은 SQLite/libsql, (f) 그리드는 WebGL/Canvas 가상화가 사실상 표준. Oracle은 대부분 미지원 또는 실험(rainfrog는 런타임 의존성 필요) — **Oracle/Tibero 등 국내 수요 DBMS를 Rust로 안정 지원하면 뚜렷한 공백을 메울 수 있음**. QoreDB 저자는 "3개 웹뷰 엔진의 CSS 불일치"와 "드라이버 생태계 성숙도 편차"를 최대 비용으로 꼽음.

## 4. 비교표

| 도구 | 스택 | OS | DBMS 수 | 가격 | 오픈소스 | 메모리 평판 |
|---|---|---|---|---|---|---|
| DBeaver CE/PRO | Java/Eclipse RCP | W/M/L | 100+ | 무료 / $113–510/년 | Apache 2.0(CE) | 무거움(0.5–2GB+) |
| DataGrip | Java/IntelliJ | W/M/L | 50+ | $109/년 개인, $259 조직 | 아니오 | 무거움 |
| TablePlus | Swift·ObjC / C#·C++ | W/M/L(beta) | 20+ | $99 영구+갱신 | 아니오 | 매우 가벼움 |
| Beekeeper | Electron/Vue | W/M/L | 24 | 무료 / $9–18/월 | GPLv3+EULA | 중간 |
| DbGate | Electron/Svelte | W/M/L+Web | 18 | 무료 / $120/년 | GPL-3.0(확인 필요) | 중간 |
| Navicat 17 | Qt(확인 필요) | W/M/L | 9+클라우드 | $1,599 영구 | 아니오 | 보통 |
| HeidiSQL | Delphi/Lazarus | W(+L/M 네이티브) | 8 | 무료 | GPL-2.0 | 매우 가벼움 |
| DbVisualizer | Java | W/M/L | 50+ | 무료 / $199 | 아니오 | 무거움 |
| Valentina Studio | C++/Qt | W/M/L | 7 | 무료 / $79+ | 아니오 | 보통 |
| Antares | Electron/Vue | W/M/L | 6 | 무료 | MIT | 중간 |
| Tabularis | Tauri/Rust | W/M/L | 3+16(플러그인) | 무료 | Apache 2.0 | 가벼움 |
| QoreDB | Tauri/Rust | W/M/L | 15 | 무료/Pro | Apache+BSL | 가벼움(~30MB) |
| rainfrog / lazysql | Rust / Go TUI | 터미널 | 5–6 | 무료 | MIT | 극경량 |
| Azure Data Studio | Electron | — | — | 퇴역(2026-02-28) | MIT | — |

## 5. 공통 핵심 기능 목록 (거의 모든 진지한 클라이언트가 보유)
1. 연결 관리자(Connection Manager): 그룹/폴더, 색상 태그, SSL/TLS, **SSH 터널**, 비밀번호는 OS 키체인 저장, 읽기전용/프로덕션 표시.
2. 오브젝트 브라우저(스키마 트리): 테이블·뷰·프로시저·함수·트리거·인덱스·시퀀스, 검색/필터, 지연 로딩.
3. SQL 편집기: 구문 강조, 스키마 인식 자동완성, 다중 문장 실행(선택 영역/커서 문장), 포매터, 파라미터 입력.
4. 결과 그리드: 가상화 스크롤, 정렬/필터, 셀 인라인 편집, NULL/BLOB/JSON 뷰어, 복사(CSV/JSON/INSERT), 페이징.
5. 데이터 편집기 → 변경분 SQL 미리보기 후 커밋/롤백 (TablePlus, DBeaver, DataGrip 공통).
6. Export/Import: CSV/JSON/SQL dump/XLSX, 테이블 간 데이터 이관.
7. DDL 생성/보기: CREATE 스크립트, 테이블 디자이너(컬럼·인덱스·FK).
8. ER 다이어그램(DBeaver, DbGate, DataGrip, Navicat, Chat2DB, rsql, Valentina).
9. 세션/트랜잭션 제어: auto-commit 토글, 수동 commit/rollback, 실행 취소.
10. 쿼리 히스토리 + 저장 쿼리/즐겨찾기.
11. 멀티탭/멀티 연결 동시 사용, 탭 복구.
12. 다크/라이트 테마, 키바인딩 커스터마이즈(Vim 모드 흔해짐).
13. (2025~) AI 텍스트-투-SQL 및 MCP 노출 — DataGrip, DBeaver, Beekeeper, DbGate, Tabularis, Chat2DB 모두 도입.

## 6. 차별화 포인트 (소수 도구만 보유)
- **SQL*Plus 스타일 바인드 변수·치환 변수를 문장 간 공유** — usql(psql 변수), Harlequin/Duckling(Jinja 템플릿 변수), Tabularis 노트북(셀 간 변수) 정도. GUI 클라이언트에서는 드묾 → ★ **nexa-sql 1순위 차별점**([08](08-session-variables.md)).
- **편집기 내 연결 전환(in-editor connect switching)** — DataGrip 콘솔의 데이터소스 전환, usql `\connect`. 대부분 탭당 고정.
- **실행계획 시각화(Plan Visualizer)** — DataGrip, DBeaver, DbVisualizer Pro, Tabularis, rsql.
- **다중 결과셋 탭/그리드 분할** — DataGrip, DBeaver, DbGate; TablePlus·Beekeeper는 제한적.
- **결과셋 비교 / 데이터 비교·배포** — DataGrip, DbGate, DBeaver Enterprise.
- **Schema Compare/Diff & 마이그레이션 스크립트** — DataGrip, DBeaver Enterprise, DbGate, VS Code MSSQL.
- **변경 안전장치**: Safe Mode(TablePlus), Universal Query Interceptor·감사로그(QoreDB), 읽기전용 연결(Beekeeper 6.1, Dataflare).
- **SQL 노트북** — Tabularis, QoreDB, VS Code MSSQL.
- **MCP 서버 내장** — Tabularis, DataGrip 2026.2, Chat2DB.
- **언어 무관 플러그인(JSON-RPC over stdio / WASM)** — Tabularis, Tabular.
- **DuckDB 페더레이션(이종 DB 조인)** — QoreDB.
- **PostGIS 지도 뷰, 내장 터미널** — rsql, DBeaver(GIS).
- **로컬 히스토리/Git 통합** — DataGrip, DbVisualizer.
- **TUI/CLI 모드 동시 제공** — Chat2DB(CLI), usql/rainfrog/lazysql(단독). GUI+TUI를 한 코어에서 내는 제품은 사실상 없음 → Rust 코어 공유 시 차별화 가능.

## 7. nexa-sql 시사점 요약
1. 포지셔닝: "DBeaver의 커버리지 × TablePlus의 가벼움"이 시장 공백. Rust 진영(Tabularis, QoreDB)이 같은 곳을 노리므로 **Oracle/Tibero/Altibase 등 국내 DBMS 지원 + 바인드 변수·세션 제어 같은 DBA 워크플로**로 차별화.
2. 드라이버: sqlx(PG/MySQL/SQLite) + tiberius(MSSQL) + russh(SSH)가 검증된 조합. Oracle은 안정 크레이트가 부족하므로 별도 전략(ODPI-C FFI 또는 플러그인) 필요 → [06](06-rust-ecosystem.md).
3. 확장성: Tabularis의 stdio JSON-RPC 플러그인은 드라이버 생태계 부족을 우회하는 현실적 해법.
4. 라이선스: Community(Apache/MIT 또는 GPL) + 상용 확장(BSL/EULA) 이중 모델이 표준. 영구 라이선스+업데이트 갱신(TablePlus, DbVisualizer) 방식은 개인 개발자 수용도가 높음. (우리는 계열 정책 = PolyForm NC → [13](13-licensing.md).)
5. UX 필수: 변경 SQL 미리보기·커밋, Safe Mode, 쿼리 히스토리, 키체인 저장, Vim 키맵, 다크 테마 — 없으면 비교표에서 바로 탈락하는 항목.
