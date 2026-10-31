# 40 · CLI `nsql` 사용법 — 순서대로 따라 하는 기능 점검

> **요청**(사용자 09-16): *"CLI 사용 방법이 정리된 문서가 있는지 확인하고 필요하면 만들어줘 · 추가된 기능들을 테스트할 수 있도록 순서대로 · `conn list`를 사용한 접속뿐 아니라 직접 hostname 등을 지정해서 접속하는 방법도 기술."* · (사용자 10-23) *"40번 CLI 명령 전체 최신화"*
> **성격**: 이 문서는 **사용·점검 안내**다. 설계 제안은 [11 CLI](11-cli.md), 인자 규약과 타 CLI 조사는 [27 CLI 인자 규약](27-cli-conventions.md), 연결 프로필 내부는 [21](21-connection-profiles.md), 드라이버 확장은 [22](22-driver-extensions.md)에 있다.
> **검증**: §2의 모든 명령·출력은 **nsql 0.2.8**(main 6561a4e 빌드 · 2026-10-23 · Windows · 격리 `NSQL_HOME` · `ui.lang=ko`)로 **실제 실행해 확인**했다 — SQLite(내장) + DuckDB(확장 · 접속 시 자동 설치). 실서버가 필요한 예(§3 · Oracle/SQL Server/PostgreSQL/MySQL/ODBC)는 "(출력 생략 · 실서버 필요)"로 표시했다. 출력의 절대 경로는 `…`로 줄였다.
> **10-23 최신화**: 명령 8개 → **14개**(`import` `format` `driver` `license` `grep` `bookmark` 추가) · 접속 대상(MySQL · DuckDB · ODBC · `?driver=` · 빈 비밀번호 `user:@host` · `--password-stdin`) · **드라이버 = 확장(D-350)** · 출력 형식 `sql:*` · 표 모양 옵션 · 셸 `\more`/`\all`/`\count`/`\pager`/`set max_rows` · 프로필 서버 유형 `conn env` · 모든 단계의 출력을 0.2.8로 다시 떴다.
> **원장**: 옵션·인자의 최종 근거는 `nsql <명령> --help`(`nsql-cli/src/help.rs`)다. 어긋나면 도움말이 맞다.

---

## 0. 준비

```sh
cargo build --release              # target/release/nsql.exe · nexa-sql.exe
cargo build                        # 디버그도 함께(실기 테스트는 디버그 exe로 · CLAUDE.md 규약)
```

이 문서는 `nsql`을 PATH에 둔 것으로 적는다. 아니면 `./target/release/nsql.exe`로 바꿔 읽는다. 설치본(winget·deb·rpm …)은 `nsql`이 PATH에 들어간다.

> **시험할 때는 격리 홈으로**: `NSQL_HOME=<빈 폴더>`(설정·프로필·라이선스·북마크·드라이버가 모두 그 아래) · 드라이버만 따로 두려면 `NSQL_DRIVER_DIR`. 실제 설정 폴더를 건드리지 않는다(CLAUDE.md 규약).

### 도움말이 원장이다

CLI 도움말은 `nsql-cli/src/help.rs` 한 곳에서 나온다(인자·옵션·예제 원장 · 영/한 `Msg` · 언어 = `ui.lang`). **이 문서와 도움말이 어긋나면 도움말이 맞다.**

> ★ **CLI 문구 = `ui.lang`을 따른다**(GUI와 같은 설정 · `system`(OS 언어) | `en` | `ko` · `nsql config set ui.lang en`) — 도움말·안내·오류가 모두 그 언어로 나온다(10-23 d9b95c0 · b255950부터 `nsql driver` 밖 명령까지 전부 · 그 전 빌드는 일부 한국어 고정). 영어로 고정되는 것 = 표 머리글 · `key = value` 필드명 · 상태 낱말(OK · installed …) · usage 구문 한 줄 · `Enter value for` SQL*Plus 프롬프트 · 진단 꼬리표(`[driver]` · `ERROR:` · ⏱) · 드라이버·서버가 돌려준 오류 원문.

```sh
nsql --help            # 개요: 명령 14개 · 접속 대상 · 공통 옵션 · 지원 방언
nsql run --help        # 한 명령의 인자·옵션·예제 전부
nsql help run          # 같은 내용
nsql -V                # 판올림 → nsql 0.2.8
```

`nsql --help` 끝줄이 **지원 방언**을 알려 준다(이 빌드에 내장됐거나 확장으로 받을 수 있는 것):

```
  지원 방언(내장 또는 확장): oracle, mssql, postgres, mysql, sqlite, odbc, duckdb
```

> ★ **드라이버 구성(D-350 · 10-23)**: 설치본에 내장된 드라이버는 **Oracle OCI뿐**이고 나머지(SQLite · PostgreSQL · MySQL · SQL Server · Oracle Thin · ODBC · DuckDB)는 **확장 드라이버**다. 접속에 필요한 드라이버가 없으면 **접속 전에 자동으로 내려받아 검증·설치**한다(설정 `driver.auto_install` · 기본 on · Step 17). 개발 빌드(`cargo build`)는 기본 feature로 대부분 내장이라 자동 설치가 일어나지 않는다 — 이 문서의 검증 빌드도 SQLite는 내장, DuckDB는 확장이었다.

### 명령 14개

| 명령 | 하는 일 | 단계 |
|---|---|---|
| `run` | SQL 스크립트(파일·stdin) 실행하고 결과 출력 | Step 2~8 |
| `shell` | 대화형 셸(줄 단위 · sqlplus/psql식) | Step 14 |
| `export` | 질의나 테이블 전체를 csv/tsv/json/jsonl/insert로 | Step 11 |
| `import` | csv/tsv/jsonl 파일을 표에 대량 적재 | Step 12 |
| `format` | 내장 Basic 포맷터(설정 `format.*`)로 SQL 포맷 | Step 13 |
| `explain` | 실행 계획(방언별 EXPLAIN) | Step 10 |
| `plan` | **접속 없이** 스크립트 해석 — 문장·변수·재작성 | Step 1 |
| `conn` | 연결 프로필(사용자 폴더 저장 · GUI와 공유) | §1-2 · Step 16 |
| `driver` | 확장 드라이버 검색·설치·갱신·삭제 | Step 17 |
| `license` | 라이선스(오프라인 파일 · 요청 코드 · 설치 · 상태) | Step 20 |
| `cat` | 카탈로그 — 스키마·객체·컬럼·하위 항목·SQL 생성·상세·소스·컴파일 오류 | Step 9 |
| `config` | 앱 설정(`settings.conf` · GUI와 공유) | Step 15 |
| `grep` | 파일 내용 검색(GUI 파일 검색과 같은 엔진) | Step 18 |
| `bookmark` | 북마크(GUI와 같은 워크스페이스 파일) | Step 19 |

---

## 1. 접속을 지정하는 두 가지 방법

`-c <target>`은 **① 접속 문자열**이나 **② 프로필 이름**을 받는다. 둘 다 같은 자리에 쓴다. 접속이 필요한 명령 = `run` `shell` `export` `import` `explain` `cat` `conn test`.

### 1-1. 직접 지정 — 접속 문자열 (프로필 없이)

가장 빠른 방법이다. 저장하지 않고 그때그때 붙는다.

| 방언 | 형식 | 예 |
|---|---|---|
| **Oracle** | `oracle://user:pass@host:port/service` | `oracle://scott:tiger@172.16.1.58:1521/APPDB` |
| **Oracle**(SQL\*Plus 꼴) | `user/pass@host:port/service` | `APPDB/pw@172.16.1.58:1521/APPDB`(방언은 `-d` · 기본 oracle) |
| **SQL Server** | `mssql://user:pass@host:port/db` | `mssql://sa:pw@172.16.1.58:1433/PLANDB_MS` |
| **PostgreSQL** | `postgres://user:pass@host:port/db` | `postgres://postgres:pw@172.16.1.60:5433/repodb` |
| **MySQL · MariaDB** | `mysql://user:pass@host:port/db` | `mysql://root:pw@dbhost:3306/shop` |
| **SQLite(파일)** | `sqlite:<경로>` | `sqlite:C:\data\app.db` · `sqlite:./app.db` |
| **SQLite(메모리)** | `sqlite::memory:` | 서버 없이 시험할 때 |
| **DuckDB(파일·메모리)** | `duckdb:<경로>` · `duckdb::memory:` | `duckdb:a.duckdb` (확장 드라이버) |
| **ODBC** | `odbc:DSN=<이름>[;Uid=…;Pwd=…]` · `odbc:Driver={…};Server=…;…` | `odbc:DSN=TIBERO_DEV;Uid=sys;Pwd=pw` · 나머지 전부가 ODBC 접속 문자열 (`Uid`/`Pwd`는 사용자·비밀번호로 올려 봉투에 저장) |
| ODBC 벤더 스킴 | `tibero://` · `altibase://` · `cubrid://` + `user:pass@host:port/db` | ODBC 드라이버 이름 프리셋으로 바뀐다(기본 포트 8629 · 20300 · 33000 · 도움말 개요에는 없음 · `nsql-script` `connect.rs` 기준) |

**기본 포트**(생략 가능): Oracle 1521 · SQL Server 1433 · PostgreSQL 5432 · MySQL 3306.

**쿼리 매개변수**(`?k=v&…` · URL 꼴 끝에): `driver=thin|oci|<id>`(이 접속의 드라이버 · §1-1 (h)) · `dialect=`/`type=`(스킴 없이 방언 강제) · `schema=` · `env=`(서버 유형) · `tls=`/`sslmode=` · `role=`(도움말 개요에는 `?driver=`만 · 나머지는 `nsql-script` `connect.rs` 기준).

#### (a) 모든 명령에 그대로 쓴다

`-c` 자리에 프로필 이름 대신 문자열을 넣기만 하면 된다. **저장되는 것은 없다.** (출력 생략 · 실서버 필요)

```sh
# 한 줄 질의 — 스크립트 파일도 필요 없다(`-` = 표준 입력)
echo "SELECT sysdate FROM dual;" | nsql run -c "oracle://scott:tiger@dbhost:1521/ORCL" -

# 스크립트 실행
nsql run     -c "mssql://sa:pw@172.16.1.58:1433/PLANDB_MS" deploy.sql

# 대화형 셸
nsql shell   -c "postgres://postgres:pw@172.16.1.60:5433/repodb"

# 내보내기 · 적재
nsql export  -c "oracle://scott:tiger@dbhost:1521/ORCL" -t EMP -f csv -o emp.csv
nsql import  -c "mysql://app:pw@dbhost:3306/shop" -t orders orders.csv

# 카탈로그 훑기
nsql cat     -c "postgres://u:pw@h:5432/mydb" tables
nsql cat     -c "oracle://scott:tiger@dbhost:1521/ORCL" -s HR columns EMPLOYEES

# 실행 계획
nsql explain -c "mssql://sa:pw@srv:1433/mydb" -q "SELECT * FROM orders WHERE id=1"

# 붙는지만 확인
nsql conn test -d oracle --host dbhost --port 1521 --db ORCL --user scott
```

포트를 생략하면 방언 기본 포트가 쓰인다:

```sh
nsql run -c "oracle://scott:tiger@dbhost/ORCL" script.sql     # → 1521
```

#### (b) 비밀번호를 주는 방법

| 방법 | 쓰기 | 언제 |
|---|---|---|
| 문자열에 직접 | `-c "oracle://scott:tiger@h:1521/ORCL"` | 빠르지만 **히스토리에 남는다** |
| **생략 → 숨김 입력** | `-c "oracle://scott@h:1521/ORCL"` | 사람이 직접 칠 때 **권장** · 그 접속에만 쓰고 저장하지 않는다 |
| **환경변수 `NSQL_PASSWORD`** | `export NSQL_PASSWORD='s3cr3t'` | CI·스크립트 — **전용 변수가 있다** |
| **`--password-stdin`** | `echo "$PW" \| nsql run --password-stdin -c user@host script.sql` | 명령줄·셸 기록·프로세스 목록에 남기지 않을 때 · 표준 입력 스크립트(`-`)와는 함께 못 쓴다 |
| 필드 + `-p` | `-d oracle --host h --user scott -p pw` | 문자열 조립이 번거로울 때 |
| **빈 비밀번호** | `-c "postgres://app:@h:5432/db"` | `user:@host` = 빈 비밀번호를 명시 → **묻지 않는다** |

찾는 순서는 **`-p`(또는 `--password-stdin`) → `NSQL_PASSWORD` → 터미널이면 숨김 프롬프트**이고, `--no-prompt`면 묻지 않는다(배치 · 치환 변수도 빈 값).

```sh
# 비밀번호를 빼면 실행 시 숨김 입력으로 묻는다
nsql run -c "oracle://scott@dbhost:1521/ORCL" script.sql
Password:

# CI — 물어보지 못하므로 --no-prompt
nsql run -c "postgres://app@dbhost:5432/appdb" --no-prompt migrate.sql      # NSQL_PASSWORD에서

# 표준 입력의 첫 줄을 비밀번호로
printf '%s\n' "$DBPW" | nsql run --password-stdin -c "postgres://app@dbhost:5432/appdb" migrate.sql
```

`--password-stdin`인데 읽을 것이 없거나 스크립트가 `-`이면 종료 2로 막는다(SQLite로 확인):

```
$ nsql run -c sqlite::memory: -f csv - --password-stdin
--password-stdin: 읽을 것이 없거나 표준 입력을 스크립트(-)가 쓰고 있습니다
```

> ⚠️ **셸 히스토리·프로세스 목록 주의.** 명령줄에 쓴 비밀번호는 `history`와 `ps`/작업 관리자에 노출된다. 사람이 칠 때는 **비밀번호를 빼고 프롬프트**를, 자동화에는 **환경변수 또는 `--password-stdin`**을 쓴다. 반복해서 쓸 접속은 §1-2 프로필이 안전하다.

#### (c) 비밀번호에 특수문자가 있을 때

`@` `/` `:`는 구분자라 그대로 쓰면 잘못 잘린다. **URL 형식(`scheme://`)에서만 퍼센트 인코딩이 풀린다.**

| 문자 | 인코딩 |
|---|---|
| `@` | `%40` |
| `/` | `%2F` |
| `:` | `%3A` |
| `#` | `%23` |
| `%` | `%25` |

```sh
# 비밀번호가 p@ss/word 인 경우
nsql run -c "oracle://sys:p%40ss%2Fword@dbhost:1521/ORCL" script.sql
```

화면·로그에는 비밀번호를 가린 꼴로만 나온다:

```
oracle://sys/***@dbhost:1521/ORCL
```

> SQL\*Plus 꼴(`user/pass@host`)은 **퍼센트 인코딩을 풀지 않는다.** 다만 호스트 구분자는 **마지막 `@`**를 쓰므로 비밀번호 안의 `@`는 그대로 통한다 — `scott/p@ss@dbhost:1521/ORCL`은 올바로 해석된다. `/`나 `:`가 든 비밀번호는 URL 형식 + 퍼센트 인코딩을 쓴다.

#### (d) Oracle 관리자 접속 · 방언 강제

```sh
# AS SYSDBA — 문자열 끝에 붙인다
nsql run -c "oracle://sys:pw@dbhost:1521/ORCL as sysdba" admin.sql

# 사용자를 나중에 물어보게(빈 사용자)
nsql shell -c "oracle://@dbhost:1521/ORCL"

# 스킴 없이 방언만 강제 — ?dialect= 또는 ?type=
nsql run -c "scott:tiger@dbhost:1521/ORCL?dialect=oracle" script.sql
```

> ⚠️ **`@`는 생략할 수 없다.** `dbhost:1521/ORCL?dialect=oracle`처럼 `@` 없이 쓰면 `dbhost`를 **사용자명**으로 읽는다. 사용자가 없으면 `@dbhost:1521/ORCL`처럼 `@`로 시작한다.

#### (e) 셸별 따옴표

접속 문자열에는 `/` `:` `@` `?` `&`가 들어가므로 **항상 따옴표로 감싼다.**

| 셸 | 쓰기 |
|---|---|
| bash · zsh | `-c "oracle://u:p@h:1521/ORCL"` — `$`가 있으면 `'...'`(단, 변수 확장도 막힘) |
| PowerShell | `-c "oracle://u:p@h:1521/ORCL"` — 변수는 `"...$env:DBPW..."` · **쉼표 목록(`--map a=b,c=d` · `--cols a,b`)도 따옴표로**(안 감싸면 배열로 쪼개진다) |
| cmd.exe | `-c "oracle://u:p@h:1521/ORCL"` — `%`는 `%%`로 이스케이프 |

```powershell
# PowerShell — 환경변수로 비밀번호
$env:DBPW = 's3cr3t'
nsql run -c "postgres://app:$env:DBPW@dbhost:5432/appdb" --no-prompt migrate.sql
```

#### (f) 서버 없이 바로 해 보기

붙을 서버가 아직 없어도 직접 접속 형식은 SQLite로 연습된다:

```sh
echo "SELECT 'direct' AS how, 1+1 AS n;" | nsql run -c "sqlite::memory:" -
```

```
Connected: sqlite://:memory: (sqlite)
how     n
------  -
direct  2

1 rows (0.000s)
```

`Connected: …` 줄은 **stderr**다(§4-6) — 파이프로 넘기면 데이터만 남는다.

#### (g) 문자열 대신 필드로

문자열 조립이 번거로우면 필드로 준다. **`conn` 전용이 아니라 `run` `shell` `export` `import` `explain` `conn`**이 받는다(`--host` `--port` `--db`/`--database` `-u`/`--user` `-p`/`--password`). (출력 생략 · 실서버 필요)

```sh
nsql run   -d postgres --host 172.16.1.60 --port 5433 --db repodb --user postgres migrate.sql
nsql shell -d oracle   --host 172.16.1.58 --port 1521 --db APPDB --user APPDB
nsql conn test -d oracle --host 172.16.1.58 --port 1521 --db APPDB --user APPDB   # 비밀번호는 물어봄
```

포트를 빼면 방언 기본값(1521 · 1433 · 5432 · 3306)이 쓰인다. 파일 방언도 필드로 된다 — `nsql conn test -d sqlite --db t1.db`:

```
OK  sqlite://t1.db  (sqlite · 0.000s)  sqlite t1.db
```

> `cat`의 도움말 옵션에는 필드 옵션이 없다 — `cat`은 `-c <문자열|프로필>`로 준다.
> ⚠️ 반대로 **`conn test`는 저장 프로필 이름 또는 필드 옵션만** 받는다(0.2.8) — `nsql conn test -c sqlite:t1.db`는 `nsql conn test <name>  또는  nsql conn test -d … --host … - 접속 문자열 또는 --host/--db 등 필드가 필요합니다`(종료 2), `nsql conn test sqlite:t1.db`는 프로필 이름 규칙 오류(종료 1)다. 문자열로 시험하려면 `echo "SELECT 1;" | nsql run -c "<문자열>" -`.

#### (h) 드라이버 지정 — `?driver=` (10-23)

같은 방언에 드라이버가 여럿이면(Oracle = Thin/OCI) 접속 대상 끝에 `?driver=<id|thin|oci>`를 붙여 **그 접속에만** 고른다([21 §8](21-connection-profiles.md)). 없으면 설정(`oracle.driver` · 기본 thin)을 따른다.

```sh
nsql run -c "oracle://scott@db:1521/orcl?driver=oci" q.sql     # Instant Client(OCI)
nsql run -c "oracle://scott@db:1521/orcl?driver=thin" q.sql    # 순수 Rust Thin(확장 oracle-thin)
nsql run -c "duckdb::memory:?driver=duckdb" -                   # 축약형 꼬리도 된다
```

> ⚠️ 0.2.8 확인 결과: `duckdb::memory:?driver=duckdb`는 동작했으나 **`sqlite::memory:?driver=sqlite` · `sqlite:t1.db?driver=sqlite`는 `?driver=…`까지 파일 이름으로 읽어** `[SQLITE 14 · 접속 끊김] unable to open database file: t1.db?driver=sqlite`로 실패했다 — SQLite 대상에는 `?driver=`를 붙이지 않는다.

### 1-2. 프로필 — 한 번 저장하고 이름으로 부르기

비밀번호가 **암호화되어 사용자 폴더에 저장**되고, **CLI와 GUI가 같은 폴더를 공유**한다([21](21-connection-profiles.md) · [DR-22](10-decision-record.md)). 손으로 따라 하는 SQLite 예는 Step 16.

```sh
# 접속 문자열로 저장(비밀번호가 없으면 숨김 입력으로 묻는다)
nsql conn add prod "oracle://APPDB@172.16.1.58:1521/APPDB"

# 필드로 저장
nsql conn add pg1 -d postgres --host 172.16.1.60 --port 5433 --db repodb --user postgres

# 확인·시험·서버 유형·삭제
nsql conn list
nsql conn show prod
nsql conn test prod          # 실제로 붙어 본다
nsql conn env prod prd       # 서버 유형 = 운영(Production · PRD) — 영속
nsql conn rm  prod
nsql conn path               # 저장 폴더 경로
```

`conn list` 출력(열 = 이름 · 방언 · **서버 유형** · 비밀번호 저장 여부 · 대상):

```
NAME    DIALECT  TYPE  PW   TARGET
lite    sqlite   DEV   -    t1.db
pgdemo  postgres  -     ✓    app@db.example:5432/appdb
```

`PW` 열의 `✓`는 비밀번호가 저장됐다는 뜻, `-`는 실행 시 묻는다(또는 필요 없음)는 뜻이다. `TYPE`은 `conn env`로 정한 서버 유형의 3자리 약어(`DEV` `TST` `PRD` · 없음 = `-`)다 — GUI는 운영(PRD)이면 칩을 띄우고 변경을 2단으로 확인한다([115](115-prod-mode-scenario-nsql-guide.md)).

이후 어디서나 이름으로 쓴다:

```sh
nsql run    -c prod script.sql
nsql export -c prod -t emp -f csv -o emp.csv
nsql shell  -c prod
```

스크립트 안에서도 쓴다 — `CONNECT prod`(셸 별칭 `\c prod`).

**저장 위치**

| OS | 경로 |
|---|---|
| Windows | `%APPDATA%\nexa-sql\profiles\<해시>.conf` |
| macOS | `~/Library/Application Support/nexa-sql/profiles/` |
| Linux | `~/.config/nexa-sql/profiles/` |

파일 이름은 이름의 해시(SHA-256 앞 16 hex)이고 이름은 파일 안 `name=`에 있다(09-22 · 종전 `<이름>.conf`는 읽기 호환). `nsql conn path`는 설정 폴더(그 아래 `profiles/`)를, `nsql conn show <이름>`은 그 프로필의 파일 경로를 알려 준다. `NSQL_HOME`을 설정하면 그 폴더로 바뀐다(테스트·개발용). 비밀번호만 봉투로 암호화되고 호스트·사용자명은 평문이다(목록에 보여야 하는 값). Windows는 DPAPI 기기 키라 **같은 Windows 계정에서만** 풀린다.

### 1-3. 둘 중 무엇을 쓰나

| 상황 | 권장 |
|---|---|
| 일회성 · 남기고 싶지 않음 | 접속 문자열 |
| 매일 쓰는 서버 · GUI와 공유 | 프로필 |
| CI · 컨테이너 | 접속 문자열 + `--no-prompt`(비밀번호는 `NSQL_PASSWORD` 또는 `--password-stdin`) |

---

## 2. 순서대로 — 서버 없이 되는 점검 (SQLite)

여기까지는 **DB 서버가 없어도** 전부 된다(DuckDB 예만 드라이버를 내려받는 네트워크가 필요). 위에서부터 차례로 실행하면 기능이 하나씩 늘어난다. 아래 출력은 전부 0.2.8 · `ui.lang=ko`에서 실제로 뜬 것이다(영어면 메시지 문구만 영어).

### Step 1 — 엔진만 시험 (접속조차 불필요 · `plan`)

`plan`은 스크립트를 **실행하지 않고** 문장 분할·세션 변수·방언별 재작성 계획만 보여준다. 세션 변수 엔진([08](08-session-variables.md))을 검증하는 가장 빠른 방법이다. 옵션은 `-d <방언>`(기본 oracle)과 위치 인자(`&1 &2 …`)뿐이다.

```sh
nsql plan -d mssql examples/golden-session-vars.sql
nsql plan -d oracle examples/golden-session-vars.sql
nsql plan -d mssql session-vars.sql 2026 Q1        # 위치 인자 → &1 &2
```

```
# dialect=mssql items=6

[1] line 2 · Exec
  → LOCAL  :V_PRG_NM = N'SP_DMP_MPO_DME_CREATE_BSY'

[3] line 6 · Exec
  → EXECUTE (Block · Bind · out=true)
    | EXEC SP_DMP_MP_VERSION_CREATE(@PC_RET, 'ACME', 'DP_202604_W16_V01', 'W', '20260415', '20260705', '', 'user01')
    · PC_RET Auto InOut = NULL
    ! 암묵 변수: PC_RET
…
[6] line 41 · Connect
  → CONNECT user/***@192.168.1.1:1521/db

# variables=9 defines=0 errors=0
  :PC_RET Auto = 
  :V_PRG_NM Varchar2(25) = SP_DMP_MPO_DME_CREATE_BSY
…
```

같은 줄 3이 `-d oracle`이면 `BEGIN SP_DMP_MP_VERSION_CREATE(:PC_RET, …); END;`로, 줄 10의 `SELECT … INTO`는 MSSQL에서 `SELECT @V_… = …` + `@@ROWCOUNT` 검사(`THROW 51403`/`51422`)로 재작성된다.

★ **스크립트 엔진을 고쳤으면 두 방언 모두 다시 볼 것**(CLAUDE.md 규약).

### Step 2 — 실제 실행 (메모리 DB · `run`)

```sh
nsql run -c "sqlite::memory:" examples/sqlite-session-vars.sql
```

```
Connected: sqlite://:memory: (sqlite)
0 rows affected (0.001s)
3 rows affected (0.000s)
:V_DEPT = DEV
0 rows affected (0.000s)
V_DEPT = DEV
V_MAX = 520
name    sal
------  ---
김철수  450

1 rows (0.000s)

dept  cnt
----  ---
DEV     2

1 rows (0.000s)
…
variable V_DEPT VARCHAR2(3)
variable V_MAX NUMBER
```

세션 변수 대입 → `PRINT` → 바인드 조회까지 한 번에 확인된다. 문장은 SQL\*Plus처럼 `;` · 단독 `/` · `GO`로 나뉜다.

### Step 3 — 파일 DB로 표 만들기

```sh
cat > t1.sql <<'SQL'
CREATE TABLE emp (id INTEGER PRIMARY KEY, name TEXT, dept TEXT, sal INTEGER);
INSERT INTO emp VALUES (1,'Kim','SALES',5000),(2,'Lee','IT',6200),(3,'Park','IT',4800);
SELECT * FROM emp ORDER BY sal DESC;
SQL
echo "SELECT * FROM emp ORDER BY sal DESC;" > q.sql

nsql run -c "sqlite:t1.db" t1.sql
```

```
Connected: sqlite://t1.db (sqlite)
0 rows affected (0.001s)
3 rows affected (0.000s)
id  name  dept    sal
--  ----  -----  ----
 2  Lee   IT     6200
 1  Kim   SALES  5000
 3  Park  IT     4800

3 rows (0.000s)
```

이후 단계는 이 `t1.db`와 `q.sql`을 쓴다.

### Step 4 — 출력 형식 (`-f`) · 레코드 보기 (`-x`)

```sh
nsql run -c "sqlite:t1.db" -f json     q.sql
nsql run -c "sqlite:t1.db" -f csv      q.sql
nsql run -c "sqlite:t1.db" -f tsv      q.sql
nsql run -c "sqlite:t1.db" -f jsonl    q.sql
nsql run -c "sqlite:t1.db" -f markdown q.sql
nsql run -c "sqlite:t1.db" -f sql:insert q.sql      # 행마다 실행문(§4-3)
nsql run -c "sqlite:t1.db" -x q.sql                 # 레코드 보기
```

```json
[
  {"id": 2, "name": "Lee", "dept": "IT", "sal": 6200},
  {"id": 1, "name": "Kim", "dept": "SALES", "sal": 5000},
  {"id": 3, "name": "Park", "dept": "IT", "sal": 4800}
]
```

```
-[ RECORD 1 ]----
id   | 2
name | Lee
dept | IT
sal  | 6200
-[ RECORD 2 ]----
…
```

`grid`(기본 · `cli.format`)·`markdown`은 사람용이고 끝에 `3 rows (…)` 요약이 붙는다. `csv` `tsv` `json` `jsonl` `insert[:TABLE]`은 파이프·CI용이라 데이터만 낸다. 형식 전체는 §4.

### Step 5 — 행 수 상한 (`--max-rows`)

```sh
nsql run -c "sqlite:t1.db" --max-rows 2 q.sql
```

```
id  name  dept    sal
--  ----  -----  ----
 2  Lee   IT     6200
 1  Kim   SALES  5000

2 rows (0.000s) (더 있음 - --max-rows를 올리세요)
```

잘렸다는 사실을 **명시**한다 — 조용히 자르지 않는다. `0` = 무제한(`run`·`export`의 기본은 무제한 · GUI는 `grid.max_rows`).

### Step 6 — 성능 계측 (`--timing`)

```sh
nsql run -c "sqlite:t1.db" --timing q.sql
```

```
Connected: sqlite://t1.db (sqlite)
⏱ connect 0.5ms
⏱ startup settings 0.2ms · opts 0.1ms · runner 4.4ms · connect 0.5ms · total 5.2ms
…
3 rows (0.000s)

⏱ execute 322µs (3 rows) · commit 34µs · total 356µs
```

단계별 시간(기동 · 접속 · 실행 · 커밋 …)은 `nsql_core::Timeline`이 쌓고 stderr로 나간다([26 성능](26-performance-architecture.md)). 실행 경로를 고쳤으면 여기로 확인한다.

### Step 7 — 실행 로그 (`--log`)

```sh
echo "SELECT 1 AS a;" | nsql run -c "sqlite::memory:" --log -
```

```
2026-10-23 19:37:40.513  connect   sqlite://:memory: (sqlite) · schema main
2026-10-23 19:37:40.514  send      1행: Query
2026-10-23 19:37:40.514  done      1 rows  113µs  결과 집합
a
-
1

1 rows (0.000s)

2026-10-23 19:37:40.514  execute   1 rows  113µs  첫 응답 · execute+fetch
2026-10-23 19:37:40.514  commit    40µs  커밋 · read tx end (rollback)
```

GUI 로그 창과 같은 내용이다. 형식은 `log.format` 설정(`raw` `markdown` `grid` `compact` `jsonl` `csv` `tsv` `template`)을 따른다.

### Step 8 — 치환 변수 · 위치 인자 · 오류와 종료 코드

```sh
echo "SELECT '&1' AS first_arg, '&2' AS second_arg, '&who' AS who;" > args.sql
nsql run -c "sqlite::memory:" -v who=nsql args.sql 2026 Q1
```

```
first_arg  second_arg  who
---------  ----------  ----
2026       Q1          nsql
```

- `[args...]` = `&1 &2 …` · `-v 이름=값`(여러 번) = 스크립트가 돌기 전에 `DEFINE`과 같은 저장소에 정의.
- `--no-prompt`면 정의되지 않은 치환 변수는 빈 값(묻지 않음).

오류가 나도 **다음 문장은 계속** 돌고 종료 코드가 1이 된다. `WHENEVER SQLERROR EXIT`이면 그 자리에서 멈춘다.

```
$ nsql run -c sqlite::memory: err.sql          # SELECT 1 · SELECT * FROM nope · SELECT 2
ok
--
 1
…
ERROR line 2: [SQLITE 1 · 테이블/뷰 없음: nope] no such table: nope
ok2
---
  2
…                                               # 종료 코드 1

$ nsql run -c sqlite::memory: err2.sql         # 첫 줄 WHENEVER SQLERROR EXIT
ERROR line 2: [SQLITE 1 · 테이블/뷰 없음: nope] no such table: nope   # 종료 코드 1 · 뒤 문장 안 돎
```

**종료 코드**(`run`): 0 = 전 항목 성공 · 1 = 하나 이상 실패(실패마다 stderr에 줄 번호) · 2 = 사용법·접속 오류(예: 없는 프로필 → `프로필 'nosuchprof'이(가) 없습니다 - \`nsql conn list\`로 확인하거나 접속 문자열을 쓰세요`).

### Step 9 — 카탈로그 탐색 (`cat`)

GUI 오브젝트 탐색기와 같은 질의(`nsql-catalog`)다. 옵션 = `-c` `-d` `-s <스키마>` `-f` `--width` `--max-col-width` `--overflow` `-x`.

```sh
nsql cat -c "sqlite:t1.db" schemas
nsql cat -c "sqlite:t1.db" kinds
nsql cat -c "sqlite:t1.db" tables
nsql cat -c "sqlite:t1.db" columns emp
```

```
Kind     Folder
-------  --------
table    Tables
view     Views
index    Indexes
trigger  Triggers

Schema  Name  Status  Modified  Extra
------  ----  ------  --------  -----
main    emp                     emp

#  Name  Type     Nullable  Default
-  ----  -------  --------  -------
1  id    INTEGER  Y
2  name  TEXT     Y
3  dept  TEXT     Y
4  sal   INTEGER  Y
```

`kinds`는 **이 방언이 지원하는 종류**만 낸다(SQLite = 4종). 전체 종류: `tables views mviews procs funcs packages bodies sequences triggers indexes synonyms types` · `external_tables foreign_tables aggregates queues dblinks java jobs scheduler_jobs scheduler_programs schema_triggers extensions event_triggers`.

**SQL 생성(`gen`)·상세(`detail`)** — GUI 우클릭 "SQL 생성"·객체 상세 패널과 같다:

```sh
nsql cat -c "sqlite:t1.db" gen select emp
nsql cat -c "sqlite:t1.db" gen ddl emp
nsql cat -c "sqlite:t1.db" detail emp
```

```
-- Bind variables (1): :id
SELECT
    E.id,
    E.name,
    E.dept,
    E.sal
FROM main.emp E
WHERE E.id = :id;
-- End of statement · bind variables (1): :id

-- main.emp definition

CREATE TABLE emp (id INTEGER PRIMARY KEY, name TEXT, dept TEXT, sal INTEGER);

== Properties ==
Property  Value
------------------------------------------
name      main.emp
size      4K · 4,096 bytes · 1 blocks × 4K

== PrimaryKey ==
Name         Detail
-------------------
PRIMARY KEY  id
```

| 하위 명령 | 뜻 |
|---|---|
| `columns <객체>` | 컬럼: 이름 · 형 · NULL 허용 · 기본값 |
| `sub <객체> <하위종류> [kind]` | `constraints` `unique_keys` `check_constraints` `foreign_keys` `references` `indexes` `triggers` `partitions` `dependencies` `rules` `policies` `extended_properties` `arguments` `attributes` `methods` `procedures` `functions`(kind = 주인 종류 · 기본 table) |
| `gen <what> <객체> [kind] [하위종류 하위이름]` | `select` `insert` `update` `delete` `merge` `call` `ddl` · 옵션 `k=v`: `qualified` `compact` `full` `fk` `note` `alias=abbr\|letters` `upper` `format` |
| `detail <객체> [kind]` | 속성 · 컬럼 · 제약/인덱스/트리거/인자 … · 소스 또는 DDL을 섹션으로 |
| `source <kind> <이름>` | 프로시저 · 함수 · 패키지 · 트리거 · 뷰 …의 소스 |
| `errors <이름>` | 저장 오브젝트의 컴파일 오류(Oracle `USER_ERRORS`) |

실서버 예(출력 생략 · 실서버 필요):

```sh
nsql cat -c prod -s HR tables                        # 스키마 지정
nsql cat -c prod sub EMPLOYEES foreign_keys
nsql cat -c prod gen merge EMPLOYEES
nsql cat -c prod gen call PKG_ORDER package
nsql cat -c prod source packages PKG_ORDER           # 원본 소스
nsql cat -c prod errors MY_PROC                      # 컴파일 오류(Oracle)
nsql cat -c PLANDB source procs PLANDB_MS.dbo.MY_PROC   # 3부 이름(SQL Server·MySQL)
nsql cat -c prod -f json tables                      # 형식 지정
```

`source` · `gen ddl` · `detail` · `errors`는 **3부 이름**(`DB.schema.name` · `DB..name` = 기본 스키마)을 받는다(SQL Server·MySQL · 10-12 121차) — 접속한 DB와 다르면 `USE [DB]`로 바꾼 뒤 읽는다(`nsql import -t DB.s.t`와 같은 규칙).

### Step 10 — 실행 계획 (`explain`)

```sh
nsql explain -c "sqlite:t1.db" -q "SELECT * FROM emp WHERE dept='IT'"
nsql explain -c "sqlite:t1.db" q.sql            # 문장이 든 파일
```

```
id  parent  notused  detail
--  ------  -------  --------
 2       0        0  SCAN emp

id  parent  notused  detail
--  ------  -------  ----------------------------
 3       0        0  SCAN emp
12       0        0  USE TEMP B-TREE FOR ORDER BY
```

방언별 관용을 알아서 쓴다(Oracle `EXPLAIN PLAN` + `DBMS_XPLAN` · SQL Server `SHOWPLAN` · PostgreSQL `EXPLAIN` · SQLite `EXPLAIN QUERY PLAN`).

> ★ **10-23(137차) 실행 계획 검토** — 위 원문 표는 이제 `--raw`일 때의 출력이다. 기본 출력 = **공통 트리 + 자동 검토**(`실행 계획` · `검토` · `[HIGH]`/`[WARN]`/`[INFO]`) · `--analyze` = 실제 실행 계획(문장을 실행 · stderr 경고) · `--raw` = DBMS 원문 그대로. 방언별 원문·파싱·검토 규칙 7·해석 요령·출력 예 = **[122 실행 계획 검토](122-execution-plan-review.md)**.

```sh
nsql explain -c "sqlite:t1.db" -q "SELECT * FROM emp WHERE dept='IT'"            # 트리 + 검토
nsql explain -c "sqlite:t1.db" -q "SELECT * FROM emp WHERE dept='IT'" --raw      # 원문(위 표)
nsql explain -c prod -q "SELECT …" --analyze                                   # 실제 실행 계획
```

### Step 11 — 내보내기 (`export`)

`-q <sql>`과 `-t <table>` 중 하나 · 기본 형식 **csv**(`cli.format`이 grid가 아니면 그 값) · `-o`가 없으면 stdout.

```sh
nsql export -c "sqlite:t1.db" -t emp                               # 표 전체 → 화면(csv)
nsql export -c "sqlite:t1.db" -t emp -f csv -o emp.csv             # 파일로
nsql export -c "sqlite:t1.db" -t emp -f tsv -o emp.tsv
nsql export -c "sqlite:t1.db" -t emp -f jsonl -o emp.jsonl
nsql export -c "sqlite:t1.db" -q "SELECT * FROM emp WHERE sal>5000" -f json -o rich.json
nsql export -c "sqlite:t1.db" -t emp -f insert:NEW_EMP -o seed.sql # INSERT 문 생성
```

```
id,name,dept,sal
1,Kim,SALES,5000
2,Lee,IT,6200
3,Park,IT,4800
```

`-o`로 쓰면 stderr에 `3 rows → emp.csv`가 남는다. `seed.sql`:

```sql
INSERT INTO NEW_EMP (id, name, dept, sal) VALUES (1, 'Kim', 'SALES', 5000);
INSERT INTO NEW_EMP (id, name, dept, sal) VALUES (2, 'Lee', 'IT', 6200);
INSERT INTO NEW_EMP (id, name, dept, sal) VALUES (3, 'Park', 'IT', 4800);
```

- 형식: `csv` `tsv` `json` `jsonl` `insert[:테이블명]`(그 밖의 `-f` 값도 공통 형식 표 §4-1을 따른다).
- `--fast` = 서버가 csv/tsv를 만들어 흘린다(PostgreSQL `COPY TO`). 지원하지 않는 DBMS면 안내 후 일반 경로 — SQLite: `--fast를 지원하지 않는 DBMS - 일반 경로로`.
- `--max-rows N`으로 상한 · 접속 옵션(`--host` … `--password-stdin` `--no-prompt`)은 `run`과 같다.
- ★ **스트리밍(10-30 · 180차 · T-8)** — 질의가 **한 문장**이고 형식이 `csv`·`tsv`·`json`·`jsonl`·`insert`이면 결과를 메모리에 모으지 않고 **배치마다 바로 쓴다**(배치 = `db.fetch_size`와 2000 중 큰 쪽 · 메모리 = 배치 하나). 서버 커서가 있는 드라이버(PostgreSQL · Oracle · Db2 · SQLite)는 커서로 이어 받고, 없는 드라이버(SQL Server · MySQL · ODBC · ClickHouse · DuckDB)는 **OFFSET 페이지 재질의**로 이어 받는다 — 이 경우 페이지 사이에 데이터가 바뀌면 행이 겹치거나 빠질 수 있으므로 **`ORDER BY`(유일 키)를 붙이는 것을 권장**. 여러 문장 · 프로시저 · `grid`/`md`/`sql:*` 형식 = 종전 일괄 경로(결과 전체를 모은 뒤 씀) · `--fast` = 그대로. 출력은 일괄 경로와 바이트 단위로 같다(시험).

### Step 12 — 대량 적재 (`import`)

csv/tsv/jsonl 파일(또는 `-` = 표준 입력)을 표에 **INSERT로만** 넣는다([89 대량 I/O](89-bulk-io-review.md)). 첫 레코드 = 헤더(열 이름)이고 표의 열 이름과 맞춘다.

| 옵션 | 뜻 · 기본값(설정 `bulk.*`) |
|---|---|
| `-t <table>` | 넣을 표(필수 · `DB.s.t` 3부 이름 가능) |
| `-f csv\|tsv` | 원료 형식(jsonl은 `-f jsonl`) |
| `--no-header` | 첫 레코드도 데이터 → 표 열 순서로 넣는다 |
| `--cols a,b,…` | 헤더가 없을 때 원료 열 이름 순서 |
| `--map src=dst,…` | 원료 열 → 표 열 이름 바꾸기 |
| `--batch N` | 배치(문장)당 행 수 · `bulk.batch_rows` = 1000 |
| `--commit-every N` | N행마다 커밋 · `0` = 끝에 한 번 · `bulk.commit_every` = 10000 |
| `--mode auto\|driver\|multirow\|single` | 적재 경로 · `auto` = 드라이버 최속 경로(Oracle 배열 DML · PostgreSQL COPY …)가 있으면 그것, 없으면 다중 행 INSERT · `bulk.mode` = auto |
| `--empty-null on\|off` | 빈 필드 = NULL(on) / 빈 문자열(off) · `bulk.empty_null` = on(숫자·시각 필드는 비면 늘 NULL) |
| `--timing` | 단계별 소요 → stderr |

```sh
# Step 11에서 만든 emp.csv(id,name,…)를 열 이름이 다른 표에
echo "CREATE TABLE emp2 (emp_no INTEGER PRIMARY KEY, ename TEXT, dept TEXT, sal INTEGER);" | nsql run -c "sqlite:t2.db" -
nsql import -c "sqlite:t2.db" -t emp2 --map "id=emp_no,name=ename" emp.csv
```

```
Connected: sqlite://t2.db (sqlite)
3행을 emp2에 적재 · 0.001s(2488/s · multirow · 배치 1)
```

```sh
# 헤더 없는 파일 · 열 지정 · 한 행씩(진단용)
printf '10,Choi,HR,3900\n11,Jung,HR,\n' > nohdr.csv
nsql import -c "sqlite:t2.db" -t emp2 --no-header nohdr.csv
printf '20,Han\n21,Yoon\n' > nohdr2.csv
nsql import -c "sqlite:t2.db" -t emp2 --no-header --cols "emp_no,ename" --mode single nohdr2.csv

# tsv · 배치 2행 · 끝에 한 번 커밋 · 계측
nsql import -c "sqlite:t2.db" -t emp2 -f tsv --map "id=emp_no,name=ename" --batch 2 --commit-every 0 --timing more.tsv

# jsonl · 표준 입력
nsql import -c "sqlite:t2.db" -t emp3 -f jsonl --map "id=emp_no,name=ename" emp.jsonl
cat emp.csv | nsql import -c "sqlite:t2.db" -t emp4 --map "id=emp_no,name=ename" -   # emp4는 emp2와 같은 꼴로 미리 만든 표
```

```
2행을 emp2에 적재 · 0.001s(1430/s · multirow · 배치 1)
2행을 emp2에 적재 · 0.001s(1529/s · single · 배치 2)
⏱ connect 1.3ms
3행을 emp2에 적재 · 0.001s(2092/s · multirow · 배치 2)
⏱ send 850µs · execute 574µs · total 1.4ms
3행을 emp3에 적재 · 0.004s(723/s · multirow · 배치 1)
3행을 emp4에 적재 · 0.002s(1579/s · multirow · 배치 1)
```

**실패하면** 그 배치를 한 행씩 다시 넣어 **문제 행·줄**을 알리고 종료 코드 1 — 이미 커밋된 행은 남는다(`--commit-every`가 작을수록 잃는 행이 적다):

```
$ nsql import -c sqlite:t2.db -t emp2 --batch 10 dup.csv      # 50 · 2(중복) · 51
오류: 2번째 행(줄 3): UNIQUE constraint failed: emp2.emp_no - 그 앞 1행은 커밋됨
```

> `--mode driver`는 드라이버 최속 경로만 쓴다 — SQLite처럼 없으면 `오류: 0번째 행(줄 0): bulk load: driver sink not supported - 그 앞 0행은 커밋됨`(종료 1). 보통은 `auto`로 둔다.
> `--no-header`를 빼먹으면 첫 줄 값(`10` …)을 열 이름으로 읽어 `emp2에 10 열이 없습니다(--map 또는 --cols)`(종료 2)로 멈춘다 — 아무것도 넣지 않는다.

SQL Server·Oracle 전용 설정(`bulk.check_constraints` · `bulk.fire_triggers` · `bulk.append_hint`)은 `nsql config list`의 `[접속 ▸ CLI]` 묶음에 있다.

### Step 13 — SQL 포맷 (`format`)

접속 없이 GUI와 같은 내장 Basic 포맷터로 정리한다. 설정 `format.*`(키워드 대소문자 · 들여쓰기 …)를 그대로 쓴다.

```sh
echo "select e.id,e.name,d.dname from emp e join dept d on d.id=e.dept_id where e.sal>5000 and d.loc='SEOUL' order by e.sal desc;" > ugly.sql
nsql format ugly.sql                      # → stdout
nsql format ugly.sql -o pretty.sql        # → 파일
nsql format ugly.sql --in-place           # 원 파일에 덮어씀
cat q.sql | nsql format - -o out.sql      # 표준 입력
```

```
SELECT
	e.id,
	e.name,
	d.dname
FROM
	emp e
JOIN dept d
	ON d.id = e.dept_id
WHERE
	e.sal > 5000
	AND d.loc = 'SEOUL'
ORDER BY
	e.sal DESC;
```

모양을 바꾸려면 `nsql config set format.keyword_case lower`(`keep|upper|lower` · 기본 upper) · `format.identifier_case` · `format.indent tab|space|editor` · `format.indent_width`(Step 15).

### Step 14 — 대화형 셸 (`shell`)

```sh
nsql shell -c "sqlite:t1.db"
```

```
Connected: sqlite://t1.db (sqlite)
nsql shell - `;`·단독 `/`·명령 줄로 실행 · exit 종료 · 변수는 세션 동안 유지 · 셸 명령은 \? · set width <n|auto> · \x · show
nsql> SELECT name, sal FROM emp WHERE dept='IT';
name   sal
----  ----
Lee   6200
Park  4800

2 rows (0.000s)

nsql> exit
```

- 실행: 줄 끝 `;` · 단독 `/` · `GO`
- 종료: `exit` / `quit` / `\q`(미커밋 변경은 커밋)
- **세션 변수·접속은 셸이 살아 있는 동안 유지**된다
- 옵션은 `run`과 같다(접속 · `-f` · `--max-rows` · `--width` · `--max-col-width` · `--overflow` · `-x` · `--timing` · `--log` · `-v`)
- 파이프도 된다: `printf 'SELECT 1;\nquit\n' | nsql shell -c "sqlite::memory:"` — 입력이 터미널이 아니면 프롬프트(`nsql> `·`   -> `)를 찍지 않는다(결과·오류만 · D-396)
- ★ **줄 편집기·Tab 완성(10-29 · T-453 1차)** — 터미널에서 띄우면 자체 줄 편집기가 켜진다(외부 라이브러리 없음 · 3-OS): ←→ · Home/End · Backspace/Delete · Ctrl+A/E/B/F/U/K/W · **↑↓ = 기록**(설정 폴더 `shell_history` · `cli.history_max` 기본 500 · 0 = 끔) · **Tab = 완성**(셸 명령 `\d` `.tables` … · 키워드 · 테이블·뷰 · `별칭.`/`스키마.` 뒤 컬럼·객체 — GUI 코드 완성과 같은 문맥 판정 · 메타는 첫 Tab 때 읽음 · 후보 여럿이면 공통 접두까지 채우고 표로 보여 줌) · **Ctrl+R = 기록 거꾸로 찾기**(글자를 칠 때마다 최신 일치 · Ctrl+R 반복 = 더 오래된 일치 · Enter = 확정 · 이동 키 = 찾기 끝내고 그 줄 편집 · Ctrl+C = 원래 줄로) · ★ **Ctrl+Z(·Ctrl+_) = 되돌리기 · Ctrl+Y = 다시(이어 친 글자·이어 지운 글자 = 한 묶음 · Enter·Ctrl+C로 비움) · Alt+Y = 마지막으로 지운 글 붙이기(Ctrl+K/U/W · Alt+D · Alt+Backspace로 지운 것) · Alt+←/→ · Ctrl+←/→ · Alt+B/F = 단어 이동 · Alt+D = 뒤 단어 지우기 · Alt+Backspace = 앞 단어 지우기(단어 = 영숫자·`_`·한글 · Ctrl+W는 공백 기준)**(10-29 · Windows는 콘솔을 직접 읽어 Ctrl+Z가 셸을 끝내지 않음 · 받은 키를 보려면 `NSQL_KEY_TRACE=<파일>`) · ★ **여러 줄 문장(10-30)** = Alt+Enter = 줄 바꿈 삽입(이음 줄 프롬프트 `   -> `) · ↑↓ = 문장 안 줄 이동 → 첫/끝 줄에서 기록 · Home/End = 그 줄 안 · 기록 한 항목 = 문장 하나(↑로 여러 줄이 통째로 돌아옴) · Ctrl+C = 줄 취소 · 빈 줄에서 Ctrl+D = 종료 · 한글 폭 2칸 · 긴 줄은 접어서 다시 그림. 파이프·리다이렉트(비터미널)는 종전처럼 줄 단위로 읽고 기록하지 않는다.
- ★ **Tab 별칭·대소문자 = GUI 설정(10-29)** — `nsql config set intel.insert_alias on`(기본 off · GUI와 공유)이면 FROM/JOIN 자리에서 테이블 후보 하나로 확정될 때 ` A`처럼 별칭을 붙인다(모양 = `intel.alias_style` abbr|letters · `intel.alias_upper` · 같은 문장의 별칭은 건너뜀 · 뒤에 이미 별칭이 있으면 이름만) · 후보 대소문자 = `intel.insert_case` · 설정은 셸을 띄울 때 읽는다(바꾼 뒤 다시 띄움 · 셸 안 `set`/`get`은 출력 설정 전용).
- ★ **`SHOW VERSION` · `SHOW AUTOCOMMIT`(10-29)** — `SHOW VERSION`(`VER` · `SERVER`) = `클라이언트: nsql 0.3.1 (full|lite)` + `서버: <방언> <버전> (<에디션>) · user=… · db=…`(서버 1왕복) · `SHOW AUTOCOMMIT`(`AUTO`) = `autocommit ON|OFF` · 접속 줄 끝에 `· autocommit on|off` · **셸의 자동 커밋 = 설정 `session.autocommit`(기본 on)** · `nsql run`·export·import = 종전대로 수동 + 끝날 때 커밋 · 셸·스크립트 안 `SET AUTOCOMMIT ON|OFF`가 우선 · `nsql --help` 첫 줄과 셸 `\?` 머리에 프로그램 판.
- ★ **트랜잭션 프롬프트 · 프로필 autocommit · 필수 항목 검사(10-30 · 180차)** — 수동 커밋에서 트랜잭션이 열리면 프롬프트가 `nsql*3> `(변경 문장 3개 · 읽기만 = `nsql*> ` · 커밋/롤백/자동 커밋 = `nsql> ` · D-401) · 접속 문자열 `?autocommit=on|off`(`true/false/1/0/yes/no`) 또는 볼트 프로필 키 `autocommit=` = 접속 직후 그 모드(설정 `session.autocommit`보다 우선 · 이후 `SET AUTOCOMMIT`은 종전대로 · D-399) · 필수 항목(사용자·호스트 등)이 빠진 접속은 서버로 아무것도 보내지 않고 `Missing required connection field(s) for <방언>: …` + 종료 코드 2(Oracle `user@TNS`·`/ as sysdba`는 예외 · D-400).
- ★ **셸 `show` = 항목 표(10-30)** — 셸 `show`(·`set`·`get` 단독) = **항목별 표**(열 = 항목 · 현재값 · 설정 가능한 값 · 설명 · 행 = width · colwidth · overflow · format · max_rows · pager · autocommit) + 꼬리 "바꾸기: set … · 영구: nsql config set cli.<항목> · autocommit: SET AUTOCOMMIT" · `get <항목>` = 그 줄만(`autocommit`/`auto`도) · `set <항목> <값>` 뒤 = 바뀐 항목 줄 · `\x` = overflow 줄 · 종전 한 줄 요약 + set 도움말 두 줄을 대체.

```
항목        현재값  설정 가능한 값                               설명
width       140     <n> | auto (0 = 무제한)                      표 한 줄 폭(auto = 터미널 폭)
format      grid    grid | markdown | csv | tsv | json | jsonl  결과 출력 형식
autocommit  ON      ON | OFF                                     문장마다 커밋
…
```

(위 표는 모양 예시 — 정확한 문구는 셸에서 `show`로 본다.)

#### 셸 안에서 출력 형식 바꾸기

프롬프트에서 `set format`으로 **즉시** 바꾼다. 다음 질의부터 적용되고 셸이 살아 있는 동안 유지된다.

```
nsql> set format markdown
width 0 · colwidth 60 · overflow none · format markdown
nsql> SELECT * FROM emp ORDER BY sal DESC;
| id | name | dept | sal |
| ---: | --- | --- | ---: |
| 2 | Lee | IT | 6200 |
| 1 | Kim | SALES | 5000 |
| 3 | Park | IT | 4800 |

3 rows (0.000s)

nsql> set format json
nsql> SELECT * FROM emp WHERE dept='IT';
[
  {"id": 2, "name": "Lee", "dept": "IT", "sal": 6200},
  {"id": 3, "name": "Park", "dept": "IT", "sal": 4800}
]
```

#### 셸 명령 (세미콜론 불요)

| 명령 | 뜻 |
|---|---|
| `set width <n\|auto>` | 한 줄 최대 폭 · `0` = 무제한 · `auto` = 터미널 폭 (SQL\*Plus `LINESIZE`) |
| `set colwidth <n>` | 한 열 최대 폭 · `0` = 무제한(넘으면 `…`) (sqlcmd `-y`) |
| `set overflow none\|wrap\|truncate\|expanded` | §4-4 |
| `set format <fmt>` | `grid` `markdown` `csv` `tsv` `json` `jsonl` `sql:select\|insert\|update\|delete\|merge` |
| **`\x`** | 레코드 보기 토글 (psql 관습) |
| `get <키>` · `show` | 설정 하나 또는 전체 보기 |
| **`copy [형식]`** | **마지막 결과**를 클립보드로(형식 생략 = 현재 형식) |
| `set max_rows <n>` | 질의당 보여 줄 행수(`0` = 무제한 · grid/markdown만 · 기본 `cli.max_rows` = 200) |
| `\more [n]` · `more [n]` | 마지막 질의의 다음 n행(서버 커서 · 닫혔으면 OFFSET 재질의) |
| `\all` | 남은 행 전부(배치로 스트리밍) |
| `\count` | 마지막 질의의 `SELECT COUNT(*)` |
| `\pager on\|off` | 긴 결과를 `$PAGER`(less)로 |
| `CONNECT <프로필>` | 스크립트 엔진 안에서 접속 전환 |

```
nsql> \x
width 0 (0 = 무제한 · set width auto = 터미널 폭) · colwidth 60 · overflow expanded · format grid · max_rows 200 · pager off
nsql> SELECT * FROM emp WHERE id=1;
-[ RECORD 1 ]----
id   | 1
name | Kim
dept | SALES
sal  | 5000

nsql> get format
format = grid
```

**긴 결과 나눠 보기** — `set max_rows 2` 뒤:

```
nsql> SELECT * FROM emp ORDER BY id;
id  name  dept    sal
--  ----  -----  ----
 1  Kim   SALES  5000
 2  Lee   IT     6200

2 rows (0.000s)
-- 처음 2행 · 더 보기: \more [n] · \all · \count

nsql> \more
id  name  dept   sal
--  ----  ----  ----
 3  Park  IT    4800

1행 추가 조회(0.000s · 커서) · 지금까지 3행

nsql> \count
총 3행(0.000s)
```

설정 `cli.auto_more on`이면 묻지 않고 다음 세그먼트를 이어 출력한다(psql `FETCH_COUNT`식).

#### 세 가지 층위

| 범위 | 방법 |
|---|---|
| **이번 질의부터(셸 안)** | `set format markdown` |
| **이번 실행만** | `nsql run -f markdown …` · `--width` `--max-col-width` `--overflow`/`-x` |
| **영구(GUI와 공유)** | `nsql config set cli.format markdown` |

우선순위는 **셸 `set` > 명령줄 플래그 > `cli.*` 설정**이다.

#### 다른 CLI 별칭 · 스크립트 포함 · SPOOL

psql·sqlite3·sqlcmd 사용자가 습관대로 쳐도 SQL\*Plus식 명령으로 대응된다.

| 별칭 | 같은 것 |
|---|---|
| `\d` · `\dt` · `.tables` · `SHOW TABLES` | 현재 스키마의 테이블 목록(`SHOW VIEWS` · `SHOW <kind>`도 · `nsql cat`과 같은 컬럼) |
| `\d <이름>` · `.schema <이름>` | `DESC[RIBE] <이름>` |
| `\i <파일>` · `:r <파일>` · `.read <파일>` | `@<파일>`(스크립트 실행) · `@@` = 호출 스크립트 기준 상대 경로 |
| `\c <대상>` | `CONNECT <대상>`(프로필 이름 또는 접속 문자열) |
| `\?` · `help` | 명령·별칭 표(`help set` = 출력 설정) |
| `SPOOL <파일> [APPEND]` · `SPOOL OFF` | 찍히는 모든 출력을 파일에도(확장자 없으면 `.lst` · `SPOOL`만 = 상태) |

`SHOW ERRORS [종류 이름]` = 마지막으로 만든 저장 단위의 컴파일 오류(Oracle `ALL_ERRORS` · 없으면 `No errors.`). `@파일 인자…`는 현재 폴더 기준, `@@파일`은 **호출한 스크립트의 폴더** 기준(중첩도 각자 폴더 · 확장자 없으면 `.sql` 보완)이며 인자는 `&1..&n`으로 들어가고 돌아오면 호출자의 값이 복원된다. `SPOOL`은 `run`·`shell` 공통이다. ⚠️ SQL\*Plus처럼 `SPOOL out`은 `SPOOL OUT`(=OFF)으로 읽히니 파일 이름은 `out.lst`처럼 확장자를 붙인다. 배치에서 미정의 `&var`·선언 없는 `:bind`를 오류로 막으려면 `nsql config set script.strict on`(비노출 · `config list all`) — 보이는 설정 `vars.undeclared error`(기본 `prompt`)는 그런 스크립트를 실행하지 않는다.

```sh
mkdir -p sub && echo "SELECT '&1' AS from_child;" > sub/child.sql
printf 'SPOOL out.lst\nSELECT 1 AS n;\n@@sub/child.sql 42\nSPOOL OFF\n' > main.sql
nsql run -c "sqlite::memory:" main.sql          # 화면과 out.lst에 같은 표 · 끝에 "스풀 닫음: out.lst"
printf '\\d\n\\d emp\nquit\n' | nsql shell -c "sqlite:t1.db"
```

```
Name  Type     Nullable  Default
----  -------  --------  -------
id    INTEGER  Y
name  TEXT     Y
dept  TEXT     Y
sal   INTEGER  Y

4 rows (0.000s)
```

### Step 15 — 설정 (`config`) — GUI와 공유

```sh
nsql config list              # 보이는 키 전부 · 현재 값 · (기본값) · 허용 값 · 설명
nsql config list all          # 비노출 구현 키까지
nsql config list perf         # 부하원 키를 도메인별로 + full/balanced/low 프리셋
nsql config get ui.lang
nsql config set cli.width 120
nsql config reset cli.width
nsql config export-json       # settings.json으로 내보내고 그 경로를 출력
nsql config path
```

```
[사용자 인터페이스 ▸ 모양]
ui.lang            ko         [system | en | ko]  화면 언어(즉시 적용). 시스템(기본값) = OS 표시 언어를 따름(지원하는 언어일 때 · 그 밖은 영어)
ui.theme           system   (기본값)  [system | light | dark]  색 테마. System은 OS의 라이트/다크 모드를 따릅니다
…
[접속 ▸ CLI]
cli.width          0        (기본값)  [0..10000]  nsql 결과 표 한 줄의 최대 폭(0 = 터미널 폭 · 파이프면 무제한). sqlplus LINESIZE. 1회성은 --width N
cli.max_col_width  60       (기본값)  [0..10000]  컬럼 하나의 최대 폭(0 = 무제한 · 긴 값은 …). sqlcmd -y. 1회성은 --max-col-width N
cli.overflow       none     (기본값)  [wrap | truncate | expanded | none]  …
cli.format         grid     (기본값)  [grid | markdown | csv | tsv | json | jsonl]  …
cli.null_text               (기본값)  [text]  nsql 출력에서 null을 이 글자로(기본 빈 칸 · psql \pset null)
bulk.batch_rows    1000     (기본값)  [1..100000]  …
…
cli.max_rows       200      (기본값)  [0..10000000]  `nsql shell`에서 질의당 보여 주는 행수 …
cli.auto_more      off      (기본값)  [on | off]  …
```

```
$ nsql config set cli.width 120
cli.width = 120
$ nsql config reset cli.width
cli.width 기본값으로 복원 (0)
$ nsql config set cli.overflow sideways
cli.overflow의 값이 올바르지 않습니다: 'sideways' (허용: wrap | truncate | expanded | none)      # 종료 2
```

```
$ nsql config list perf
perf.mode = full · 실효 = full · 키 30개
키 · 값 · 출처 · full / balanced / low

[db · DB]
grid.max_rows              200        기본             200 / 200 / 100
…
[net · 네트워크]
probe.enabled              on         기본             on / on / off
…
```

GUI 설정 창과 **같은 `settings.conf`**를 보고, 바꾼 값은 GUI에 즉시 반영된다. 읽기 전용 정보 키(`*.info_*`)는 `config list`에 나오지 않는다. 새 설정 키는 `nsql-settings::REGISTRY`에만 추가한다(CLAUDE.md 규약).

### Step 16 — 연결 프로필 (`conn`) 직접 해 보기

§1-2를 SQLite로 따라 한다(격리 `NSQL_HOME`이면 실제 프로필에 영향 없음).

```sh
nsql conn add lite "sqlite:t1.db"
nsql conn add mem -d sqlite --db ":memory:"
nsql conn add pgdemo "postgres://app:@db.example:5432/appdb" --no-prompt   # 저장만(접속 안 함) · 빈 비밀번호
nsql conn list
nsql conn show lite
nsql conn test lite
nsql conn env lite dev
echo "SELECT count(*) AS n FROM emp;" | nsql run -c lite -
nsql conn rm pgdemo
```

```
저장: lite = sqlite://t1.db  (비밀번호 없음 - 접속 시 방언이 요구하면 실패)  (…\home)
저장: pgdemo = postgres://app/***@db.example:5432/appdb  (…\home)

NAME    DIALECT  TYPE  PW   TARGET
lite    sqlite   -     -    t1.db
mem     sqlite   -     -    :memory:
pgdemo  postgres  -     ✓    app@db.example:5432/appdb

lite: sqlite://t1.db
  유형: None (NON)
  파일: …\home\profiles\c5df50f999eb649a.conf

OK  sqlite://t1.db  (sqlite · 0.001s)  sqlite t1.db

lite: Development (DEV)

삭제: pgdemo
```

`conn env <이름> [<유형>]` — 유형 = `none|non` · `development|dev` · `test|tst` · `production|prd`(대소문자 무관) · 생략하면 현재 값만 본다. 운영 프로필의 시나리오 점검은 [115](115-prod-mode-scenario-nsql-guide.md).

### Step 17 — 드라이버 (`driver`) — D-350

확장 드라이버는 `<설정 폴더>/drivers/<id>/<버전>/`(또는 `NSQL_DRIVER_DIR`) 아래 `nsql_driver_<id>.dll/.so/.dylib` + 매니페스트 `nexa-driver.json`으로 산다([22](22-driver-extensions.md)). 같은 id의 여러 버전이 나란히 있을 수 있고(SxS) 최신을 쓴다.

```sh
nsql driver path
nsql driver search                    # 색인(저장소 drivers/index.json · 1시간 캐시) + 설치 버전
nsql driver list
```

```
id             latest     installed  dialect    this-os  name
sqlite         0.1.2      -          sqlite     yes      SQLite
pg             0.1.2      -          postgres   yes      PostgreSQL
mysql          0.1.2      -          mysql      yes      MySQL / MariaDB
mssql          0.1.2      -          mssql      yes      SQL Server
oracle-thin    0.1.2      -          oracle     yes      Oracle (Thin)
oracle-oci     0.1.2      -          oracle     yes      Oracle (OCI, Instant Client)
odbc           0.1.2      -          odbc       yes      ODBC
duckdb         0.1.2      -          duckdb     yes      DuckDB
index: …\home\drivers\index.json (fetched)

설치된 확장 드라이버 없음 - …\home\drivers
```

**접속 시 자동 설치** — 드라이버 없이 DuckDB에 붙으면 먼저 받아 설치하고 접속한다(진행은 stderr `[driver] …`):

```sh
echo "SELECT 42 AS answer, version() AS v;" | nsql run -c "duckdb::memory:" -
```

```
[driver] duckdb: 색인 받는 중…
[driver] duckdb: 내려받는 중 0%
…
[driver] duckdb: 내려받는 중 100%
[driver] duckdb: 검증 중…
[driver] duckdb: 설치 중…
[driver] duckdb: 설치됨
Connected: duckdb://:memory: (duckdb)
answer  v
------  ------
    42  v2.0.0

1 rows (0.001s)
```

`driver.auto_install off`면 받지 않고 실패한다(종료 1):

```
ERROR line 0: [-1350] duckdb driver is not installed - install it: nsql driver install duckdb (or View ▸ Driver Manager)
```

설치 뒤:

```
$ nsql driver list
id             version      dialect    ABI   manifest  path
duckdb         0.1.2        duckdb     2     yes       …\home\drivers\duckdb\0.1.2\nsql_driver_duckdb.dll

$ nsql driver check duckdb
정상: 확장을 불러왔습니다
id       = duckdb
version  = 0.1.2
dialect  = duckdb
abi      = 2
path     = …\home\drivers\duckdb\0.1.2\nsql_driver_duckdb.dll

$ nsql driver update --check
모두 최신(1개 설치)

$ nsql driver install duckdb
duckdb@0.1.2은(는) 이미 설치돼 있습니다 (duckdb@0.1.2 is already installed)      # 종료 1 · 바꿔 넣으려면 --force
```

```sh
echo "CREATE TABLE t AS SELECT range AS n, range*range AS sq FROM range(4); SELECT * FROM t;" | nsql run -c "duckdb:t.duckdb" -
```

```
Connected: duckdb://t.duckdb (duckdb)
4 rows affected (0.002s)
n  sq
-  --
0   0
1   1
2   4
3   9
```

| 하위 명령 · 옵션 | 뜻 |
|---|---|
| `list` · `path` | 설치된 확장(id별 최신) · 드라이버 폴더 경로 |
| `check <id>` | 확장을 실제로 올려 함수표 정보 출력 |
| `search [<글>] [--refresh]` | 색인 + 설치 버전 · `--refresh` = 색인 다시 받기(조건부 GET) |
| `install <id>[@<ver>] [--pre] [--force]` | 내려받아 검증(sha256 + Ed25519)하고 원자적으로 설치 · `@<ver>` = 버전 고정 · `--pre` = 프리릴리스 허용 · `--force` = 같은 버전 바꿔 넣기 |
| `install --file <zip> [--force]` | 오프라인 패키지(옆의 `.sha256`·`.sig`) 또는 **전체 번들** `nsql-drivers-all-<ver>-<os>-<arch>.zip`(폐쇄망) |
| `update [<id>] [--check]` | 더 새 버전을 옆에 설치(전부 또는 하나) · `--check` = 목록만 |
| `rm <id>[@<ver>]` | 확장 폴더 삭제(한 버전 또는 전부) |

```sh
nsql driver install oracle-oci@0.1.1 --force
nsql driver install --file nsql-driver-duckdb-0.1.1-windows-x86_64.zip
nsql driver install --file nsql-drivers-all-0.1.1-windows-x86_64.zip     # 오프라인 번들(전 드라이버)
nsql run -c "oracle://scott@db:1521/orcl?driver=oci" q.sql                # 이 접속에만 드라이버 지정(§1-1 (h))
```

- 자동 설치 실패 = **-1351**(설치 실패) · 드라이버 없음 = **-1350**(+ `nsql driver install <id>` 안내). 저장소 = `driver.repo`(기본 `SosomLab/nexa-sql`) · 요청 상한 = `driver.http_timeout_secs`(30) · 하루 1회 갱신 알림 = `driver.check_updates`(자동 교체 없음).
- 출력 문구는 `ui.lang`을 따른다(표 머리글·`key = value`는 영어 고정).

### Step 18 — 파일 검색 (`grep`)

접속 없이 폴더를 병렬로 걸으며 찾는다(GUI 파일 검색과 같은 엔진). `.git` `node_modules` `target` `.nexa-sql` · `.gitignore` 규칙 · 이진 파일은 건너뛴다. 출력 = `경로:줄:열: 본문` · **대소문자 구분이 기본**.

```sh
nsql grep emp proj                       # 경로 생략 = 현재 폴더
nsql grep -i -w EMP proj                 # 대소문자 무시 · 단어 단위
nsql grep -e "sp_\w+_create" proj        # 정규식
nsql grep -i select proj/scripts/report.sql
```

```
proj\scripts\load.sql:1:13: INSERT INTO emp SELECT * FROM emp_stage;
proj\scripts\load.sql:1:31: INSERT INTO emp SELECT * FROM emp_stage;
proj\scripts\load.sql:2:9: EXEC sp_emp_create 1;
proj\scripts\report.sql:3:8:   FROM emp
proj\scripts\report.sql:5:15: select * from emp_hist;

proj\scripts\load.sql:1:13: INSERT INTO emp SELECT * FROM emp_stage;
proj\scripts\report.sql:3:8:   FROM emp

proj\scripts\load.sql:2:6: EXEC sp_emp_create 1;

proj/scripts/report.sql:2:1: SELECT dept, SUM(sal) AS total
proj/scripts/report.sql:5:1: select * from emp_hist;
```

- 옵션: `-i`/`--ignore-case` · `-w`/`--word` · `-e`/`--regex` · `--no-ignore`(`.gitignore` 무시 · 기본 폴더 제외는 유지) · `-j`/`--threads <n>`(기본 CPU 수 · 최대 16) · `--max-file-kb <n>`(기본 1024 · 0 = 무제한) · 경로 여러 개 가능 · `-`로 시작하는 패턴은 `--` 뒤에.
- **종료 코드** 0 = 일치 있음 · 1 = 없음(`nsql grep nothing_here proj` → 출력 없이 1) · 2 = 인자/패턴 오류.

### Step 19 — 북마크 (`bookmark`)

GUI와 같은 워크스페이스 파일 `NSQL_HOME/workspaces/<프로젝트>.nsql-workspace`(프로젝트 없음 = `default`)를 읽고 쓴다. `add`는 줄의 본문·문맥을 닻으로 저장해 GUI가 편집 뒤에도 찾아 옮길 수 있게 한다.

```sh
nsql bookmark add proj/scripts/report.sql 2 --label "monthly totals"
nsql bookmark add proj/scripts/load.sql 1
nsql bookmark list
nsql bookmark list --md
nsql bookmark rm 2
nsql bookmark prune --days 7
```

```
added #1 at line 2 of …\work\proj\scripts\report.sql
added #2 at line 1 of …\work\proj\scripts\load.sql

   id  document                       line  text                                     state
    2  load.sql                          1  INSERT INTO emp SELECT * FROM emp_stage; live
    1  report.sql                        2  monthly totals                           live
2 bookmark(s) · 0 invalid · …\home\workspaces\default.nsql-workspace

| id | document | line | text | state |
|---:|---|---:|---|---|
| 2 | load.sql | 1 | INSERT INTO emp SELECT * FROM emp_stage; | live |
| 1 | report.sql | 2 | monthly totals | live |

removed #2
pruned 0 invalid bookmark(s) older than 7 day(s)
```

- `list --doc <경로>` = 한 문서만 · `--project <파일>` = `default` 대신 그 프로젝트(`*.nsql-project`)의 워크스페이스 · `prune [--days N]` = N일(기본 30) 넘은 무효 항목 정리.
- ⚠️ 0.2.8 확인 결과 `--doc`은 **절대 경로**일 때만 걸렸다(`--doc proj/scripts/report.sql`·`--doc report.sql` = 0행 · 바닥줄은 전체 개수). 북마크 출력은 아직 영어 고정이다.

### Step 20 — 라이선스 (`license`)

라이선스는 `<설정>/license/`에 두는 **서명된 텍스트 파일**이다(GUI와 공유 · [23](23-license-activation.md) · [25](25-license-tiers-and-server.md)). 없으면 게이트된 기능은 무료 수준에 머문다.

```sh
nsql license status
nsql license request "Hong Gildong" hong@example.com     # 이메일로 보낼 한 줄 코드
nsql license path
```

```
state          free
file           -
build_date     2026-10-23
app_version    0.2.8
machine        3EMF…NJP
install_to     …\home\license\nexa-sql.license

NEXAREQ1.3EMF…NJP.DXSK…ACGYK4C1J6RPK2C1D68SGMVHX91QPWSS08XMPRS3FDSKGMS9XD1QPWST0CNW62VBGDHJJWRVFDM50
→ kiros33@sosomlab.com
```

`request`의 기기 ID는 해시만 담긴다. `install <파일>`은 **먼저 검증하고 유효할 때만 복사**한다(원본 보존 · 거부 = 종료 1 · 쓰지 않음):

```
$ nsql license install fake.license
라이선스를 설치하지 않았습니다: invalid(format) (파일은 그대로 · 기기 코드 = 3EMF…NJP)
$ nsql license export lic-backup.txt
설치된 라이선스가 없습니다                       # 종료 1
$ nsql license remove
nothing to remove · state free
```

하위 명령 = `status` · `request [<이름> [<이메일>]]` · `install <파일>`(= `import <파일>`) · `remove` · `path` · `export <파일>`(설치본 백업). 실제 라이선스 설치·삭제는 격리 홈에서만 시험한다.

---

## 3. 실서버 점검 (Oracle · SQL Server · PostgreSQL · MySQL · ODBC)

서버가 준비되면 §2와 같은 순서를 실서버로 반복한다. (출력 생략 · 실서버 필요)

```sh
# 1) 붙는지부터(드라이버가 없으면 여기서 자동 설치)
nsql conn test -d oracle --host 172.16.1.58 --port 1521 --db APPDB --user APPDB
#    또는 저장해 두고
nsql conn add appdb "oracle://APPDB@172.16.1.58:1521/APPDB"
nsql conn test appdb

# 2) 카탈로그가 보이는지
nsql cat -c appdb schemas
nsql cat -c appdb tables

# 3) 통합 테스트 스크립트
nsql run -c appdb     examples/it-oracle.sql
nsql run -c PLANDB    examples/it-mssql.sql
nsql run -c repodb examples/it-pg.sql
nsql run -c mydb      examples/it-mysql.sql

# 4) 계측·로그를 켜고 한 번 더
nsql run -c appdb --timing --log examples/it-oracle.sql

# 5) 대량 적재·추출(드라이버 최속 경로: Oracle 배열 DML · PostgreSQL COPY)
nsql export -c repodb -t big_table -f csv --fast -o big.csv
nsql import -c repodb -t big_copy --commit-every 50000 --timing big.csv
```

### 프로필을 전혀 만들지 않고 같은 순서로

저장을 남기고 싶지 않을 때(남의 PC · 일회성 점검 · CI)는 §1-1 문자열을 그대로 반복하면 된다. 매번 치기 번거로우면 **셸 변수에 한 번만 담는다.**

```sh
# 비밀번호는 묻게 두고 접속만 변수로
DB="oracle://APPDB@172.16.1.58:1521/APPDB"

nsql conn test -d oracle --host 172.16.1.58 --port 1521 --db APPDB --user APPDB
nsql cat     -c "$DB" schemas
nsql cat     -c "$DB" tables
nsql run     -c "$DB" examples/it-oracle.sql
nsql run     -c "$DB" --timing --log examples/it-oracle.sql
nsql export  -c "$DB" -t EMP -f csv -o emp.csv
```

```powershell
# PowerShell
$DB = "mssql://sa:$env:DBPW@172.16.1.58:1433/PLANDB_MS"
nsql cat -c $DB tables
nsql run -c $DB --no-prompt examples\it-mssql.sql
```

비밀번호를 매번 묻는 게 번거롭고 저장도 싫다면 **셸 세션 동안만 환경변수**에 둔다 — 창을 닫으면 사라진다:

```sh
read -s NSQL_PASSWORD && export NSQL_PASSWORD
nsql run -c "oracle://APPDB@172.16.1.58:1521/APPDB" --no-prompt examples/it-oracle.sql
```

### Oracle 주의 — Thin과 OCI

- **Thin**(`?driver=thin` · 확장 `oracle-thin` · 기본 `oracle.driver`) = 순수 Rust · Instant Client 불필요.
- **OCI**(`?driver=oci` · 설치본 내장 · 확장 `oracle-oci`) = Instant Client가 **런타임에 필요**하다. 못 찾으면 `DPI-1047`이 난다.

```sh
# Windows (cmd)
set NSQL_ORACLE_CLIENT_DIR=C:\oracle\instantclient_21_13
# macOS/Linux
export NSQL_ORACLE_CLIENT_DIR=/opt/oracle/instantclient_21_13
```

### ODBC (Tibero · Altibase · CUBRID …)

```sh
nsql conn add tib "odbc:DSN=TIBERO_DEV;Uid=sys;Pwd=pw" && nsql conn test tib
nsql run -c "odbc:Driver={Tibero 7 ODBC Driver};Server=10.0.0.5;Port=8629;DB=tibero;Uid=sys" q.sql
nsql run -c "tibero://sys@10.0.0.5:8629/tibero" q.sql          # 벤더 스킴 = 드라이버 이름 프리셋
```

OS에 해당 ODBC 드라이버(또는 DSN)가 등록돼 있어야 한다. `Uid`/`Pwd`는 접속 문자열에서 빼 사용자·비밀번호로 다룬다(프로필이면 봉투 저장 · 화면은 가림).

### 서버 메시지

MSSQL `PRINT`/`RAISERROR`와 PostgreSQL `NOTICE`는 **실행 중 실시간**으로 나온다. Oracle `DBMS_OUTPUT`은 폴링 모니터 방식이다([32](32-server-messages-and-live-log.md)).

---

## 4. 출력 형식 상세

결과를 어떤 모양으로 낼지는 **형식(`-f`)**과 **표 모양(폭·넘침)** 두 축으로 정한다. `-f`는 `run` `shell` `export` `cat`이 받는다(셸 안에서는 `set format`).

### 4-1. 형식 한눈에

| 형식 | 별칭 | 무엇에 쓰나 |
|---|---|---|
| `grid` | `default` `table` `ansiconsole` | **기본(`cli.format`).** 사람이 읽는 텍스트 표 |
| `markdown` | `md` | 편집기·위키·PR 본문에 붙여 넣기 |
| `csv` | — | 엑셀·스프레드시트 |
| `tsv` | `delimited` | 탭 구분 · 붙여 넣기 |
| `json` | — | 배열 하나 — API·`jq` |
| `jsonl` | `jsonlines` `ndjson` | 줄당 객체 하나 — 스트리밍·로그 적재 |
| `sql:select` | — | 행마다 `SELECT … WHERE 키` |
| `sql:insert` | `sql` | 행마다 `INSERT`(테이블 이름 = 실행한 문장에서 추정) |
| `sql:update` | — | 행마다 `UPDATE … WHERE 키` |
| `sql:delete` | — | 행마다 `DELETE … WHERE 키` |
| `sql:merge` | — | 행마다 `MERGE`(upsert) |
| `insert:<테이블>` | `insert` | 테이블 이름을 정해 `INSERT` 생성(`run`에서 이름 없이 `insert`면 `T`) |

별칭은 0.2.8에서 모두 확인했다(`-f md` · `table` · `default` · `ansiconsole` · `ndjson` · `jsonlines` · `delimited` · `sql`).

### 4-2. 실제 출력 (같은 결과, 형식만 바꿈)

기준 결과(`q.sql` = `SELECT * FROM emp ORDER BY sal DESC`):

```
id  name  dept    sal
--  ----  -----  ----
 2  Lee   IT     6200
 1  Kim   SALES  5000
 3  Park  IT     4800
```

**`-f markdown`** — 숫자 열은 오른쪽 정렬 표시(`---:`)가 붙는다.

```
| id | name | dept | sal |
| ---: | --- | --- | ---: |
| 2 | Lee | IT | 6200 |
| 1 | Kim | SALES | 5000 |
| 3 | Park | IT | 4800 |
```

**`-f csv`** / **`-f tsv`**(구분자만 탭)

```
id,name,dept,sal
2,Lee,IT,6200
1,Kim,SALES,5000
3,Park,IT,4800
```

**`-f json`** — 배열 하나. `jq`로 바로 먹인다.

```json
[
  {"id": 2, "name": "Lee", "dept": "IT", "sal": 6200},
  {"id": 1, "name": "Kim", "dept": "SALES", "sal": 5000},
  {"id": 3, "name": "Park", "dept": "IT", "sal": 4800}
]
```

**`-f jsonl`** — 줄당 객체 하나. 큰 결과를 스트리밍할 때.

```
{"id": 2, "name": "Lee", "dept": "IT", "sal": 6200}
{"id": 1, "name": "Kim", "dept": "SALES", "sal": 5000}
{"id": 3, "name": "Park", "dept": "IT", "sal": 4800}
```

NULL은 grid에서 빈 칸이다 — 글자로 보이려면 `nsql config set cli.null_text "(null)"`(psql `\pset null`):

```
n       s
------  -
(null)  x
```

**`-f xlsx`** / **`-f parquet`**(11-01 213차 · D-432) — 표 계산·분석 도구로 넘길 때. 이진 파일이라 `-o 파일`(또는 `> 파일`)로 받는다.

```
nsql export -c prod -q "select * from emp" -f xlsx    -o emp.xlsx
nsql export -c prod -t big_table               -f parquet -o big.parquet
```

- xlsx = 시트 하나 · 행 상한 1,048,576(넘으면 오류 — 큰 결과는 csv/parquet) · 셀 32,767자 · |정수| > 2^53 · 16자리 이상 Decimal은 글자로(정밀도 보존) · 별칭 `excel`.
- parquet = 배치 하나 = 행 그룹 하나(메모리 = 한 배치) · SNAPPY 압축 · 열 형 = 첫 배치의 첫 비NULL 값(INT64 · DOUBLE · BOOLEAN · UTF8) · 같은 열 이름은 `_2`를 붙임 · 결과 0행 = 오류(빈 파일 안 만듦).
- 접속 안내 줄은 `export`에서 stderr로 나간다 — stdout 리다이렉트(`> f.parquet` · CSV 파이프)가 순수하다.

### 4-3. SQL 형식 5종 — 결과를 실행문으로

결과 행을 **다시 실행할 수 있는 SQL**로 만든다. 데이터 이관·재현·패치 스크립트에 쓴다. 테이블 이름은 실행한 문장에서 추정한다. 아래는 `emp`에 **PK(`id`)가 있을 때**(Step 3)의 실제 출력이다.

**`-f sql:insert`** (또는 그냥 `sql`)

```sql
INSERT INTO emp (id, name, dept, sal) VALUES (2, 'Lee', 'IT', 6200);
INSERT INTO emp (id, name, dept, sal) VALUES (1, 'Kim', 'SALES', 5000);
```

**`-f sql:update`** — 키 아닌 열을 SET에, 키를 WHERE에

```sql
UPDATE emp SET name = 'Lee', dept = 'IT', sal = 6200 WHERE id = 2;
```

**`-f sql:delete`**

```sql
DELETE FROM emp WHERE id = 2;
```

**`-f sql:select`**

```sql
SELECT id, name, dept, sal FROM emp WHERE id = 2;
```

**`-f sql:merge`** — upsert 한 문장(행마다 한 줄)

```sql
MERGE INTO emp t USING (SELECT 2 AS id, 'Lee' AS name, 'IT' AS dept, 6200 AS sal) s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET t.name = s.name, t.dept = s.dept, t.sal = s.sal WHEN NOT MATCHED THEN INSERT (id, name, dept, sal) VALUES (s.id, s.name, s.dept, s.sal);
```

**`-f insert:<테이블>`** — 다른 테이블로 옮길 때

```sh
nsql export -c "sqlite:t1.db" -t emp -f insert:EMP_BACKUP -o seed.sql
```

```sql
INSERT INTO EMP_BACKUP (id, name, dept, sal) VALUES (1, 'Kim', 'SALES', 5000);
```

#### ⚠️ WHERE 절의 키는 이렇게 고른다

`update` · `delete` · `merge` · `select`는 행을 **식별할 키**가 필요하다. 순서는 [41 결과 → SQL 키 규칙](41-sql-copy-key-rules.md)을 따른다:

1. **기본 키(PK)**
2. 없으면 **첫 유니크 인덱스**
3. 그것도 없으면 **앞 3개 컬럼 + 경고**

3번으로 떨어지면 출력 끝에 주석으로 한 번 경고한다(PK 없는 표 · 0.2.8 실제 출력):

```sql
UPDATE emp SET sal = 6200 WHERE id = 2 AND name = 'Lee' AND dept = 'IT';
-- 경고: emp에 기본 키·유니크 인덱스가 없어 앞 3개 컬럼(id, name, dept)을 키로 썼습니다. 실행 전 WHERE/ON을 확인하세요(설정 sql.key_mode)
```

> **이 경고가 보이면 실행 전에 `WHERE`/`ON`을 직접 확인한다.** 앞 3개 컬럼이 행을 유일하게 고르지 못하면 **의도보다 많은 행이 바뀐다.**

키 선택은 설정으로 바꾼다. 실제 키 이름은 **`grid.key_mode`**(`[데이터 편집기 ▸ 결과 셋]` · GUI 결과 복사와 공유)이고, 도움말·경고 문구에 나오는 옛 이름 `sql.key_mode`도 그대로 받는다:

```sh
nsql config set grid.key_mode pk     # 기본 — PK → 유니크 → 앞 3컬럼(경고)
nsql config set grid.key_mode all    # 모든 컬럼을 WHERE에 넣는다(안전하지만 길다)
```

### 4-4. 표 모양 — `grid`일 때만

| 축 | 플래그 | 설정 | 셸 |
|---|---|---|---|
| 한 줄 최대 폭 | `--width <n>` · `--linesize <n>` | `cli.width` (0) | `set width <n\|auto>` |
| 한 열 최대 폭 | `--max-col-width <n>` | `cli.max_col_width` (60) | `set colwidth <n>` |
| 넘칠 때 처리 | `--overflow <mode>` | `cli.overflow` (`none`) | `set overflow <mode>` |
| 레코드 보기 | `-x` · `--expanded` | — | `\x` |
| NULL 글자 | — | `cli.null_text` (빈 칸) | — |

**폭**: `0` = 터미널 폭(파이프로 넘길 때는 무제한) · 셸의 `auto` = 터미널 폭. SQL\*Plus `LINESIZE`에 해당한다.
**열 폭**: `0` = 무제한, 넘치면 끝에 `…`가 붙는다. sqlcmd `-y`에 해당한다.

**넘침 처리 4가지**

| 모드 | 동작 |
|---|---|
| `none` (기본) | 행당 한 줄로 길게 — 편집기에 붙이면 정렬이 유지된다 |
| `wrap` | 열을 묶음으로 나눠 아래로 · 행 번호 `#` 열이 붙는다 |
| `truncate` | 열을 줄여서 폭에 맞춘다 |
| `expanded` | 한 줄에 한 열 (psql `\x`) |

실제 출력(`SELECT 'aaa…'(20자) AS col_one, 'bbb…' AS col_two, 'ccc…' AS col_three`):

```
$ nsql run -c sqlite::memory: --width 40 --overflow wrap wide.sql
#  col_one
-  --------------------
1  aaaaaaaaaaaaaaaaaaaa

#  col_two
-  --------------------
1  bbbbbbbbbbbbbbbbbbbb

#  col_three
-  --------------------
1  cccccccccccccccccccc

$ nsql run -c sqlite::memory: --width 40 --overflow truncate wide.sql
col_one       col_two       col_three
------------  ------------  ------------
aaaaaaaaaaa…  bbbbbbbbbbb…  ccccccccccc…

$ nsql run -c sqlite::memory: --max-col-width 8 wide.sql
col_one   col_two   col_thr…
--------  --------  --------
aaaaaaa…  bbbbbbb…  ccccccc…
```

**`-x` / `\x` 레코드 보기** — 열이 많을 때 가장 읽기 쉽다. 폭과 무관하게 항상 레코드로 낸다(Step 4).

### 4-5. 어디서 정하나 — 세 층위

| 범위 | 방법 | 예 |
|---|---|---|
| **셸 안에서 즉시** | `set …` | `set format markdown` |
| **이번 실행만** | 플래그 | `-f markdown --width 120 -x` |
| **영구(GUI와 공유)** | 설정 | `nsql config set cli.format markdown` |

우선순위는 **셸 `set` > 플래그 > `cli.*` 설정**이다. `cli.format`이 받는 값은 `grid | markdown | csv | tsv | json | jsonl`이다(`sql:*`은 플래그·셸에서만).

```sh
nsql config set cli.format        markdown
nsql config set cli.width         160
nsql config set cli.max_col_width 40
nsql config set cli.overflow      wrap
nsql config set grid.key_mode     pk
nsql config list | grep "^cli\."
```

### 4-6. stdout / stderr 분리 — 파이프에 중요

**결과 데이터는 stdout, 나머지(`Connected:` 알림·진행·경고·`--timing`·`--log`·`[driver]` 진행)는 stderr**로 나간다. 그래서 리다이렉트하면 **데이터만** 남는다.

```sh
nsql run -c prod -f csv query.sql > out.csv          # out.csv에는 CSV만
nsql run -c prod -f json query.sql | jq '.[].name'   # 바로 jq로
nsql run -c prod -f markdown q.sql > result.md       # PR에 붙일 표
```

**종료 코드**(`run`): 0 = 전 항목 성공 · 1 = 하나 이상 실패(실패마다 stderr에 줄 번호) · 2 = 사용법·접속 오류. `grep`은 0 = 일치 · 1 = 없음 · 2 = 오류. CI에서 그대로 쓴다.

```sh
nsql run -c prod --no-prompt migrate.sql || echo "실패 (종료 코드 $?)"
```

### 4-7. 클립보드로 — 셸 `copy`

직전 결과를 클립보드에 넣는다(형식 생략 = 현재 형식). Windows `CF_UNICODETEXT` · macOS `pbcopy` · Linux `xclip`. (자동 점검에서는 사용자 클립보드를 덮어쓰지 않도록 실행하지 않았다.)

```
nsql> SELECT * FROM emp;
nsql> copy markdown
3 rows copied as markdown
```

---

## 5. 명령 요약

```text
nsql run      -c <target> [접속 옵션] [-f fmt] [-v name=value]… [--max-rows N] [--width N]
              [--max-col-width N] [--overflow mode] [-x] [--timing] [--log] <script|-> [args…]
nsql shell    -c <target> [접속 옵션] [run과 같은 출력 옵션]
nsql export   -c <target> (-q <sql> | -t <table>) [-f fmt] [-o file] [--max-rows N] [--fast]
nsql import   -c <target> -t <table> [-f csv|tsv|jsonl] [--no-header] [--cols a,b] [--map src=dst,…]
              [--batch N] [--commit-every N] [--mode auto|driver|multirow|single] [--empty-null on|off]
              [--timing] <file|->
nsql format   [<file|->] [-o <file>] [--in-place]
nsql explain  -c <target> (-q <sql> | <file>)
nsql plan     [-d dialect] <script|-> [args…]
nsql conn     list | add <name> [<target>] [접속 옵션] | show <name> | env <name> [<type>]
              | rm <name> | test [<name>] [접속 옵션] | path
nsql driver   list | path | check <id> | rm <id>[@<ver>] | search [<text>] [--refresh]
              | install <id>[@<ver>] [--pre] [--force] | install --file <zip> [--force]
              | update [<id>] [--check]
nsql license  status | request [<name> [<email>]] | install <file> | remove | path
              | export <file> | import <file>
nsql cat      -c <target> [-s schema] [-f fmt] schemas | kinds | <kind> | columns <object>
              | sub <object> <sub-kind> [kind] | detail <object> [kind]
              | gen <what> <object> [kind] [sub-kind sub-name] | source <kind> <name> | errors <name>
nsql config   list [all|perf] | get <key> | set <key> <value> | reset <key> | export-json | path
nsql grep     <pattern> [<path>…] [-i] [-w] [-e] [--no-ignore] [-j <n>] [--max-file-kb <n>]
nsql bookmark list [--project <file>] [--doc <path>] [--md] | add <file> <line> [--label <text>]
              | rm <id> | prune [--days N]
```

접속 옵션 = `-d` `--host` `--port` `--db`/`--database` `-u`/`--user` `-p`/`--password` `--password-stdin` `--no-prompt`.

### 공통 인자

| 인자 | 뜻 |
|---|---|
| `-c, --connect <target>` | 프로필 이름 **또는** 접속 문자열 (§1) |
| `-d, --dialect <name>` | 대상이 방언을 말하지 않을 때 — `oracle`(기본) `mssql` `postgres` `sqlite` … · `plan`의 기본 방언 |
| `--host` `--port` `--db`/`--database` `-u` `-p` | 접속 문자열 대신 **필드로** (`run` `shell` `export` `import` `explain` `conn`) |
| `--password-stdin` | 비밀번호를 표준 입력 첫 줄에서(스크립트 `-`와 함께 못 씀) |
| `--no-prompt` | 배치 모드 — 비밀번호를 묻지 않고, 치환 변수는 빈 값 |
| `-v, --var <name=value>` | 실행 전 치환 변수 정의(여러 번 · `run` `shell`) |
| `-f, --format <fmt>` | 출력 형식 (§4) · `import`에서는 원료 형식 |
| `-o, --out <file>` | 출력 파일(`export` `format` · 없으면 표준 출력) |
| `-q, --query <sql>` | 인라인 SQL 한 문장(`export` `explain`) |
| `-t, --table <table>` | 테이블(`export` = 통째로 내보내기 · `import` = 넣을 표) |
| `-s, --schema <schema>` | 카탈로그 스키마(`cat` · 기본 현재 스키마) |
| `--max-rows <n>` | 결과셋당 가져올 행 상한(`0` = 무제한 · GUI는 `grid.max_rows` = 200) |
| `--width, --linesize <n>` | 표 한 줄 최대 폭(`0` = 터미널 폭 · 파이프면 무제한 · `cli.width`) |
| `--max-col-width <n>` | 한 열 최대 폭(`0` = 무제한 · 기본 60) |
| `--overflow <mode>` | `none`(기본) `wrap` `truncate` `expanded` |
| `-x, --expanded` | 레코드 보기(psql `\x`) |
| `--timing` | 단계별 소요 시간 → stderr ([26](26-performance-architecture.md)) |
| `--log` | 실행 로그 → stderr (형식 = `log.format`) |
| `-h, --help` · `-V, --version` | 도움말 · 판올림 |
| `<script\|->` | 스크립트 파일 또는 `-`(표준 입력) |
| `[args]` | 스크립트의 `&1 &2 …` 치환 인자 |

### 비밀번호를 찾는 순서

1. `-p, --password <pw>` 또는 `--password-stdin`
2. **환경변수 `NSQL_PASSWORD`**
3. 터미널이면 **숨김 입력**으로 묻는다(그 접속에만 쓰고 저장하지 않음)
4. `--no-prompt`면 묻지 않는다 · 빈 비밀번호는 `user:@host`로 명시

```sh
export NSQL_PASSWORD='s3cr3t'
nsql run -c "postgres://app@dbhost:5432/appdb" --no-prompt migrate.sql
```

### 도움말

```sh
nsql --help              # 개요 · 명령 목록 · 접속 대상 · 공통 옵션 · 지원 방언
nsql <명령> --help       # 한 명령의 인자·옵션·예
nsql help <명령>         # 같은 내용
```

모르는 명령이면 `알 수 없는 명령: <이름>` + 개요를 찍고 종료 2.

### 스크립트에서 쓰는 것

- `&1`~`&n` — 명령줄 인자 치환 · `&이름` — `DEFINE`/`-v` 변수 · 층 이동 = `DEFINE 이름 SHARE|LOCAL|GLOBAL`(10-26 · 공유 세션·앱 전체로 올리기 · [63 §7](63-variable-management.md))
- `WHENEVER SQLERROR EXIT` — 오류 시 종료(종료 코드로 CI 판정)
- `CONNECT <프로필이름|접속 문자열>` — 스크립트 안에서 접속 전환
- `EXEC :V := 'x'` · `SELECT … INTO :V` · `PRINT` — 세션 변수([08](08-session-variables.md))
- `@파일` · `@@파일` · `SPOOL` — Step 14

---

## 6. 문제 해결

| 증상 | 원인·조치 |
|---|---|
| `[-1350] <id> driver is not installed` | 드라이버 확장 없음 + 자동 설치 꺼짐 → `nsql driver install <id>` 또는 `nsql config set driver.auto_install on` |
| `-1351`(자동 설치 실패) | 네트워크·GitHub 도달 확인 · 폐쇄망이면 오프라인 번들 `nsql driver install --file nsql-drivers-all-<ver>-<os>-<arch>.zip` |
| `DPI-1047` | Oracle OCI인데 Instant Client 없음 → `NSQL_ORACLE_CLIENT_DIR` 지정 또는 `?driver=thin` |
| `접속 실패: error connecting to server` | 호스트·포트·방화벽 확인. `nsql conn test`로 접속만 따로 시험 |
| `비밀번호 봉투를 열 수 없습니다` | 다른 Windows 계정·다른 PC의 프로필이다 → `nsql conn add <이름> …`로 다시 저장 |
| `프로필 '<이름>'이(가) 없습니다 …` | 이름은 `[A-Za-z0-9_.-]{1,64}`이고 `:`·`@`·`/`가 없다. 이름 꼴인데 프로필이 없으면 **오류로 알려 준다**(오타를 접속 문자열로 착각하지 않게 · 종료 2) |
| 결과가 잘림 | `--max-rows N`을 올린다 · 셸은 `set max_rows 0` 또는 `\more`/`\all` |
| `import`: `<표>에 <열> 열이 없습니다(--map 또는 --cols)` | 헤더 이름이 표 열과 다르다 → `--map src=dst` · 헤더가 없으면 `--no-header`(+ `--cols`) |
| PowerShell에서 `--map`·`--cols` 값이 이상하게 잘림 | 쉼표 목록이 배열로 쪼개졌다 → `"id=emp_no,name=ename"`처럼 따옴표 |
| ⚠ lite 설치본에서 `explain`·`export`·`import`·`cat`만 `[ORA--1350] oracle-thin driver is not installed`(확장이 **설치돼 있어도**) | **v0.2.8까지의 결함 · 10-23 b6b7377에서 수정**(다음 설치본부터): 이 네 명령이 드라이버 설정(확장 폴더 · 자동 설치 · `oracle.driver`)을 적용하지 않았다(`run`·`shell`·`conn test`는 정상) → v0.2.8에서는 `run`으로 실행하거나 `NSQL_DRIVER_DIR=<설정 폴더>/drivers`를 지정 · 수정 뒤 표기 = `[-1350]`(내부 코드는 `ORA-` 접두 없음) + 언어별 안내 |
| `SQLITE 14 … unable to open database file: x.db?driver=…` | SQLite 대상에 `?driver=`를 붙였다 → 뗀다(§1-1 (h)) |
| 한국어로 보고 싶다 | `nsql config set ui.lang ko` |
| SQLite 경로를 못 연다 | Windows 경로를 그대로 쓴다(`sqlite:C:\data\app.db`). Git Bash의 `/c/...` 꼴은 Windows 바이너리가 못 읽는다 |

---

## 7. 관련 문서

| 무엇 | 문서 |
|---|---|
| CLI 설계 제안·단계 | [11 CLI 도구](11-cli.md) |
| 인자 규약·타 CLI 조사 | [27 CLI 인자 규약](27-cli-conventions.md) |
| 연결 프로필 내부(암호화·공유 · `?driver=` §8) | [21 연결 프로필](21-connection-profiles.md) |
| 드라이버 확장(자동 설치 · 오프라인 번들 · `nsql driver`) | [22 드라이버 확장](22-driver-extensions.md) |
| 라이선스 | [23 라이선스 활성화](23-license-activation.md) · [25 티어·서버](25-license-tiers-and-server.md) |
| 세션 변수 엔진 | [08 세션 변수](08-session-variables.md) |
| **결과 → SQL 키 규칙**(`-f sql:*`) | **[41 SQL 복사 키 규칙](41-sql-copy-key-rules.md)** |
| 대량 적재·추출(`import` · `export --fast`) | [89 대량 I/O](89-bulk-io-review.md) |
| 운영 모드 시나리오(`conn env`) | [115 운영 모드 시나리오 nsql 안내](115-prod-mode-scenario-nsql-guide.md) |
| 설정 체계 | [24 설정·VS Code 분석](24-settings-and-vscode-analysis.md) |
| 성능 계측(`--timing`) | [26 성능 아키텍처](26-performance-architecture.md) |
| 서버 메시지·로그(`--log`) | [32 서버 메시지·실행 중 로그](32-server-messages-and-live-log.md) |
| 실서버 테스트 환경 | [20 Codespaces·Docker](20-testing-codespaces.md) |

> ★ **10-26 추가** — `nsql conn var <프로필> [NAME=value …]`(프로필 변수 보기/넣기 · `NAME=` = 삭제 · T-153 ②) · `nsql plan` EXECUTE 줄에 전달 모드(`bind N` · `declare-prepend` · `no params` · T-416) · 계획 출력 머리에 바인드 기준 한 줄(T-418).
