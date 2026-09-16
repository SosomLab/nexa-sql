# 27 · CLI 인자 규약 — 다양한 DB CLI 조사 · 공통화 · `nsql` 명령 구조

> **요청**(사용자 09-14): *"다양한 데이터베이스의 CLI 프로그램 arguments를 조사해 공통화하고 최대한 익숙한 명령 구조로"* + 사용자 제공 CLI 목록(RDBMS 15 · 국산 4 · NoSQL 11 · 분석/DW 13 · NewSQL 4 — §5) *"큰 골격은 크게 다르지 않을 것"*.
> **선행**: [11 CLI](11-cli.md)(서브커맨드 구조) · [23/25](25-license-tiers-and-server.md)(`nsql license`) · [24](24-settings-and-vscode-analysis.md)(`nsql config`).
> **상태**: 📐 조사·통일안. 긴 옵션(`--host` 등)은 ✅ 09-14 구현 · **짧은 옵션 충돌은 D-41**.

---

## 1. 대표 CLI 인자 조사(2026-09)

| CLI | 접속 표기 | 호스트 | 포트 | 사용자 | 비밀번호 | DB | 단일 명령 | 파일 실행 | 출력 | 조용히/배치 | 출처 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **psql**(PG · Redshift · Greenplum · Cockroach 호환) | `postgresql://user:pw@host:port/db` URI 또는 옵션 | `-h` | `-p` | `-U` | `-W` 프롬프트 · `PGPASSWORD` · `~/.pgpass` | `-d` | `-c "SQL"` | `-f file` | `-o file` · `-A -t -F` | `-q` · `-X` | [psql](https://www.postgresql.org/docs/current/app-psql.html) |
| **mysql / mariadb**(TiDB 동일) | 옵션만 | `-h` | **`-P`** | `-u` | **`-p[pw]`** · `MYSQL_PWD` · `~/.my.cnf` | `-D` 또는 위치 인자 | `-e "SQL"` | `< file` · `source` | `--batch -N` `--table` | `-s` · `-B` | [mysql](https://dev.mysql.com/doc/refman/8.0/en/mysql-command-options.html) |
| **sqlcmd**(SQL Server) | 옵션만 · `-S host,port` | `-S` | `-S host,port` | `-U` | **`-P`** · `SQLCMDPASSWORD` · `-E` 통합 인증 | `-d` | `-Q "SQL"`(종료) / `-q`(대기) | `-i file` | `-o file` · `-s sep` `-W` | `-b`(오류 시 종료) | [sqlcmd](https://learn.microsoft.com/sql/tools/sqlcmd/sqlcmd-utility) |
| **sqlplus / SQLcl** | **`user/pw@//host:port/service`**(Easy Connect) · TNS 별칭 · `/nolog` · `as sysdba` | (접속 문자열 안) | (안) | (안) | (안 · 생략 시 프롬프트) | 서비스명(안) | `-c`(SQLcl) · 표준입력 | `@script.sql` 위치 인자 · `@@` | `SPOOL` · `SET` | `-S` silent · `-L` 한 번만 로그인 | [SQL*Plus](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqpug/starting-SQL-Plus.html) |
| **sqlite3 / duckdb** | 파일 경로 위치 인자 | — | — | — | — | 파일 | `sqlite3 f.db "SQL"` · `-cmd` | `< file` · `.read` | `-csv -json -header` | `-batch` · `-readonly` | [sqlite3](https://sqlite.org/cli.html) |
| **usql**(범용 · 20+ DB) | **DSN URL** `driver://user:pw@host:port/db?opt` · 파일 경로 | (URL) | (URL) | (URL) | (URL · `-W` 프롬프트 · `-w` 금지) | (URL) | `-c` | `-f` | `-o` · `-J` json · `-C` csv | `-q` · `-X` | [usql](https://github.com/xo/usql) |
| db2 · hdbsql · isql(ASE/Altibase/Firebird) · tbsql · csql · gsql · cqlsh · clickhouse-client · snowsql · bteq | 대체로 옵션형(`-h/-p/-u`류) 또는 벤더 접속 문자열 · 일부는 셸 진입 후 `CONNECT` | 제각각 | 제각각 | `-u`/`-U` | `-p`/`-P`/`-w` | `-d`/`-n`/`-k` | `-e`/`-q`/`-s` | `-f`/`-i`/`@` | 제각각 | 제각각 | 벤더 문서 |

### 1-1. 큰 골격(사용자 지적대로 거의 같다)

1. **접속 표기는 두 계열**: ⓐ **URL/DSN 한 줄**(psql URI · usql · sqlplus Easy Connect · 파일 경로) ⓑ **필드 옵션**(`-h -p -U -d`). 익숙함은 둘 다 — 개발자는 URL, DBA는 필드.
2. **필드 옵션의 짧은 글자는 두 진영이 충돌**: psql `-p`=포트·`-W`=비번 프롬프트 vs mysql `-P`=포트·`-p`=비번 vs sqlcmd `-P`=비번·`-S`=서버. `-d`는 psql/sqlcmd = DB, mysql `-D`. **`-h`(호스트)·`-U`/`-u`(사용자)만 사실상 합의**.
3. **실행 방식 셋**: 단일 명령(`-c`/`-e`/`-Q`) · 파일(`-f`/`-i`/`@file`) · 표준입력.
4. **출력**: 파일(`-o`) · 형식(csv/json/표 · 구분자) · 헤더/피드백 끄기 · 조용히(`-q`/`-s`/`-S`).
5. **비밀번호는 인자보다 환경변수/프롬프트/자격 파일**이 관행(`PGPASSWORD`·`MYSQL_PWD`·`SQLCMDPASSWORD`·`.pgpass`·`.my.cnf`) — 우리는 **프로필(vault)**이 그 자리.
6. **DB 안 셸 명령**: psql `\d` · sqlcmd `:r` · sqlplus `DESCRIBE`/`@` · sqlite `.tables` — 우리 `nsql shell`은 SQL*Plus식(`@`·`SET`·`DESC`)을 기본(DR-6 · docs/11).

---

## 2. `nsql` 통일안

원칙: **URL/프로필이 1급**(usql·sqlplus 계열 · 이미 구현) · **긴 옵션은 모든 CLI의 이름을 그대로 받는다**(충돌 없음) · **짧은 옵션은 psql 진영을 기본**으로 하되 mysql/sqlcmd 별칭을 **충돌 없는 것만** 추가(D-41).

```
접속(공통 · 모든 서브커맨드)
  -c, --connect <target>        프로필 이름 · URL(oracle://u:p@h:1521/svc · mssql:// · postgres:// · sqlite:file.db) · sqlplus식 u/p@h:port/svc
  -h, --host <host>             (psql·mysql 동일)          -S <host[,port]>  sqlcmd 별칭(쉼표 포트 해석)
      --port <n>                ★ 짧은 글자 = D-41(psql -p vs mysql -P)
  -U, --user <user>             (psql·sqlcmd)  -u 별칭(mysql)
      --password[=<pw>]         값 생략 = 프롬프트(mysql식) · -W 프롬프트(psql식) · NSQL_PASSWORD 환경변수 · 프로필 vault 우선
  -d, --db <name>               ★ 지금은 -d = 방언 → D-41(psql/sqlcmd -d = DB · mysql -D)
  -T, --type <dialect>          방언(oracle|mssql|postgres|mysql|sqlite|odbc) — URL 스킴·프로필이 있으면 불요
  -E                            통합 인증(sqlcmd · MSSQL Windows/Entra · T-5 후속)
실행
  nsql run   [접속] <script|-> [args]      파일/표준입력(sqlplus @script · psql -f)
  nsql run   [접속] -e "SQL"               단일 명령(mysql -e · sqlcmd -Q · psql -c 대응 — psql -c는 우리 -c(접속)와 충돌해 -e 채택)
  nsql shell [접속]                          대화형(SQL*Plus 관례 · @ · SET · DESC)
  nsql export [접속] (-e <sql> | -t <table>) [-f csv|tsv|json|jsonl|insert] [-o file]
출력·동작
  -f, --format <fmt>            grid|csv|tsv|json|jsonl   (psql -A/-t 조합 · sqlite -csv/-json 대응)
  -o, --out <file>              (psql·sqlcmd 동일)
  -q, --quiet                   피드백·배너 끔(psql -q · sqlplus -S)
  -b, --bail                    첫 오류에서 종료(sqlcmd -b · psql ON_ERROR_STOP)
      --no-prompt               치환 변수 프롬프트 끔(배치)
      --timing                  단계별 소요(docs/26)
  -v, --set NAME=VALUE          치환 변수 사전 정의(psql -v · sqlcmd -v · sqlplus DEFINE)
관리
  nsql conn    list | add <name> [<target>] [필드 옵션] | show | rm | test [<name>] | path
  nsql config  list | get | set | reset | path
  nsql license request | install | status | remove | server <url> (23/25)
```

### 2-1. 지금과의 차이(구현 ✅ / 대기)

| 항목 | 현재 | 통일안 | 상태 |
|---|---|---|---|
| `--host --port --db --user` 긴 옵션 · `-u` | ✅ 09-14(`conn add/test`) | 전 서브커맨드로 확장 | ☐ T-51 |
| `-d` | 방언 | **DB 이름**(psql·sqlcmd 관례) · 방언은 `-T/--type` | **D-41** |
| `-p` | 비밀번호(`conn add`) | psql 관례면 **포트** · 비밀번호는 `--password`/`-W`/env | **D-41** |
| `-q` | `export`의 SQL | `--quiet` · 단일 SQL은 `-e` | **D-41**(export `-q`→`-e` 이동) |
| `-U` | 없음 | 사용자(psql·sqlcmd) | ☐ T-51 |
| `-S host,port` | 없음 | sqlcmd 별칭 | ☐ T-51 |
| `-v NAME=VALUE` | 없음(위치 인자 `&1..`만) | 치환 변수 사전 정의 | ☐ T-51 |
| `-b` · `NSQL_PASSWORD` · `-W` | 없음 | 추가 | ☐ T-51 |

### 2-2. 익숙함 검증(각 진영 사용자가 그대로 치면)

| 사용자 | 치는 것 | 결과 |
|---|---|---|
| psql | `nsql run -h db -p 5432 -U app -d sales -f q.sql` | D-41 ⓐ면 그대로 동작(`-f`는 우리 형식 옵션과 충돌 → **파일은 위치 인자**, `-f`는 형식 유지 · 안내 메시지) |
| mysql | `nsql run -h db -P 3306 -u app -D sales -e "SELECT 1"` | `-P`·`-D`는 별칭으로 수용(충돌 없음 · D-41 ⓐ에서 `-P` = 포트 별칭) |
| sqlcmd | `nsql run -S db,1433 -U app -P pw -d sales -Q "SELECT 1"` | `-S` ✅ · `-P` = **mysql 포트와 충돌** → sqlcmd `-P`(비번)는 받지 않고 안내(`--password`) · `-Q`→`-e` 별칭 |
| sqlplus | `nsql run -c app/pw@//db:1521/svc @q.sql` | ✅ 이미 동작(`@`는 위치 인자에서 벗겨 파일로) |
| usql | `nsql run -c postgres://app:pw@db/sales -c "SELECT 1"` | 두 번째 `-c`는 SQL이 아님 → `-e`로 안내 |

---

## 3. 결정(D-41 · 사용자)

| 안 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **ⓐ psql 기본(권장)** | `-h -p(포트) -U -d(DB) -W` · 방언 `-T` · 비번 `--password`/env · mysql 별칭 `-u -P(포트) -D -e` · sqlcmd 별칭 `-S -Q -i -b` · sqlcmd `-P`(비번)만 거부 | 가장 널리 쓰이는 관례(PG 계열 7종이 psql 호환) · 오픈소스 개발자 직관 | 기존 `-d`(방언)·`-p`(비번) 의미 변경(0.0.x라 비용 0) |
| ⓑ 현행 유지 | `-d` 방언 · `-p` 비번 | 변경 없음 | psql·sqlcmd 사용자에게 `-d`가 배신 |
| ⓒ 짧은 옵션 최소화 | 긴 옵션만 · `-c`·`-h`·`-U`만 | 충돌 0 | 타이핑 많음 · 익숙함 낮음 |

---

## 4. 후속 작업

| ID | 항목 | 의존 |
|---|---|---|
| **T-51** | D-41 반영 — 옵션 파서 재정리(별칭 표 · 충돌 안내) · `-e` · `-v` · `-b` · `-W`/`NSQL_PASSWORD` · `-S host,port` · `--help`를 진영별 예시 3줄로 | D-41 |
| **T-52** | 셸 명령 대응표 — `\d`·`:r`·`.tables`를 SQL*Plus식 `DESC`·`@`·`SHOW TABLES` 별칭으로 받기(docs/11 §shell) | T-7 |

---

## 4-1. 표 폭(09-16 구현) — sqlplus LINESIZE · psql `\pset`/`\x` · sqlcmd `-w`/`-y` 대응

| 층 | 우리 | 대응 |
|---|---|---|
| 영구 기본값 | `nsql config set cli.width N`(0 = 터미널 폭 · 파이프면 무제한) · `cli.max_col_width`(60) · `cli.overflow`(wrap 기본 · truncate · expanded · none) | psql `~/.psqlrc` `\pset` · sqlplus `login.sql` `SET LINESIZE` |
| 1회성 | `--width N` · `--max-col-width N` · `--overflow …` · `-x` | sqlcmd `-w`/`-y` · psql `-x` |
| 세션 | 셸 `set width <n|auto>` · `set colwidth <n>` · `set overflow …` · `\x` · `show` | sqlplus `SET LINESIZE` · psql `\x`/`\pset` |

wrap = 컬럼을 폭에 맞는 묶음으로 나눠 묶음마다 표(맨 앞 `#` 행 번호) · 구현 nsql-io `GridOpts`.

## 5. 사용자 제공 CLI 목록(참고 · 기본 포트는 접속 폼 `Dialect::default_port`의 확장 근거)

RDBMS: sqlplus/sql(SQLcl) 1521 · psql 5432 · mysql/mariadb 3306 · sqlcmd/mssql-cli 1433 · db2 50000 · sqlite3 — · hdbsql 30015 · isql(ASE) 5000 · dbaccess 9088 · isql-fb 3050 · H2 Shell 9092 · ij(Derby) 1527 | 국산: tbsql 8629 · isql(Altibase) 20300 · csql(CUBRID) 33000 · gsql(GOLDILOCKS) 22581 | NoSQL: mongosh 27017 · redis-cli 6379 · cqlsh 9042 · elasticsearch-sql-cli 9200 · cypher-shell 7687 · influx 8086 · hbase shell 16000 · cbq 8091 · etcdctl 2379 · memcached 11211 · CouchDB 5984 | 분석/DW: snowsql · bq · psql/rsql(Redshift) · clickhouse-client 9000 · duckdb · bteq · vsql 5433 · psql(Greenplum) · nzsql · beeline · trino/presto-cli · impala-shell · spark-sql | NewSQL: cockroach sql · ysqlsh/ycqlsh · mysql(TiDB) · SurrealDB.

→ ODBC 폴백 방언(`Dialect::Odbc` · Tibero/Altibase/CUBRID)의 기본 포트 표는 [22 드라이버 확장](22-driver-extensions.md) manifest `default_port`로 확장(드라이버가 안다 · 앱은 모른다).

---

## 6. `CONNECT` 동사 — 우리 명령과 DBMS 네이티브의 혼동(사용자 09-14)

**사실 확인**: SQL*Plus의 `CONNECT`는 **서버 명령이 아니라 클라이언트 명령**이다(sqlplus가 해석 · 서버로 보내지 않는다). psql `\c`, sqlcmd `:connect`, mysql `
`/`connect`도 같다 — 모두 클라이언트 셸 어휘다. 따라서 **서버 쪽 충돌은 없다.** 혼동은 "어느 클라이언트의 문법인가"뿐이다.

| 안 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **ⓐ 래퍼 하나(권장)** | `CONNECT`는 **우리 클라이언트 명령 하나**. SQL*Plus 문법(`user/pw@host:port/svc` · `/ as sysdba`)을 **그대로 받고**, 프로필 이름·URL(`mssql://…`)을 **상위 집합**으로 더한다(이미 이렇게 구현 — docs/08·11 `Command::Connect`). 다른 셸 어휘(`\c` · `:connect` · `
`)는 **별칭**(T-52) | SQL*Plus 스크립트가 수정 없이 돈다(DR-6 워크플로 1급) · 배울 명령 1개 · 방언이 바뀌어도 같은 동사 | "이건 오라클 명령인가?"라는 질문이 남는다 → **문서·도움말에 "클라이언트 명령"임을 명시** · `SHOW COMMANDS`에 출처 표기 |
| ⓑ 네이티브/래퍼 분리 | 방언별 네이티브(`CONNECT`=Oracle · `\c`=PG …)는 그 방언에서만, 우리 것은 `\connect` 또는 `@connect` | 출처가 이름에 보인다 | SQL*Plus 스크립트의 `CONNECT`가 MSSQL 세션에선 오류 → 이식성 손실 · 사용자가 두 문법을 배운다 · 혼합 스크립트(프로필로 방언 전환)가 깨진다 |
| ⓒ 접두 필수 | 우리 명령은 전부 `\`·`@` 접두(psql식) · 맨몸 `CONNECT`는 서버로 보냄 | 애매함 0 | Oracle 서버는 `CONNECT`를 모른다(ORA-00900) — 맨몸 CONNECT가 쓸모없어진다 · SQL*Plus 호환 포기 |

**권장 = ⓐ + 명시**: ① `CONNECT`는 우리 클라이언트 명령이며 SQL*Plus 상위 호환(도움말 첫 줄에 명시) ② 로그 창에 `connect` 엔트리가 "client"로 찍힌다(서버로 안 감을 눈으로 확인) ③ 셸 별칭 `\c`·`:connect`(T-52) ④ **세션 모드와의 관계**([24] `session.mode`): `shared`면 편집기 안의 `CONNECT`가 **모든 탭의 세션을 바꾼다** → 상태줄 경고 + 로그 · `per-editor`면 그 탭만 ⑤ 서버로 보내야 하는 드문 경우(예: MySQL Shell의 `\connect`와 같은 이름의 사용자 프로시저)는 `EXEC`/`CALL`로 감싸므로 충돌 없음.

→ 결정 **D-45**: ⓐ(권장) / ⓑ / ⓒ.

