# 115 · 운영기(PRD) 점검 시나리오 — `nsql` 명령 사용법

> **요구(사용자 10-16)**: "`prod-mode-scenario.sql`을 테스트해 볼 수 있는 nsql 명령어 사용 방법을 정리한 문서"
> **대상 파일** = [`examples/prod-mode-scenario.sql`](../examples/prod-mode-scenario.sql)(단계별 쿼리 + 확인 포인트) · [`scripts/prod-mode-scenario.sh`](../scripts/prod-mode-scenario.sh)(같은 단계를 자동으로 돌리는 스크립트)
> **설계 원문** = [114](114-prod-held-commit-and-safeguards.md)(운영 보류 커밋 §5 · 트랜잭션 경계 가드 §5-2 · Commit 표현 §5-3 · 객체 백업 §7 · 시나리오 §11) · 결정 D-295~D-308

---

## 0. 먼저 알아 둘 것 — `nsql`(CLI)은 운영 보류를 하지 않는다

| 기능 | GUI(nexa-sql) | CLI(`nsql run`) |
|---|---|---|
| 운영 보류 커밋(변경 = Commit 2단을 눌러야 반영) | ✅ | **❌ 바로 커밋**(D-308 · 실측 10-16: PRD 프로필로 `UPDATE` → 즉시 반영) |
| 운영 2단 재실행(DDL은 3초 안 다시 실행) | ✅ | ❌ |
| 객체 백업(CREATE OR REPLACE·DROP 전 DDL 파일) | ✅ | ❌ |
| 스크립트 안 `COMMIT` | 직전 멈춤 → 확인 | 그대로 커밋 |
| 보류 중 `COMMIT;` 문만 실행 | **거부 경고**(서버로 안 감 · 상단 Commit 2단 또는 카드 [커밋]으로 · D-308) | 그대로 커밋 |
| `SET CONNTYPE x` | 이 탭 세션의 임시 유형 | 안내 문구만(저장 안 함 · `nsql conn env`를 쓰라고 알림) |
| 서버 유형 영속 지정 | 프로필 폼 | **`nsql conn env <프로필> prd`** |

> ⚠ **`examples/prod-mode-scenario.sql` 전체를 `nsql run`으로 운영 프로필에 돌리지 않는다.** CLI는 보류 없이 끝까지 실행해 UPDATE를 커밋하고 9단계의 DROP까지 실행한다. 보류·2단·백업은 **GUI에서** 확인하고, `nsql`은 아래처럼 **준비 · 유형 전환 · 다른 세션 확인 조회 · 정리 · 자동 시나리오 실행**에 쓴다.

---

## 1. 격리 홈 만들기(실제 프로필 보호)

실제 설정 폴더(`%APPDATA%\nexa-sql`)의 프로필 유형을 바꾸지 않도록 **사본 홈**을 만들고 `NSQL_HOME`으로 가리킨다. GUI와 CLI가 같은 `NSQL_HOME`을 보면 같은 프로필·설정을 쓴다.

**Git Bash**
```bash
cd /d/Projects/kiros33/nexa-sql
export NSQL_HOME="$(cygpath -m "$PWD/target/prod-manual/home")"
mkdir -p "$NSQL_HOME"
cp -R "$APPDATA/nexa-sql/profiles" "$NSQL_HOME/"; cp "$APPDATA/nexa-sql/device.key" "$NSQL_HOME/"
printf 'run.prod_confirm=on\n' > "$NSQL_HOME/settings.conf"
NSQL=./target/release/nsql.exe          # 또는 설치본 nsql
```

**PowerShell**
```powershell
cd D:\Projects\kiros33\nexa-sql
$env:NSQL_HOME = "$PWD\target\prod-manual\home"
New-Item -ItemType Directory -Force $env:NSQL_HOME | Out-Null
Copy-Item -Recurse -Force "$env:APPDATA\nexa-sql\profiles" $env:NSQL_HOME
Copy-Item -Force "$env:APPDATA\nexa-sql\device.key" $env:NSQL_HOME
Set-Content "$env:NSQL_HOME\settings.conf" "run.prod_confirm=on`n"
$NSQL = ".\target\release\nsql.exe"
```

확인:
```bash
$NSQL conn path            # → <격리 홈>/profiles  (실제 폴더가 아니면 OK)
$NSQL conn list            # 프로필 목록(BISCM 등)
```

---

## 2. 단계별 명령

아래 `BISCM`은 프로필 이름 예(Oracle). 다른 프로필이면 바꿔 쓴다.

| 단계 | `nsql`로 하는 일 | 명령 | 확인 |
|---|---|---|---|
| 미리 보기 | 파일을 접속 없이 해석(문장 25개 · 종류) | `$NSQL plan -d oracle examples/prod-mode-scenario.sql` | `# dialect=oracle items=25` · 각 문장 `Sql::Ddl/Dml` |
| ① 유형 = 운영 | 사본 프로필을 PRD로 | `$NSQL conn env BISCM prd` → `$NSQL conn env BISCM` | `BISCM: Production (PRD)` |
| 0 준비 | 임시 표·프로시저 생성(운영 전환 **전**이나 비운영 프로필로 해도 됨) | 아래 §2-1의 `prep.sql`을 `$NSQL run -c BISCM --no-prompt prep.sql` | `PROCEDURE … compiled · no errors` |
| GUI 실행 | 같은 격리 홈으로 GUI를 열고 시나리오 파일을 연다 | Git Bash: `NSQL_HOME="$NSQL_HOME" ./target/release/nexa-sql.exe BISCM` · PowerShell: `& .\target\release\nexa-sql.exe BISCM` | 상태줄 PRD 칩 |
| 1~8 | **GUI에서** 파일의 단계 블록을 선택해 실행(Ctrl+Enter = 문장 · F5 = 전체/선택) | — | 파일의 각 "확인:" 주석 · 114 §5 |
| 3·4·5 확인 | **다른 세션**에서 값 조회(보류 중이면 옛 값이 보여야 함) | `echo "SELECT ID, V FROM NSQLT_HOLD_T ORDER BY ID;" \| $NSQL run -c BISCM --no-prompt -f csv -` | 3단계 보류 중 = `1,o1` → 커밋 뒤 `1,x1` · 4단계 롤백 뒤 `2,o2` · 5단계 최종 `3,b3` |
| 2·7 확인 | 객체 백업 파일(17자리 시각 확장자) | `ls "$NSQL_HOME/backups/objects"` · PowerShell `Get-ChildItem "$env:NSQL_HOME\backups\objects"` | `BISCM_BISCM.NSQLT_HOLD_PRC_PROCEDURE.<yyyyMMddHHmmssSSS>` |
| 7 확인 | 삭제한 프로시저가 서버에서 사라졌는지 | `$NSQL cat -c BISCM procs` | `NSQLT_HOLD_PRC` 없음 |
| 8 준비(옵션) | 보류 한도 1분으로 줄이기(격리 홈 설정) | `$NSQL config set tx.prod_idle_limit_min 1` · `$NSQL config set tx.idle_countdown_secs 5` | GUI 재시작 뒤 보류 1분 → 카운트다운 → 자동 롤백 |
| 9 정리 | 임시 객체 삭제 · 유형 되돌림 | §2-2의 `cleanup.sql`을 `$NSQL run -c BISCM --no-prompt cleanup.sql` → `$NSQL conn env BISCM none` | 객체 없음 · `BISCM: None (NON)` |

### 2-1. `prep.sql`(0단계만 · 파일에서 떼어 씀)

```sql
CREATE TABLE NSQLT_HOLD_T (ID NUMBER PRIMARY KEY, V VARCHAR2(20));
INSERT INTO NSQLT_HOLD_T VALUES (1, 'o1');
INSERT INTO NSQLT_HOLD_T VALUES (2, 'o2');
INSERT INTO NSQLT_HOLD_T VALUES (3, 'o3');
COMMIT;
CREATE OR REPLACE PROCEDURE NSQLT_HOLD_PRC AS BEGIN NULL; END;
/
```

### 2-2. `cleanup.sql`(9단계)

```sql
DROP TABLE NSQLT_HOLD_T;
DROP PROCEDURE NSQLT_HOLD_PRC;
DROP TABLE NSQLT_HOLD_T2;
```
(없는 객체의 DROP 오류는 무시해도 된다 · 종료 코드 1 = 하나 이상 실패)

> 다른 세션 조회 팁: 보류는 **GUI 세션 안의 미커밋 트랜잭션**이라, CLI는 늘 다른 세션이다. GUI에서 보류 중 CLI로 조회하면 커밋 전 값이 보여야 정상이다(Oracle 읽기 일관성).

---

## 3. 자동으로 돌리기 — `scripts/prod-mode-scenario.sh`

같은 단계를 GUI 기동 명령으로 자동 실행하고 PASS/FAIL을 낸다(실제 프로필은 사본만 바꿈 · 임시 객체는 끝에 정리).

| 명령 | 뜻 |
|---|---|
| `bash scripts/prod-mode-scenario.sh -P "$APPDATA/nexa-sql"` | Oracle BISCM(기본) · 0~7·9단계 |
| `… -P "$APPDATA/nexa-sql" -c <프로필>` | 다른 Oracle 프로필 |
| `… -P "$APPDATA/nexa-sql" -i -k` | 단계마다 멈춤(Enter = 다음 · q = 종료) + GUI를 띄운 채 직접 보기 |
| `… -P "$APPDATA/nexa-sql" -s 3` | 3단계(보류 → Commit 2단)만(0·9는 늘) |
| `… -P "$APPDATA/nexa-sql" -L -s 8` | 8단계(보류 한도 1분 → 자동 롤백 · 약 90 초) |
| `bash scripts/prod-mode-scenario.sh -d sqlite -c Local` | 서버 없이 로컬 SQLite 파일 DB로(프로필을 스크립트가 만듦 · `-P` 불필요) |
| `-o <폴더>` | 산출 폴더(기본 `target/prod-scenario`) · 단계 산출(`s<n>*.tx/.out/.toast/.dump`)과 보고 `prod-scenario.txt` |

- 실행 파일 = 기본 `target/debug/nexa-sql(.exe)` · `target/debug/nsql(.exe)` → Release로 하려면 `-g target/release/nexa-sql.exe -n target/release/nsql.exe`.
- 10-15~16 실측: BISCM 26/0 · `-L -s 8` 5/0(SQLite·BISCM).

---

## 4. SQLite로 따라 하기(서버 없이)

```bash
export NSQL_HOME="$(cygpath -m "$PWD/target/prod-sqlite/home")"; mkdir -p "$NSQL_HOME"
$NSQL conn add Local "sqlite:$NSQL_HOME/local.sqlite" -d sqlite --no-prompt
$NSQL conn env Local prd
printf "CREATE TABLE NSQLT_HOLD_T (ID INTEGER PRIMARY KEY, V TEXT);\nINSERT INTO NSQLT_HOLD_T VALUES (1,'o1'),(2,'o2'),(3,'o3');\nCREATE VIEW NSQLT_HOLD_V AS SELECT ID FROM NSQLT_HOLD_T;\n" > prep-sqlite.sql
$NSQL run -c Local --no-prompt prep-sqlite.sql
./target/release/nexa-sql.exe Local      # GUI · 파일 끝 "SQLite 판" 블록대로
```
- SQLite 판 차이: 2단계 백업 대상 = `DROP VIEW NSQLT_HOLD_V` · 7단계 = 탐색기 Views ▸ `NSQLT_HOLD_V` 삭제 · 문장 끝 `;`.

---

## 5. 문제가 생기면

| 증상 | 확인 |
|---|---|
| CLI로 돌렸는데 보류가 안 됨 | 정상(§0 · CLI는 보류 정책 없음) |
| GUI에서 보류가 안 됨 | `$NSQL conn env <프로필>`이 PRD인지 · 설정 `tx.prod_hold`(기본 on) · 상태줄 PRD 칩 |
| 백업 파일이 없음 | 설정 Object backup ▸ `backup.object_*` · 새 객체(아직 없음)는 백업 생략 · Output "Object backup: …" 줄 |
| 보류 한도 롤백이 안 옴 | 격리 홈 `settings.conf`의 `tx.prod_idle_limit_min` · GUI 재시작 · 기준 = 가장 오래된 미커밋 문장 뒤 경과(D-307) |
| 실제 프로필 유형이 바뀌었는지 걱정 | `NSQL_HOME` 없이 `nsql conn env <프로필>` → 원래 값이면 OK |
