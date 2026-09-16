# 40 · CLI `nsql` 사용법 — 순서대로 따라 하는 기능 점검

> **요청**(사용자 09-16): *"CLI 사용 방법이 정리된 문서가 있는지 확인하고 필요하면 만들어줘 · 추가된 기능들을 테스트할 수 있도록 순서대로 · `conn list`를 사용한 접속뿐 아니라 직접 hostname 등을 지정해서 접속하는 방법도 기술."*
> **성격**: 이 문서는 **사용·점검 안내**다. 설계 제안은 [11 CLI](11-cli.md), 인자 규약과 타 CLI 조사는 [27 CLI 인자 규약](27-cli-conventions.md), 연결 프로필 내부는 [21](21-connection-profiles.md)에 있다.
> **검증**: 이 문서의 모든 명령·출력은 `target/release/nsql.exe`(빌드 09-16 14:00 · 커밋 2d7d850 이후)로 **실제 실행해 확인**했다(Windows · 드라이버 sqlite·oracle·mssql·postgres).
> **원장**: 옵션·인자의 최종 근거는 `nsql <명령> --help`(`nsql-cli/src/help.rs`)다. 어긋나면 도움말이 맞다.

---

## 0. 준비

```sh
cargo build --release              # target/release/nsql.exe · nexa-sql.exe
cargo build                        # 디버그도 함께(실기 테스트는 디버그 exe로 · CLAUDE.md 규약)
```

이 문서는 `nsql`을 PATH에 둔 것으로 적는다. 아니면 `./target/release/nsql.exe`로 바꿔 읽는다.

### 도움말이 원장이다

CLI 도움말은 `nsql-cli/src/help.rs` 한 곳에서 나온다(인자·옵션·예제 원장 · 영/한 `Msg`). **이 문서와 도움말이 어긋나면 도움말이 맞다.**

```sh
nsql --help            # 개요: 명령 8개 · 접속 문자열 · 공통 옵션 · 이 빌드의 드라이버
nsql run --help        # 한 명령의 인자·옵션·예제 전부
nsql help run          # 같은 내용
nsql -V                # 판올림
```

`nsql --help` 마지막 줄이 **이 빌드에 실제로 들어간 드라이버**를 알려준다:

```
Drivers in this build: sqlite, oracle, mssql, postgres
```

### 명령 8개

| 명령 | 하는 일 |
|---|---|
| `run` | 스크립트(파일·stdin) 실행하고 결과 출력 |
| `shell` | 대화형 셸(줄 단위 · sqlplus/psql 식) |
| `export` | 질의나 테이블 전체를 파일로 |
| `explain` | 실행 계획(방언별 EXPLAIN) |
| `plan` | **접속 없이** 스크립트 파싱 — 문장·변수·재작성 |
| `conn` | 연결 프로필(사용자 폴더 저장 · GUI와 공유) |
| `cat` | 카탈로그 — 스키마·객체·컬럼·소스·컴파일 오류 |
| `config` | 앱 설정(`settings.conf` · GUI와 공유) |

---

## 1. 접속을 지정하는 두 가지 방법

`-c <target>`은 **① 접속 문자열**이나 **② 프로필 이름**을 받는다. 둘 다 같은 자리에 쓴다.

### 1-1. 직접 지정 — 접속 문자열 (프로필 없이)

가장 빠른 방법이다. 저장하지 않고 그때그때 붙는다.

| 방언 | 형식 | 예 |
|---|---|---|
| **Oracle** | `oracle://user:pass@host:port/service` | `oracle://scott:tiger@192.168.0.58:1521/BISCM` |
| **Oracle**(SQL\*Plus 꼴) | `user/pass@host:port/service` | `BISCM/pw@192.168.0.58:1521/BISCM` |
| **SQL Server** | `mssql://user:pass@host:port/db` | `mssql://sa:pw@192.168.0.58:1433/M4PLAN_MS` |
| **PostgreSQL** | `postgres://user:pass@host:port/db` | `postgres://postgres:pw@192.168.0.60:5433/matrixdb2` |
| **SQLite(파일)** | `sqlite:<경로>` | `sqlite:C:\data\app.db` · `sqlite:./app.db` |
| **SQLite(메모리)** | `sqlite::memory:` | 서버 없이 시험할 때 |

**기본 포트**(생략 가능): Oracle 1521 · SQL Server 1433 · PostgreSQL 5432 · MySQL 3306.

#### (a) 모든 명령에 그대로 쓴다

`-c` 자리에 프로필 이름 대신 문자열을 넣기만 하면 된다. **저장되는 것은 없다.**

```sh
# 한 줄 질의 — 스크립트 파일도 필요 없다(`-` = 표준 입력)
echo "SELECT sysdate FROM dual;" | nsql run -c "oracle://scott:tiger@dbhost:1521/ORCL" -

# 스크립트 실행
nsql run     -c "mssql://sa:pw@192.168.0.58:1433/M4PLAN_MS" deploy.sql

# 대화형 셸
nsql shell   -c "postgres://postgres:pw@192.168.0.60:5433/matrixdb2"

# 내보내기
nsql export  -c "oracle://scott:tiger@dbhost:1521/ORCL" -t EMP -f csv -o emp.csv

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

#### (b) 비밀번호를 주는 네 가지 방법

| 방법 | 쓰기 | 언제 |
|---|---|---|
| 문자열에 직접 | `-c "oracle://scott:tiger@h:1521/ORCL"` | 빠르지만 **히스토리에 남는다** |
| **생략 → 숨김 입력** | `-c "oracle://scott@h:1521/ORCL"` | 사람이 직접 칠 때 **권장** |
| **환경변수 `NSQL_PASSWORD`** | `export NSQL_PASSWORD='s3cr3t'` | CI·스크립트 — **전용 변수가 있다** |
| 필드 + `-p` | `-d oracle --host h --user scott -p pw` | 문자열 조립이 번거로울 때 |

찾는 순서는 **`-p` → `NSQL_PASSWORD` → 숨김 프롬프트**이고, `--no-prompt`면 3번을 건너뛰고 실패한다.

```sh
# 비밀번호를 빼면 실행 시 숨김 입력으로 묻는다
nsql run -c "oracle://scott@dbhost:1521/ORCL" script.sql
Password:

# CI — 물어보지 못하므로 --no-prompt(없으면 실패시킨다)
DBPW='s3cr3t'
nsql run -c "postgres://app:$DBPW@dbhost:5432/appdb" --no-prompt migrate.sql
```

> ⚠️ **셸 히스토리·프로세스 목록 주의.** 명령줄에 쓴 비밀번호는 `history`와 `ps`/작업 관리자에 노출된다. 사람이 칠 때는 **비밀번호를 빼고 프롬프트**를, 자동화에는 **환경변수**를 쓴다. 반복해서 쓸 접속은 §1-2 프로필이 안전하다.

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

확인해 보면 이렇게 풀린다:

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
| PowerShell | `-c "oracle://u:p@h:1521/ORCL"` — 변수는 `"...$env:DBPW..."` |
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
how     n
------  -
direct  2

1 rows (0.000s)
```

#### (g) 문자열 대신 필드로

문자열 조립이 번거로우면 필드로 준다. **`conn` 전용이 아니라 `run` `shell` `export` `explain` `cat` 전부**가 받는다.

```sh
nsql run   -d postgres --host 192.168.0.60 --port 5433 --db matrixdb2 --user postgres migrate.sql
nsql shell -d oracle   --host 192.168.0.58 --port 1521 --db BISCM --user BISCM
nsql cat   -d mssql    --host srv --db M4PLAN_MS --user sa tables
nsql conn test -d oracle --host 192.168.0.58 --port 1521 --db BISCM --user BISCM   # 비밀번호는 물어봄
```

포트를 빼면 방언 기본값(1521 · 1433 · 5432)이 쓰인다.

### 1-2. 프로필 — 한 번 저장하고 이름으로 부르기

비밀번호가 **암호화되어 사용자 폴더에 저장**되고, **CLI와 GUI가 같은 폴더를 공유**한다([21](21-connection-profiles.md) · [DR-22](10-decision-record.md)).

```sh
# 접속 문자열로 저장(비밀번호가 없으면 숨김 입력으로 묻는다)
nsql conn add prod "oracle://BISCM@192.168.0.58:1521/BISCM"

# 필드로 저장
nsql conn add pg1 -d postgres --host 192.168.0.60 --port 5433 --db matrixdb2 --user postgres

# 확인·시험·삭제
nsql conn list
nsql conn show prod
nsql conn test prod          # 실제로 붙어 본다
nsql conn rm  prod
nsql conn path               # 저장 폴더 경로
```

`conn list` 출력:

```
NAME          DIALECT   PW   TARGET
M4PLAN        mssql     ✓    BISCM_MS@192.168.0.58:1433/M4PLAN_MS
SNOPDB_19c    oracle    ✓    BISCM@61.81.244.214:1521/BISCM
biscm         oracle    ✓    BISCM@192.168.0.58:1521/BISCM
matrixdb2     postgres  ✓    postgres@192.168.0.60:5433/matrixdb2
```

`PW` 열의 `✓`는 비밀번호가 저장됐다는 뜻, `-`는 실행 시 묻는다는 뜻이다.

이후 어디서나 이름으로 쓴다:

```sh
nsql run    -c prod script.sql
nsql export -c prod -t emp -f csv -o emp.csv
nsql shell  -c prod
```

스크립트 안에서도 쓴다 — `CONNECT prod`.

**저장 위치**

| OS | 경로 |
|---|---|
| Windows | `%APPDATA%\nexa-sql\profiles\<이름>.conf` |
| macOS | `~/Library/Application Support/nexa-sql/profiles/` |
| Linux | `~/.config/nexa-sql/profiles/` |

`NSQL_HOME`을 설정하면 그 폴더로 바뀐다(테스트·개발용). 비밀번호만 봉투로 암호화되고 호스트·사용자명은 평문이다(목록에 보여야 하는 값). Windows는 DPAPI 기기 키라 **같은 Windows 계정에서만** 풀린다.

### 1-3. 둘 중 무엇을 쓰나

| 상황 | 권장 |
|---|---|
| 일회성 · 남기고 싶지 않음 | 접속 문자열 |
| 매일 쓰는 서버 · GUI와 공유 | 프로필 |
| CI · 컨테이너 | 접속 문자열 + `--no-prompt`(비밀번호는 환경변수로 조립) |

---

## 2. 순서대로 — 서버 없이 되는 점검 (SQLite)

여기까지는 **DB 서버가 없어도** 전부 된다. 위에서부터 차례로 실행하면 기능이 하나씩 늘어난다.

### Step 1 — 엔진만 시험 (접속조차 불필요)

`plan`은 스크립트를 **실행하지 않고** 방언별 재작성 계획만 보여준다. 세션 변수 엔진([08](08-session-variables.md))을 검증하는 가장 빠른 방법이다.

```sh
nsql plan -d mssql examples/golden-session-vars.sql
nsql plan -d oracle examples/golden-session-vars.sql
```

```
# dialect=mssql items=6

[1] line 2 · Exec
  → LOCAL  :V_PRG_NM = N'SP_M4P_MPO_M4E_CREATE_BSY'

[3] line 6 · Exec
  → EXECUTE (Block · Bind · out=true)
    | EXEC SP_M4P_MP_VERSION_CREATE(@PC_RET, 'SEBANG', …)
    · PC_RET Auto InOut = NULL
    ! 암묵 변수: PC_RET
```

★ **스크립트 엔진을 고쳤으면 두 방언 모두 다시 볼 것**(CLAUDE.md 규약).

### Step 2 — 실제 실행 (메모리 DB)

```sh
nsql run -c "sqlite::memory:" examples/sqlite-session-vars.sql
```

```
Connected: sqlite://:memory: (sqlite)
0 rows affected (0.012s)
3 rows affected (0.000s)
:V_DEPT = DEV
V_DEPT = DEV
V_MAX = 520
name    sal
------  ---
김철수  450

1 rows (0.000s)
```

세션 변수 대입 → `PRINT` → 바인드 조회까지 한 번에 확인된다.

### Step 3 — 파일 DB로 표 만들기

```sh
cat > t1.sql <<'SQL'
CREATE TABLE emp (id INTEGER, name TEXT, dept TEXT, sal INTEGER);
INSERT INTO emp VALUES (1,'Kim','SALES',5000),(2,'Lee','IT',6200),(3,'Park','IT',4800);
SELECT * FROM emp ORDER BY sal DESC;
SQL

nsql run -c "sqlite:t1.db" t1.sql
```

```
id  name  dept    sal
--  ----  -----  ----
 2  Lee   IT     6200
 1  Kim   SALES  5000
 3  Park  IT     4800

3 rows (0.000s)
```

### Step 4 — 출력 형식 (`-f`)

```sh
nsql run -c "sqlite:t1.db" -f json  q.sql
nsql run -c "sqlite:t1.db" -f csv   q.sql
nsql run -c "sqlite:t1.db" -f tsv   q.sql
nsql run -c "sqlite:t1.db" -f jsonl q.sql
```

```json
[
  {"id": 2, "name": "Lee", "dept": "IT", "sal": 6200},
  {"id": 1, "name": "Kim", "dept": "SALES", "sal": 5000}
]
```

`grid`(기본)는 사람용, 나머지는 파이프·CI용이다.

### Step 5 — 행 수 상한 (`--max-rows`)

```sh
nsql run -c "sqlite:t1.db" --max-rows 2 q.sql
```

```
2 rows (0.000s) (more rows available — raise --max-rows)
```

잘렸다는 사실을 **명시**한다 — 조용히 자르지 않는다.

### Step 6 — 성능 계측 (`--timing`)

```sh
nsql run -c "sqlite:t1.db" --timing q.sql
```

```
3 rows (0.001s)

⏱ execute 500µs (3 rows)
```

단계별 시간은 `nsql_core::Timeline`이 쌓는다([26 성능](26-performance-architecture.md)). 실행 경로를 고쳤으면 여기로 확인한다.

### Step 7 — 실행 로그 (`--log`)

```sh
nsql run -c "sqlite::memory:" --log - <<< "SELECT 1 AS a;"
```

```
2026-09-16 11:24:13.198  connect   sqlite://:memory: (sqlite)
2026-09-16 11:24:13.198  send      line 1: Query
2026-09-16 11:24:13.198  done      1 rows  148µs  result set
2026-09-16 11:24:13.198  execute   1 rows  148µs  first response · execute+fetch
```

GUI 로그 창(Ctrl+`)과 같은 내용이다. 형식은 `log.format` 설정(raw/markdown/grid)을 따른다.

### Step 8 — 카탈로그 탐색 (`cat`)

```sh
nsql cat -c "sqlite:t1.db" schemas
nsql cat -c "sqlite:t1.db" kinds
nsql cat -c "sqlite:t1.db" tables
nsql cat -c "sqlite:t1.db" columns emp
```

```
#  Name  Type     Nullable  Default
-  ----  -------  --------  -------
1  id    INTEGER  Y
2  name  TEXT     Y
3  dept  TEXT     Y
4  sal   INTEGER  Y
```

**종류(kind)**: `tables views mviews procs funcs packages bodies sequences triggers indexes synonyms types`

```sh
nsql cat -c prod -s HR tables            # 스키마 지정
nsql cat -c prod source procs MY_PROC    # 원본 소스
nsql cat -c prod errors MY_PROC          # 컴파일 오류(Oracle)
nsql cat -c prod -f json tables          # 형식 지정
```

### Step 9 — 실행 계획 (`explain`)

```sh
nsql explain -c "sqlite:t1.db" -q "SELECT * FROM emp WHERE dept='IT'"
nsql explain -c prod plan.sql
```

```
id  parent  notused  detail
--  ------  -------  --------
 2       0        0  SCAN emp
```

방언별 `EXPLAIN` 관용을 알아서 쓴다(Oracle `EXPLAIN PLAN` + `DBMS_XPLAN` · PG `EXPLAIN` · MSSQL `SHOWPLAN`).

### Step 10 — 내보내기 (`export`)

```sh
nsql export -c "sqlite:t1.db" -t emp -f csv                    # 표 전체 → 화면
nsql export -c "sqlite:t1.db" -t emp -f csv -o emp.csv         # 파일로
nsql export -c "sqlite:t1.db" -q "SELECT * FROM emp WHERE sal>5000" -f json -o rich.json
nsql export -c "sqlite:t1.db" -t emp -f insert:NEW_EMP -o seed.sql   # INSERT 문 생성
```

```
id,name,dept,sal
1,Kim,SALES,5000
2,Lee,IT,6200
3,Park,IT,4800
```

형식: `csv` `tsv` `json` `jsonl` `insert[:테이블명]`.

### Step 11 — 대화형 셸 (`shell`)

```sh
nsql shell -c "sqlite:t1.db"
```

```
nsql shell — `;`·단독 `/`·명령 줄로 실행 · exit 종료 · 변수는 세션 동안 유지
nsql> SELECT name, sal FROM emp WHERE dept='IT';
name   sal
----  ----
Lee   6200
Park  4800

2 rows (0.001s)
nsql> exit
```

- 실행: `;` · 단독 `/` · 명령 줄
- 종료: `exit` / `quit` / `\q`
- **세션 변수는 셸이 살아 있는 동안 유지**된다
- 파이프도 된다: `printf 'SELECT 1;\nquit\n' | nsql shell -c "sqlite::memory:"`

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

nsql> set format json
nsql> SELECT * FROM emp WHERE dept='IT';
[
  {"id": 2, "name": "Lee", "dept": "IT", "sal": 6200},
  {"id": 3, "name": "Park", "dept": "IT", "sal": 4800}
]
```

**형식 이름**

| 값 | 설명 |
|---|---|
| `grid` (기본) | 사람이 읽는 텍스트 표 · 별칭 `default` `table` `ansiconsole` |
| `markdown` | GitHub 표(숫자 오른쪽 정렬) · 별칭 `md` |
| `csv` · `tsv` | 구분자 · `tsv` 별칭 `delimited` |
| `json` | 배열 `[{col: val}, …]` |
| `jsonl` | 줄당 객체 하나 · 별칭 `jsonlines` `ndjson` |
| `sql[:종류]` | 행마다 SQL 문 — `sql:select` `sql:insert` `sql:update` `sql:delete` `sql:merge`(기본 `insert`) · 키 규칙 [41](41-sql-copy-key-rules.md) |
| `insert[:테이블]` | `INSERT INTO 테이블 … VALUES (…);` |

#### 표 모양 명령 (`grid`일 때)

| 명령 | 뜻 |
|---|---|
| `set width <n\|auto>` | 한 줄 최대 폭 · `0` = 무제한 · `auto` = 터미널 폭 (SQL\*Plus `LINESIZE`) |
| `set colwidth <n>` | 한 열 최대 폭 · `0` = 무제한(넘으면 `…`) (sqlcmd `-y`) |
| `set overflow none\|wrap\|truncate\|expanded` | `wrap` = 열 묶음 + 행 번호 · `truncate` = 줄여서 맞춤 · `expanded` = 한 줄에 한 열 · `none` = 행당 한 줄(기본) |
| **`\x`** | `expanded` 토글 (psql 관습) |

```
nsql> \x
width 0 · colwidth 60 · overflow expanded · format grid

nsql> SELECT * FROM emp WHERE id=1;
-[ RECORD 1 ]----
id   | 1
name | Kim
dept | SALES
sal  | 5000
```

#### 현재 값 보기 · 클립보드

| 명령 | 뜻 |
|---|---|
| `set` · `show` · `\pset` · `help set` | 현재 값 전부 + 명령 목록 |
| `get <키>` · `show <키>` | 값 하나 — `width` `colwidth` `overflow` `format` `all` |
| **`copy [형식]`** | **직전 결과**를 클립보드로(형식 생략 = 현재 형식) |

```
nsql> get format
format = json

nsql> copy markdown
3 rows copied as markdown
```

#### 세 가지 층위

| 범위 | 방법 |
|---|---|
| **이번 질의부터(셸 안)** | `set format markdown` |
| **이번 실행만** | `nsql run -f markdown …` · `--width` `--max-col-width` `--overflow`/`-x` |
| **영구(GUI와 공유)** | `nsql config set cli.format markdown` |

```sh
nsql config set cli.format     markdown     # 기본 출력 형식
nsql config set cli.width      auto         # 터미널 폭에 맞춤
nsql config set cli.max_col_width 40
nsql config set cli.overflow   wrap
nsql config list | grep ^cli.               # 현재 값 확인
```

우선순위는 **셸 `set` > 명령줄 플래그 > `cli.*` 설정**이다.

### Step 12 — 설정 (`config`) — GUI와 공유

```sh
nsql config list              # 전체 키 · 현재 값 · (기본값) · 설명
nsql config get ui.lang
nsql config set ui.lang ko    # 한국어
nsql config set ui.theme dark
nsql config reset ui.lang
nsql config path
```

```
[User Interface ▸ Appearance]
ui.lang            en       (default)  [en | ko]  Language of the user interface (applies immediately)
```

GUI 설정 창과 **같은 `settings.conf`**를 본다. 새 설정 키는 `nsql-settings::REGISTRY`에만 추가한다(CLAUDE.md 규약).

---

## 3. 실서버 점검 (Oracle · SQL Server · PostgreSQL)

서버가 준비되면 §2와 같은 순서를 실서버로 반복한다.

```sh
# 1) 붙는지부터
nsql conn test -d oracle --host 192.168.0.58 --port 1521 --db BISCM --user BISCM
#    또는 저장해 두고
nsql conn add biscm "oracle://BISCM@192.168.0.58:1521/BISCM"
nsql conn test biscm

# 2) 카탈로그가 보이는지
nsql cat -c biscm schemas
nsql cat -c biscm tables

# 3) 통합 테스트 스크립트
nsql run -c biscm  examples/it-oracle.sql
nsql run -c M4PLAN examples/it-mssql.sql
nsql run -c matrixdb2 examples/it-pg.sql

# 4) 계측·로그를 켜고 한 번 더
nsql run -c biscm --timing --log examples/it-oracle.sql
```

### 프로필을 전혀 만들지 않고 같은 순서로

저장을 남기고 싶지 않을 때(남의 PC · 일회성 점검 · CI)는 §1-1 문자열을 그대로 반복하면 된다. 매번 치기 번거로우면 **셸 변수에 한 번만 담는다.**

```sh
# 비밀번호는 묻게 두고 접속만 변수로
DB="oracle://BISCM@192.168.0.58:1521/BISCM"

nsql conn test -d oracle --host 192.168.0.58 --port 1521 --db BISCM --user BISCM
nsql cat     -c "$DB" schemas
nsql cat     -c "$DB" tables
nsql run     -c "$DB" examples/it-oracle.sql
nsql run     -c "$DB" --timing --log examples/it-oracle.sql
nsql export  -c "$DB" -t EMP -f csv -o emp.csv
```

```powershell
# PowerShell
$DB = "mssql://sa:$env:DBPW@192.168.0.58:1433/M4PLAN_MS"
nsql cat -c $DB tables
nsql run -c $DB --no-prompt examples\it-mssql.sql
```

비밀번호를 매번 묻는 게 번거롭고 저장도 싫다면 **셸 세션 동안만 환경변수**에 둔다 — 창을 닫으면 사라진다:

```sh
read -s DBPW && export DBPW
nsql run -c "oracle://BISCM:$DBPW@192.168.0.58:1521/BISCM" --no-prompt examples/it-oracle.sql
```

### Oracle 주의

Instant Client가 **런타임에 필요**하다. 못 찾으면 `DPI-1047`이 난다.

```sh
# Windows
set NSQL_ORACLE_CLIENT_DIR=C:\oracle\instantclient_21_13
# macOS/Linux
export NSQL_ORACLE_CLIENT_DIR=/opt/oracle/instantclient_21_13
```

### 서버 메시지

MSSQL `PRINT`/`RAISERROR`와 PostgreSQL `NOTICE`는 **실행 중 실시간**으로 나온다. Oracle `DBMS_OUTPUT`은 폴링 모니터 방식이다([32](32-server-messages-and-live-log.md)).

---

## 4. 출력 형식 상세

결과를 어떤 모양으로 낼지는 **형식(`-f`)**과 **표 모양(폭·넘침)** 두 축으로 정한다.

### 4-1. 형식 한눈에

| 형식 | 별칭 | 무엇에 쓰나 |
|---|---|---|
| `grid` | `default` `table` `ansiconsole` | **기본.** 사람이 읽는 텍스트 표 |
| `markdown` | `md` | 편집기·위키·PR 본문에 붙여 넣기 |
| `csv` | — | 엑셀·스프레드시트 |
| `tsv` | `delimited` | 탭 구분 · 붙여 넣기 |
| `json` | — | 배열 하나 — API·`jq` |
| `jsonl` | `jsonlines` `ndjson` | 줄당 객체 하나 — 스트리밍·로그 적재 |
| `sql:select` | — | 행마다 `SELECT … WHERE 키` |
| `sql:insert` | `sql` | 행마다 `INSERT` |
| `sql:update` | — | 행마다 `UPDATE … WHERE 키` |
| `sql:delete` | — | 행마다 `DELETE … WHERE 키` |
| `sql:merge` | — | 행마다 `MERGE`(upsert) |
| `insert:<테이블>` | `insert` | 테이블 이름을 바꿔 `INSERT` 생성 |

### 4-2. 실제 출력 (같은 결과, 형식만 바꿈)

기준 결과:

```
id  name  dept    sal
--  ----  -----  ----
 2  Lee   IT     6200
 1  Kim   SALES  5000
```

**`-f markdown`** — 숫자 열은 오른쪽 정렬 표시(`---:`)가 붙는다.

```
| id | name | dept | sal |
| ---: | --- | --- | ---: |
| 2 | Lee | IT | 6200 |
| 1 | Kim | SALES | 5000 |
```

**`-f csv`** / **`-f tsv`**

```
id,name,dept,sal
2,Lee,IT,6200
1,Kim,SALES,5000
```

**`-f json`** — 배열 하나. `jq`로 바로 먹인다.

```json
[
  {"id": 2, "name": "Lee", "dept": "IT", "sal": 6200},
  {"id": 1, "name": "Kim", "dept": "SALES", "sal": 5000}
]
```

**`-f jsonl`** — 줄당 객체 하나. 큰 결과를 스트리밍할 때.

```
{"id": 2, "name": "Lee", "dept": "IT", "sal": 6200}
{"id": 1, "name": "Kim", "dept": "SALES", "sal": 5000}
```

### 4-3. SQL 형식 5종 — 결과를 실행문으로

결과 행을 **다시 실행할 수 있는 SQL**로 만든다. 데이터 이관·재현·패치 스크립트에 쓴다. 테이블 이름은 실행한 문장에서 추정한다.

**`-f sql:insert`** (또는 그냥 `sql`)

```sql
INSERT INTO emp (id, name, dept, sal) VALUES (2, 'Lee', 'IT', 6200);
INSERT INTO emp (id, name, dept, sal) VALUES (1, 'Kim', 'SALES', 5000);
```

**`-f sql:update`**

```sql
UPDATE emp SET sal = 6200 WHERE id = 2 AND name = 'Lee' AND dept = 'IT';
```

**`-f sql:delete`**

```sql
DELETE FROM emp WHERE id = 2 AND name = 'Lee' AND dept = 'IT';
```

**`-f sql:select`**

```sql
SELECT id, name, dept, sal FROM emp WHERE id = 2 AND name = 'Lee' AND dept = 'IT';
```

**`-f sql:merge`** — upsert 한 문장

```sql
MERGE INTO emp t USING (SELECT 2 AS id, 'Lee' AS name, 'IT' AS dept, 6200 AS sal) s
  ON (t.id = s.id AND t.name = s.name AND t.dept = s.dept)
  WHEN MATCHED THEN UPDATE SET t.sal = s.sal
  WHEN NOT MATCHED THEN INSERT (id, name, dept, sal) VALUES (s.id, s.name, s.dept, s.sal);
```

**`-f insert:<테이블>`** — 다른 테이블로 옮길 때

```sh
nsql export -c prod -t EMP -f insert:EMP_BACKUP -o seed.sql
```

```sql
INSERT INTO EMP_BACKUP (id, name, dept, sal) VALUES (2, 'Lee', 'IT', 6200);
```

#### ⚠️ WHERE 절의 키는 이렇게 고른다

`update` · `delete` · `merge` · `select`는 행을 **식별할 키**가 필요하다. 순서는 [41 결과 → SQL 키 규칙](41-sql-copy-key-rules.md)을 따른다:

1. **기본 키(PK)**
2. 없으면 **첫 유니크 인덱스**
3. 그것도 없으면 **앞 3개 컬럼 + 경고**

3번으로 떨어지면 출력 끝에 주석으로 한 번 경고한다:

```sql
-- WARNING: no primary key or unique index found for emp — the first 3 column(s)
-- (id, name, dept) were used as the key. Check WHERE/ON before running (setting sql.key_mode)
```

> **이 경고가 보이면 실행 전에 `WHERE`/`ON`을 직접 확인한다.** 앞 3개 컬럼이 행을 유일하게 고르지 못하면 **의도보다 많은 행이 바뀐다.**

키 선택은 설정으로 바꾼다:

```sh
nsql config set sql.key_mode pk     # 기본 — PK → 유니크 → 앞 3컬럼(경고)
nsql config set sql.key_mode all    # 모든 컬럼을 WHERE에 넣는다(안전하지만 길다)
```

### 4-4. 표 모양 — `grid`일 때만

| 축 | 플래그 | 설정 | 셸 |
|---|---|---|---|
| 한 줄 최대 폭 | `--width <n>` · `--linesize` | `cli.width` | `set width <n\|auto>` |
| 한 열 최대 폭 | `--max-col-width <n>` | `cli.max_col_width` (60) | `set colwidth <n>` |
| 넘칠 때 처리 | `--overflow <mode>` | `cli.overflow` (`none`) | `set overflow <mode>` |
| 레코드 보기 | `-x` · `--expanded` | — | `\x` |

**폭**: `0` = 무제한(파이프로 넘길 때) · `auto` = 터미널 폭. SQL\*Plus `LINESIZE`에 해당한다.
**열 폭**: `0` = 무제한, 넘치면 끝에 `…`가 붙는다. sqlcmd `-y`에 해당한다.

**넘침 처리 4가지**

| 모드 | 동작 |
|---|---|
| `none` (기본) | 행당 한 줄로 길게 — 편집기에 붙이면 정렬이 유지된다 |
| `wrap` | 열을 묶음으로 나눠 아래로 · 행 번호 `#` 열이 붙는다 |
| `truncate` | 열을 줄여서 폭에 맞춘다 |
| `expanded` | 한 줄에 한 열 (psql `\x`) |

**`-x` / `\x` 레코드 보기** — 열이 많을 때 가장 읽기 쉽다. 폭과 무관하게 항상 레코드로 낸다.

```
-[ RECORD 1 ]----
id   | 1
name | Kim
dept | SALES
sal  | 5000
```

### 4-5. 어디서 정하나 — 세 층위

| 범위 | 방법 | 예 |
|---|---|---|
| **셸 안에서 즉시** | `set format` | `set format markdown` |
| **이번 실행만** | 플래그 | `-f markdown --width 120 -x` |
| **영구(GUI와 공유)** | 설정 | `nsql config set cli.format markdown` |

우선순위는 **셸 `set` > 플래그 > `cli.*` 설정**이다.

```sh
nsql config set cli.format        markdown
nsql config set cli.width         auto
nsql config set cli.max_col_width 40
nsql config set cli.overflow      wrap
nsql config set sql.key_mode      pk
nsql config list | grep ^cli.
```

### 4-6. stdout / stderr 분리 — 파이프에 중요

**결과 데이터는 stdout, 나머지(접속 알림·진행·경고·`--timing`·`--log`)는 stderr**로 나간다. 그래서 리다이렉트하면 **데이터만** 남는다.

```sh
nsql run -c prod -f csv query.sql > out.csv          # out.csv에는 CSV만
nsql run -c prod -f json query.sql | jq '.[].name'   # 바로 jq로
nsql run -c prod -f markdown q.sql > result.md       # PR에 붙일 표
```

**종료 코드 = 실패한 항목 수**(0 = 전부 성공). CI에서 그대로 쓴다.

```sh
nsql run -c prod --no-prompt migrate.sql || echo "실패 $? 건"
```

### 4-7. 클립보드로 — 셸 `copy`

직전 결과를 클립보드에 넣는다(형식 생략 = 현재 형식). Windows `CF_UNICODETEXT` · macOS `pbcopy` · Linux `xclip`.

```
nsql> SELECT * FROM emp;
nsql> copy markdown
3 rows copied as markdown
```

---

## 5. 명령 요약

```text
nsql plan    [-d dialect] <script|-> [args]
nsql run     -c <target> [-d dialect] [-f grid|csv|tsv|json|jsonl] [--no-prompt]
             [--timing] [--log] [--max-rows N] <script|-> [args]
nsql shell   -c <target> [-d dialect]
nsql export  -c <target> (-q <sql> | -t <table>) [-f fmt] [-o file]
nsql explain -c <target> (-q <sql> | <file>)
nsql cat     -c <target> [-s schema] [-f fmt] schemas | kinds | <kind>
                                              | columns <object> | source <kind> <name> | errors <name>
nsql conn    list | add <name> [<target>] [-d dialect --host h --port n --db d --user u -p pw] [--no-prompt]
             | show <name> | rm <name> | test [<name>] | path
nsql config  list | get <key> | set <key> <value> | reset <key> | path
```

### 공통 인자

| 인자 | 뜻 |
|---|---|
| `-c, --connect <target>` | 프로필 이름 **또는** 접속 문자열 (§1) |
| `-d, --dialect <name>` | `oracle`(기본) `mssql` `postgres` `sqlite` — 문자열로 정해지면 생략 가능 |
| `--host` `--port` `--db` `-u` `-p` | 접속 문자열 대신 **필드로** 준다 (`run` `shell` `export` `explain` `cat` `conn` 모두) |
| `--no-prompt` | 배치 모드 — 비밀번호를 묻지 않고, 치환 변수는 빈 값 |
| `-f, --format <fmt>` | 출력 형식 (§4) |
| `-o, --out <file>` | 출력 파일(`export` · 없으면 표준 출력) |
| `-q, --query <sql>` | 인라인 SQL 한 문장(`export` `explain`) |
| `-t, --table <table>` | 테이블 통째로(`export`) |
| `--max-rows <n>` | 결과셋당 가져올 행 상한(`0` = 무제한 · GUI는 `grid.max_rows` = 200) |
| `--width, --linesize <n>` | 표 한 줄 최대 폭(`0` = 터미널 폭 · 파이프면 무제한) |
| `--max-col-width <n>` | 한 열 최대 폭(`0` = 무제한 · 기본 60) |
| `--overflow <mode>` | `none`(기본) `wrap` `truncate` `expanded` |
| `-x, --expanded` | 레코드 보기(psql `\x`) |
| `--timing` | 단계별 소요 시간 → stderr ([26](26-performance-architecture.md)) |
| `--log` | 실행 로그 → stderr (형식 = `log.format`) |
| `-h, --help` · `-V, --version` | 도움말 · 판올림 |
| `<script\|->` | 스크립트 파일 또는 `-`(표준 입력) |
| `[args]` | 스크립트의 `&1 &2 …` 치환 인자 |

### 비밀번호를 찾는 순서

`-p`로 주지 않으면 이 순서로 찾는다:

1. `-p, --password <pw>`
2. **환경변수 `NSQL_PASSWORD`**
3. stdin이 터미널이면 **숨김 입력**으로 묻는다
4. `--no-prompt`면 묻지 않고 실패

```sh
export NSQL_PASSWORD='s3cr3t'
nsql run -c "postgres://app@dbhost:5432/appdb" --no-prompt migrate.sql
```

### 도움말

```sh
nsql --help              # 개요 · 명령 목록 · 접속 문자열 · 공통 옵션 · 이 빌드의 드라이버
nsql <명령> --help       # 한 명령의 인자·옵션·예
nsql help <명령>         # 같은 내용
```

### 스크립트에서 쓰는 것

- `&1`~`&n` — 명령줄 인자 치환
- `WHENEVER SQLERROR EXIT` — 오류 시 종료(종료 코드로 CI 판정)
- `CONNECT <프로필이름>` — 스크립트 안에서 접속 전환
- `EXEC :V := 'x'` · `SELECT … INTO :V` · `PRINT` — 세션 변수([08](08-session-variables.md))

---

## 6. 문제 해결

| 증상 | 원인·조치 |
|---|---|
| `DPI-1047` | Oracle Instant Client 없음 → `NSQL_ORACLE_CLIENT_DIR` 지정 |
| `접속 실패: error connecting to server` | 호스트·포트·방화벽 확인. `nsql conn test`로 접속만 따로 시험 |
| `비밀번호 봉투를 열 수 없습니다` | 다른 Windows 계정·다른 PC의 프로필이다 → `nsql conn add <이름> …`로 다시 저장 |
| 프로필 이름을 접속 문자열로 오해 | 이름은 `[A-Za-z0-9_.-]{1,64}`이고 `:`·`@`·`/`가 없다. 이름 꼴인데 프로필이 없으면 **오류로 알려 준다**(오타를 접속 문자열로 착각하지 않게) |
| 결과가 잘림 | `--max-rows N`을 올린다(기본 상한은 `grid.max_rows` 설정) |
| 한국어로 보고 싶다 | `nsql config set ui.lang ko` |
| SQLite 경로를 못 연다 | Windows 경로를 그대로 쓴다(`sqlite:C:\data\app.db`). Git Bash의 `/c/...` 꼴은 Windows 바이너리가 못 읽는다 |

---

## 7. 관련 문서

| 무엇 | 문서 |
|---|---|
| CLI 설계 제안·단계 | [11 CLI 도구](11-cli.md) |
| 인자 규약·타 CLI 조사 | [27 CLI 인자 규약](27-cli-conventions.md) |
| 연결 프로필 내부(암호화·공유) | [21 연결 프로필](21-connection-profiles.md) |
| 세션 변수 엔진 | [08 세션 변수](08-session-variables.md) |
| **결과 → SQL 키 규칙**(`-f sql:*`) | **[41 SQL 복사 키 규칙](41-sql-copy-key-rules.md)** |
| 설정 체계 | [24 설정·VS Code 분석](24-settings-and-vscode-analysis.md) |
| 성능 계측(`--timing`) | [26 성능 아키텍처](26-performance-architecture.md) |
| 서버 메시지·로그(`--log`) | [32 서버 메시지·실행 중 로그](32-server-messages-and-live-log.md) |
| 실서버 테스트 환경 | [20 Codespaces·Docker](20-testing-codespaces.md) |
