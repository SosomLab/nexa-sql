# 접속(Connections)

## 간략 접속 — 접속 문자열 한 줄로

1. 로그인 창 맨 위 **접속 문자열(간략 접속)** 칸에 `mysql://사용자:비밀번호@호스트:3306/DB`처럼 넣고 **[바로 접속]**(또는 Enter)을 누른다. Oracle `oracle://…` · SQL Server `mssql://…` · PostgreSQL `postgres://…` · SQLite 파일 경로도 같다.
2. 입력한 내용으로 **프로필이 자동 저장**된다(이름 = `방언-호스트-DB` · 같은 이름이 있으면 `-2`가 붙는다). 다음부터는 그 이름만 넣어도 된다.
3. **비밀번호는 저장되지 않는다.** 문자열에 넣은 비밀번호는 이번 실행 동안만 쓰고, 없으면 비밀번호 칸으로 이동하니 직접 입력한다. 저장하고 싶으면 프로필을 고친 뒤 비밀번호 옆 **저장**을 체크하고 [저장]을 누른다.

## Supported databases (English)

| Database | How | Notes |
|---|---|---|
| Oracle · SQL Server · PostgreSQL · MySQL/MariaDB · SQLite · DuckDB · ODBC | built-in or extension drivers | see the sections below |
| ClickHouse | extension driver `clickhouse` (HTTP 8123 / HTTPS 8443) | `clickhouse://user:pass@host:8123/db[?tls=required]` · no transactions · grid editing not supported · installable from drivers release 0.1.3 |
| Redshift · CockroachDB · YugabyteDB · Greenplum · TimescaleDB | PostgreSQL driver (wire-compatible) | `redshift://` 5439 · `cockroach://` 26257 · `yugabyte://` 5433 · `greenplum://` / `timescale://` 5432 |
| IBM Db2 (LUW) | native DRDA driver (no IBM client · installed automatically on first connect · drivers-v0.1.4+ · see [Drivers](Drivers.md)) · ODBC fallback with `?driver=odbc` (needs IBM clidriver) | `db2://user:pass@host:50000/DB` · catalog = SYSCAT · Database type **Db2** in the login form |
| Microsoft Access | ODBC adapter + Access dialect (Windows ACE driver · Linux/macOS MDBTools) | `access:C:/path/file.accdb` · Database type **MS Access** |
| SAP HANA · Informix | ODBC preset | `db2://` 50000 · `hana://` 30015 · `informix://` 9088 · install the vendor ODBC driver first |

### 지원 데이터베이스(한국어)

- ClickHouse = 확장 드라이버 `clickhouse`(HTTP 8123 / HTTPS 8443) · 트랜잭션 없음 · 그리드 편집 미지원 · 드라이버 릴리스 0.1.3부터 설치.
- Redshift · CockroachDB · YugabyteDB · Greenplum · TimescaleDB = PostgreSQL 드라이버로 접속(스킴만 쓰면 기본 포트가 채워진다).
- IBM Db2 = 네이티브 DRDA 드라이버(IBM 클라이언트 불필요 · 첫 접속 때 자동 설치 · drivers-v0.1.4 이상 · [드라이버](Drivers.md)) · `?driver=odbc` = ODBC 폴백(IBM clidriver 필요) · `db2://사용자:비밀번호@호스트:50000/DB` · 접속 창 DB 종류 **Db2**.
- MS Access = ODBC 어댑터 위 Access 방언(Windows ACE · Linux/macOS MDBTools) · `access:<파일>` · DB 종류 **MS Access**.
- SAP HANA · Informix = ODBC 프리셋(벤더 ODBC 드라이버 설치 필요) · 접속 창 DB 종류 목록에는 없다(PostgreSQL/ODBC로 고르고 포트만 바꾸거나 접속 문자열에 스킴).

## ODBC로 접속

- DSN이 있으면 `odbc:DSN=이름`(사용자·비밀번호는 칸에 따로) · DSN 없이 드라이버를 바로 쓰면 `odbc:Driver={SQL Server};Server=호스트,1433;Database=DB;Uid=사용자;Pwd=비밀번호`처럼 ODBC 접속 문자열을 그대로 붙인다(비밀번호는 화면·로그에 `***`로 가려진다).
- **Windows**: 앱 쪽 설치는 필요 없고 **벤더 ODBC 드라이버**와 DSN(ODBC 데이터 원본 관리자)만 있으면 된다 · 1차 제한 = 실행 중지(■) · REF CURSOR 미지원.
- **macOS·Linux**: unixODBC + 벤더 드라이버 등록이 필요하다 — 아래 "ODBC on macOS / Linux" 절차.

### ODBC on macOS / Linux (English)

**1. Prerequisites**
- The ODBC driver ships as an extension driver (`nsql driver install odbc`). Builds that include ODBC built in are developer builds only; the macOS installer (lite) uses the extension. An Intel-Mac (`macos-x86_64`) ODBC package is not published yet (T-401) — Apple Silicon and Linux packages exist.
- unixODBC must be installed: macOS `brew install unixodbc` · Linux `sudo apt-get install unixodbc` (or `unixodbc-dev` to build). `odbcinst -j` shows which `odbcinst.ini` / `odbc.ini` files are used.

**2. Install and register a vendor driver** (SQL Server example)
```
brew tap microsoft/mssql-release https://github.com/Microsoft/homebrew-mssql-release
HOMEBREW_ACCEPT_EULA=Y brew install msodbcsql18 mssql-tools18
odbcinst -q -d          # must list: [ODBC Driver 18 for SQL Server]
```
Homebrew prefix: Intel = `/usr/local` · Apple Silicon = `/opt/homebrew`.

**3. Check the connection outside the app first**
```
isql -v -k "Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes"
```

**4. Enter it in Nexa SQL**
- Login window, Database type = ODBC, **DSN** field = `Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;TrustServerCertificate=yes` (user and password in their own fields), or
- Quick connect: `odbc:Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes`, or
- CLI: `nsql conn add MyMssql "odbc:Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes"`
- The driver name must match `odbcinst -q -d` exactly (`{SQL Server}` is the Windows name and does not exist on macOS/Linux).

**5. DSN instead of a connection string** — `~/.odbc.ini`:
```
[MyMssql]
Driver = ODBC Driver 18 for SQL Server
Server = host,1433
Database = db
TrustServerCertificate = yes
```
then connect with `odbc:DSN=MyMssql` (user and password in their fields).

**6. Other drivers** — install, then make sure the name is registered (`odbcinst -q -d`); if not, add a section to `odbcinst.ini`:
```
[PostgreSQL Unicode]
Driver = /usr/local/lib/psqlodbcw.so      # brew install psqlodbc (Apple Silicon: /opt/homebrew/lib)
```
- MariaDB/MySQL = MariaDB Connector/ODBC · Oracle = Instant Client ODBC package (`libsqora`) · every vendor uses the same `[Name]` + `Driver = <path to the library>` form.

**Transactions** — Nexa SQL keeps the ODBC driver in autocommit mode by default. In manual-commit mode the first statement starts a transaction (autocommit off) and COMMIT or ROLLBACK ends it and turns autocommit back on.

**7. Error guide**

| Message | Meaning | Fix |
|---|---|---|
| `Can't open lib '…' : file not found` / IM002 `Data source name not found` (Nexa SQL adds "Installed ODBC drivers: …", shown as **Driver or DSN not available**) | The driver or DSN name is not registered | Install the driver · use the exact name from `odbcinst -q -d` |
| Same error although the driver is listed | Architecture mismatch (e.g. an arm64 library in an Intel app) | `file <driver library>` must match the app (x86_64 / arm64) |
| `Login timeout expired` | Network (host, port, VPN, firewall) | Check reachability (`nc -z host 1433`) |
| certificate / SSL error | Driver 18 encrypts by default | Add `TrustServerCertificate=yes` (or install the CA) |
| `Login failed for user` | Account / password | Check credentials and the default database |
| `SSL Provider: OpenSSL library could not be loaded` (macOS · Homebrew) | `msodbcsql18` pulls in **openssl@4**, so `/usr/local/opt/openssl` points to 4.x while the driver looks for `libssl.3.dylib` there | Link `libssl.3.dylib` and `libcrypto.3.dylib` inside the openssl@4 `lib` folder to openssl@3 (`scripts/mac-odbc-setup.sh` does this), or start the app with `DYLD_LIBRARY_PATH=$(brew --prefix openssl@3)/lib` |

Scripts (developer checkout): `scripts/mac-odbc-setup.sh` installs unixODBC, ODBC Driver 18 for SQL Server and SQLite3 ODBC and applies the OpenSSL fix · `scripts/mac-odbc-rollback.sh` undoes it in reverse order (dry run by default, `--yes` to apply, `--with-unixodbc` to remove unixODBC too).

### macOS·Linux ODBC 절차(한국어)

1. **전제**: ODBC 드라이버는 확장 드라이버(`nsql driver install odbc`)다. ODBC를 내장한 판은 개발 빌드뿐이고, macOS 설치본(lite)은 확장을 쓴다. Intel 맥(`macos-x86_64`)용 ODBC 패키지는 아직 없다(T-401 · Apple Silicon·Linux용은 있음). unixODBC 설치 = 맥 `brew install unixodbc` · Linux `sudo apt-get install unixodbc` · 쓰는 설정 파일 확인 = `odbcinst -j`.
2. **벤더 드라이버 설치·등록**(SQL Server 예): 위 2번 명령 → `odbcinst -q -d`에 `[ODBC Driver 18 for SQL Server]`가 보여야 한다. Homebrew 경로 = Intel `/usr/local` · Apple Silicon `/opt/homebrew`.
3. **앱 밖 사전 확인**: 위 3번 `isql -v -k "…"`.
4. **앱 입력**: 로그인 창 데이터베이스 유형 = ODBC · **DSN 칸** = `Driver={ODBC Driver 18 for SQL Server};Server=호스트,1433;Database=DB;TrustServerCertificate=yes`(사용자·비밀번호는 칸에 따로) · 또는 간략 접속 `odbc:…` · 또는 CLI `nsql conn add …`. 드라이버 이름은 `odbcinst -q -d`와 글자까지 같아야 한다(`{SQL Server}`는 Windows 이름 · 맥·Linux에는 없음).
5. **DSN 방식**: 위 5번처럼 `~/.odbc.ini`에 등록 → `odbc:DSN=이름`.
6. **다른 드라이버**: PostgreSQL = `brew install psqlodbc` 뒤 `odbcinst.ini`에 `[PostgreSQL Unicode]` + `Driver = /usr/local/lib/psqlodbcw.so`(Apple Silicon `/opt/homebrew/lib`) · MariaDB/MySQL = MariaDB Connector/ODBC · Oracle = Instant Client ODBC 패키지(`libsqora`) · 등록 형식은 모두 `[이름]` + `Driver = 라이브러리 경로`.
- **트랜잭션**: ODBC 드라이버는 기본 자동 커밋으로 둡니다. 수동 커밋 모드에서는 첫 문장이 트랜잭션을 시작하고(자동 커밋 끔), COMMIT·ROLLBACK이 끝내며 자동 커밋을 다시 켭니다.
7. **오류 대조**: (맥 Homebrew) `SSL Provider: OpenSSL library could not be loaded` = `msodbcsql18`가 openssl@4를 끌어와 `/usr/local/opt/openssl`이 4.x를 가리킴 → openssl@4 `lib` 안의 `libssl.3.dylib`·`libcrypto.3.dylib`를 openssl@3로 링크(`scripts/mac-odbc-setup.sh`가 처리) 또는 `DYLD_LIBRARY_PATH=$(brew --prefix openssl@3)/lib`로 실행 · 설치·되돌리기 스크립트 = `scripts/mac-odbc-setup.sh` · `scripts/mac-odbc-rollback.sh`(기본 dry-run · `--yes`) · 그 밖: `Can't open lib` / IM002 = 미등록(앱은 "Installed ODBC drivers: …"를 덧붙이고 **드라이버/DSN 없음**으로 분류) · 목록에 있는데 같은 오류 = 아키텍처 불일치(`file <라이브러리>`로 x86_64/arm64 확인) · `Login timeout expired` = 네트워크(호스트·포트·VPN) · 인증서 오류 = `TrustServerCertificate=yes` · `Login failed` = 계정.
