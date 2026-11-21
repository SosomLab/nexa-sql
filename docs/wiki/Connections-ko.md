# 접속

[English](Connections) | **한국어**

## 간략 접속 — 접속 문자열 한 줄로

1. **데이터베이스 로그인** 창 맨 위 **접속 문자열(간략 접속)** 칸에 `mysql://사용자:비밀번호@호스트:3306/DB`처럼 넣고 **[바로 접속]**(또는 Enter)을 누른다. Oracle `oracle://…`, SQL Server `mssql://…`, PostgreSQL `postgres://…`, SQLite 파일 경로도 같다.
2. 입력한 내용으로 **프로필이 자동 저장**된다(이름 = `방언-호스트-DB` · 같은 이름이 있으면 `-2`가 붙는다). 다음부터는 그 이름만 넣어도 된다.
3. **비밀번호는 저장되지 않는다.** 문자열에 넣은 비밀번호는 이번 실행 동안만 쓰고, 없으면 **비밀번호** 칸으로 이동하니 직접 입력한다. 저장하고 싶으면 프로필을 고친 뒤 비밀번호 옆 **저장**을 체크하고 [저장]을 누른다.

## 로그인 폼의 선택 항목

- **비밀번호 없음**(**사용자** 줄 오른쪽 체크) — 비밀번호를 보내지 않는 것이 뜻인 프로필용이다: Oracle OS 인증, PostgreSQL `trust`, 비밀번호 없는 계정. 켜면 접속 때 비밀번호를 묻지 않고 빈 비밀번호를 보낸다. 프로필에는 `nopassword=on`이 저장되고(비밀이 아니라 암호화하지 않음), 그 프로필을 폼에 불러오면 체크된 채 열린다. Oracle은 **사용자**도 비워 두면 OS 인증(`/`)으로 접속한다 — OCI 드라이버에서만 되고 Thin 드라이버는 오류를 낸다. CLI에서는 접속 문자열의 비밀번호 칸을 비우면(`postgres://u:@h:5432/db`) `nsql conn add`가 `nopassword=on`으로 저장한다.
- **역할**(**DB 종류** 줄 오른쪽 콤보 · Oracle일 때만) — **일반** · SYSDBA · SYSOPER. 프로필에는 `role=SYSDBA` 또는 `role=SYSOPER`가 저장된다(비면 일반 접속). 접속 문자열 꼬리 `?role=sysdba`도 받는다(저장은 대문자). CLI는 `… AS SYSDBA`로 보여 준다.

## 지원 데이터베이스

| 데이터베이스 | 방식 | 비고 |
|---|---|---|
| Oracle · SQL Server · PostgreSQL · MySQL/MariaDB · SQLite · DuckDB · ODBC | 설치본에는 Oracle OCI만 들어 있고, 나머지(Oracle Thin 포함)는 처음 접속할 때 자동 설치되는 확장 드라이버(`driver.auto_install`) | 아래 절 참고 |
| ClickHouse | 확장 드라이버 `clickhouse`(HTTP 8123 / HTTPS 8443) | `clickhouse://사용자:비밀번호@호스트:8123/db[?tls=required]` · 트랜잭션 없음 · 그리드 편집 미지원 · 최신 드라이버 릴리스에서 설치 |
| Redshift · CockroachDB · YugabyteDB · Greenplum · TimescaleDB | PostgreSQL 드라이버(와이어 호환) | `redshift://` 5439 · `cockroach://` 26257 · `yugabyte://` 5433 · `greenplum://` / `timescale://` 5432 |
| IBM Db2(LUW) | 네이티브 DRDA 드라이버(IBM 클라이언트 불필요 · 첫 접속 때 최신 드라이버 릴리스에서 자동 설치 · [드라이버](Drivers-ko.md) 참고) · `?driver=odbc` = ODBC 폴백(IBM clidriver 필요) | `db2://사용자:비밀번호@호스트:50000/DB` · 카탈로그 = SYSCAT · 데이터베이스 로그인 창 DB 종류 **Db2** |
| Microsoft Access | ODBC 어댑터 + Access 방언(Windows ACE 드라이버 · Linux/macOS MDBTools) | `access:C:/경로/파일.accdb` · DB 종류 **MS Access** |
| SAP HANA · Informix | ODBC 프리셋(ODBC 드라이버로 접속) | `hana://` 30015(ODBC 드라이버 `HDBODBC`) · `informix://` 9088(`IBM INFORMIX ODBC DRIVER`) · 벤더 ODBC 드라이버를 먼저 설치 |

- PostgreSQL 호환 스킴은 스킴만 쓰면 기본 포트가 채워진다.
- SAP HANA와 Informix는 데이터베이스 로그인 창 **DB 종류** 목록에 없다. 접속 문자열에 스킴을 쓰거나(`hana://사용자:비밀번호@호스트/DB` = ODBC 드라이버 이름과 기본 포트가 채워짐), **ODBC**를 고르고 **DSN** 칸에 ODBC 접속 문자열을 넣는다.

## ODBC로 접속

- DSN이 있으면 `odbc:DSN=이름`(사용자·비밀번호는 칸에 따로). DSN 없이 드라이버를 바로 쓰면 `odbc:Driver={SQL Server};Server=호스트,1433;Database=DB;Uid=사용자;Pwd=비밀번호`처럼 ODBC 접속 문자열을 그대로 붙인다(비밀번호는 화면·로그에 `***`로 가려진다).
- **Windows**: ODBC 확장 드라이버는 첫 접속 때 자동 설치되고, 그 밖에는 **벤더 ODBC 드라이버**와 DSN(ODBC 데이터 원본 관리자)만 있으면 된다 · 현재 제한 = 실행 중지(■), REF CURSOR 미지원.
- **macOS, Linux**: unixODBC와 벤더 드라이버 등록이 필요하다 — 아래 "macOS / Linux에서 ODBC" 절차.

### macOS / Linux에서 ODBC

**1. 전제**
- ODBC 드라이버는 확장 드라이버(`nsql driver install odbc`)로 배포된다. ODBC를 내장한 판은 개발 빌드뿐이고, macOS 설치본(lite)은 확장을 쓴다. Intel 맥(`macos-x86_64`)용 ODBC 패키지는 아직 게시되지 않았다(T-401) — Apple Silicon·Linux용은 있다.
- unixODBC가 설치돼 있어야 한다: 맥 `brew install unixodbc` · Linux `sudo apt-get install unixodbc`(빌드하려면 `unixodbc-dev`). 어느 `odbcinst.ini` / `odbc.ini` 파일을 쓰는지는 `odbcinst -j`로 본다.

**2. 벤더 드라이버 설치·등록**(SQL Server 예)
```
brew tap microsoft/mssql-release https://github.com/Microsoft/homebrew-mssql-release
HOMEBREW_ACCEPT_EULA=Y brew install msodbcsql18 mssql-tools18
odbcinst -q -d          # 목록에 [ODBC Driver 18 for SQL Server]가 있어야 한다
```
Homebrew 경로: Intel = `/usr/local` · Apple Silicon = `/opt/homebrew`.

**3. 앱 밖에서 먼저 접속 확인**
```
isql -v -k "Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes"
```

**4. Nexa SQL에 입력**
- **데이터베이스 로그인** 창, **DB 종류** = ODBC, **DSN** 칸 = `Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;TrustServerCertificate=yes`(사용자·비밀번호는 칸에 따로), 또는
- 간략 접속: `odbc:Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes`, 또는
- CLI: `nsql conn add MyMssql "odbc:Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes"`
- 드라이버 이름은 `odbcinst -q -d`와 글자까지 같아야 한다(`{SQL Server}`는 Windows 이름이라 macOS/Linux에는 없다).

**5. 접속 문자열 대신 DSN** — `~/.odbc.ini`:
```
[MyMssql]
Driver = ODBC Driver 18 for SQL Server
Server = host,1433
Database = db
TrustServerCertificate = yes
```
그다음 `odbc:DSN=MyMssql`로 접속한다(사용자·비밀번호는 칸에).

**6. 다른 드라이버** — 설치한 뒤 이름이 등록됐는지(`odbcinst -q -d`) 확인하고, 없으면 `odbcinst.ini`에 절을 추가한다:
```
[PostgreSQL Unicode]
Driver = /usr/local/lib/psqlodbcw.so      # brew install psqlodbc (Apple Silicon: /opt/homebrew/lib)
```
- MariaDB/MySQL = MariaDB Connector/ODBC · Oracle = Instant Client ODBC 패키지(`libsqora`) · 어느 벤더든 `[이름]` + `Driver = <라이브러리 경로>` 형식은 같다.

**트랜잭션** — Nexa SQL은 ODBC 드라이버를 기본 자동 커밋으로 둔다. 수동 커밋 모드에서는 첫 문장이 트랜잭션을 시작하고(자동 커밋 끔), COMMIT이나 ROLLBACK이 끝내면서 자동 커밋을 다시 켠다.

**7. 오류 대조**

| 메시지 | 뜻 | 조치 |
|---|---|---|
| `Can't open lib '…' : file not found` / IM002 `Data source name not found`(Nexa SQL이 "Installed ODBC drivers: …"를 덧붙이고 **드라이버/DSN 없음**으로 보여 줌) | 드라이버나 DSN 이름이 등록되지 않음 | 드라이버 설치 · `odbcinst -q -d`의 이름을 그대로 쓴다 |
| 목록에 드라이버가 있는데 같은 오류 | 아키텍처 불일치(예: Intel 앱에 arm64 라이브러리) | `file <드라이버 라이브러리>`가 앱과 같아야 한다(x86_64 / arm64) |
| `Login timeout expired` | 네트워크(호스트, 포트, VPN, 방화벽) | 도달 확인(`nc -z host 1433`) |
| 인증서 / SSL 오류 | Driver 18은 기본으로 암호화한다 | `TrustServerCertificate=yes` 추가(또는 CA 설치) |
| `Login failed for user` | 계정 / 비밀번호 | 자격 증명과 기본 데이터베이스 확인 |
| `SSL Provider: OpenSSL library could not be loaded`(macOS · Homebrew) | `msodbcsql18`가 **openssl@4**를 끌어와 `/usr/local/opt/openssl`이 4.x를 가리키는데, 드라이버는 그곳에서 `libssl.3.dylib`를 찾는다 | openssl@4 `lib` 폴더 안의 `libssl.3.dylib`·`libcrypto.3.dylib`를 openssl@3로 링크(`scripts/mac-odbc-setup.sh`가 처리)하거나 `DYLD_LIBRARY_PATH=$(brew --prefix openssl@3)/lib`로 앱을 실행 |

스크립트(개발 체크아웃): `scripts/mac-odbc-setup.sh` = unixODBC, ODBC Driver 18 for SQL Server, SQLite3 ODBC 설치와 OpenSSL 조치 · `scripts/mac-odbc-rollback.sh` = 역순으로 되돌림(기본 dry-run, `--yes`로 적용, `--with-unixodbc`로 unixODBC까지 제거).
