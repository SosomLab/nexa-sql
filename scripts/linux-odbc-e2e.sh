#!/usr/bin/env bash
# Linux ODBC 어댑터 E2E(10-24 · 맥 10-24 §1~2 검증을 Linux에서 자동으로 · 전제 = scripts/linux-odbc-setup.sh).
#   ① 등록 드라이버 3(SQLite3 · PostgreSQL Unicode · ODBC Driver 18 for SQL Server)
#   ② CLI SQLite3 ODBC 왕복(CREATE · 한글 INSERT · SELECT 2행) — LANG 있음/없음 둘 다(ensure_utf8_ctype · C 로캘에서 한글 깨짐 결함 재발 감시)
#   ③ PostgreSQL ODBC 로그인 단계 도달(틀린 비밀번호 → 인증 실패 메시지 · 네트워크·드라이버 정상 판정) ④ SQL Server ODBC 같은 꼴(18456/Login failed · 한글 메시지)
#   ⑤ GUI 격리 홈 = 접속 문자열 인자로 SQLite3 ODBC 접속 → run.all(자동 커밋) → grid rows=1 + DB 실제 값(10-24 §2 ④-a = 자동 커밋 멈춤 결함 회귀)
#   ⑥ GUI 수동 커밋 = run.all → run.commit → DB 값(§2 ④-b)
# 사용: scripts/linux-odbc-e2e.sh -o <출력폴더> [-n target/debug/nsql] [-e target/debug/nexa-sql] [--pg host:port/db] [--ms host,port]
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/timeout.sh"
OUT=""; NSQL="$ROOT/target/debug/nsql"; EXE="$ROOT/target/debug/nexa-sql"; PG="192.168.0.60:5433/matrixdb2"; MS="192.168.0.58,1433"
while [ $# -gt 0 ]; do case "$1" in -o) OUT=$2; shift 2;; -n) NSQL=$2; shift 2;; -e) EXE=$2; shift 2;; --pg) PG=$2; shift 2;; --ms) MS=$2; shift 2;; *) echo "usage: $0 -o <out> [-n nsql] [-e gui] [--pg host:port/db] [--ms host,port]"; exit 2;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir>"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/odbc-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && printf '%s\n' "$2" | head -6 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
skp() { say "  SKIP  $1"; skip=$((skip+1)); }

say "① 등록 드라이버"
DRV=$(odbcinst -q -d 2>/dev/null)
for d in "SQLite3" "PostgreSQL Unicode" "ODBC Driver 18 for SQL Server"; do
  printf '%s\n' "$DRV" | grep -qx "\[$d\]" && ok "odbcinst [$d]" || bad "odbcinst [$d] 없음(setup 전?)" "$DRV"
done

say "② CLI SQLite3 ODBC 왕복(LANG 있음 → 없음)"
DB="$OUT/odbc-rt.sqlite"; rm -f "$DB"
printf 'CREATE TABLE t_odbc(id INTEGER, name TEXT);\nINSERT INTO t_odbc VALUES (1, %s);\nINSERT INTO t_odbc VALUES (2, %s);\nSELECT id, name FROM t_odbc ORDER BY id;\n' "'한글'" "'가나다'" > "$OUT/rt.sql"
for mode in lang nolang; do
  rm -f "$DB"
  if [ $mode = lang ]; then LANG=ko_KR.UTF-8 "$NSQL" run -c "odbc:Driver={SQLite3};Database=$DB" "$OUT/rt.sql" > "$OUT/rt-$mode.out" 2>&1; rc=$?
  else env -u LANG -u LC_ALL -u LC_CTYPE "$NSQL" run -c "odbc:Driver={SQLite3};Database=$DB" "$OUT/rt.sql" > "$OUT/rt-$mode.out" 2>&1; rc=$?; fi
  o=$(cat "$OUT/rt-$mode.out")
  if [ $rc = 0 ] && printf '%s' "$o" | grep -q '한글' && printf '%s' "$o" | grep -q '가나다' && printf '%s' "$o" | grep -qE '2 rows?|2행'; then ok "왕복($mode) 한글 2행(영/한 출력 모두 허용)"; else bad "왕복($mode) rc=$rc" "$o"; fi
done
# DB 파일을 SQLite 내장 드라이버로 다시 읽어 ODBC가 실제로 썼는지 확인
printf 'SELECT count(*) AS n FROM t_odbc;\n' > "$OUT/cnt.sql"
o=$("$NSQL" run -c "sqlite:$DB" "$OUT/cnt.sql" 2>&1); printf '%s' "$o" | grep -qE '(^|[^0-9])2([^0-9]|$)' && ok "ODBC가 쓴 DB를 내장 SQLite로 읽음 = 2행" || bad "DB 재확인" "$o"

say "③ PostgreSQL ODBC 로그인 단계(틀린 비밀번호 = 인증 메시지)"
host=${PG%%:*}; rest=${PG#*:}; port=${rest%%/*}; db=${rest#*/}
if timeout 4 bash -c "exec 3<>/dev/tcp/$host/$port" 2>/dev/null; then
  printf 'SELECT 1;\n' > "$OUT/one.sql"
  o=$(printf 'wrong-password-e2e\n' | "$NSQL" run -c "odbc:Driver={PostgreSQL Unicode};Server=$host;Port=$port;Database=$db;UID=postgres" --password-stdin "$OUT/one.sql" 2>&1)
  printf '%s' "$o" | grep -qiE 'password authentication failed|28P01|authentication' && ok "PG ODBC 인증 단계 도달($host:$port)" || bad "PG ODBC 메시지 예상 밖" "$o"
else skp "PG $host:$port 미도달"; fi

say "④ SQL Server ODBC 로그인 단계(틀린 비밀번호 = 18456 · 한글 메시지 깨짐 없음)"
mh=${MS%%,*}; mp=${MS#*,}
if timeout 4 bash -c "exec 3<>/dev/tcp/$mh/$mp" 2>/dev/null; then
  for mode in lang nolang; do
    if [ $mode = lang ]; then o=$(printf 'wrong-password-e2e\n' | LANG=ko_KR.UTF-8 "$NSQL" run -c "odbc:Driver={ODBC Driver 18 for SQL Server};Server=$MS;Database=M4PLAN_MS;TrustServerCertificate=yes;UID=BISCM_MS" --password-stdin "$OUT/one.sql" 2>&1)
    else o=$(printf 'wrong-password-e2e\n' | env -u LANG -u LC_ALL -u LC_CTYPE "$NSQL" run -c "odbc:Driver={ODBC Driver 18 for SQL Server};Server=$MS;Database=M4PLAN_MS;TrustServerCertificate=yes;UID=BISCM_MS" --password-stdin "$OUT/one.sql" 2>&1); fi
    printf '%s\n' "$o" > "$OUT/ms-$mode.out"
    if printf '%s' "$o" | grep -qE '18456|Login failed|로그인하지 못했습니다'; then
      if printf '%s' "$o" | grep -q '로그인하지 못했습니다'; then
        printf '%s' "$o" | grep -q "'BISCM_MS'" && ok "MSSQL ODBC($mode) 18456 + 한글 메시지 온전" || bad "MSSQL ODBC($mode) 한글 메시지 안 사용자 이름 깨짐(LC_CTYPE?)" "$o"
      else ok "MSSQL ODBC($mode) 18456(영어 메시지)"; fi
    else bad "MSSQL ODBC($mode) 예상 밖" "$o"; fi
  done
else skp "SQL Server $MS 미도달"; fi

say "⑤⑥ GUI 격리 홈 SQLite3 ODBC(자동 커밋 run.all → grid · 수동 커밋 run.commit)"
if [ -x "$EXE" ]; then
  H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
  gui_case() { # $1 이름 · $2 settings 추가 줄 · $3 기동 명령 꼬리
    local name=$1 extra=$2 tail=$3
    local gdb="$OUT/$name.sqlite"; rm -f "$gdb"
    printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1100,760\n%s' "$extra" > "$H/settings.conf"
    printf 'CREATE TABLE g(id INTEGER, v TEXT);\nINSERT INTO g VALUES (1, %s);\nSELECT id, v FROM g;\n' "'한글'" > "$OUT/$name.sql"
    local cmd="@connected:open:$OUT/$name.sql,@after:4000:run.all,@after:9000:grid.dump:$OUT/$name.grid${tail},@after:12500:output.dump:$OUT/$name.out"
    NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 16 "$EXE" "odbc:Driver={SQLite3};Database=$gdb" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
    echo "$gdb"
  }
  gdb=$(gui_case g5 "" "")
  if grep -q 'rows=1' "$OUT/g5.grid" 2>/dev/null; then ok "⑤ GUI 자동 커밋 run.all → grid rows=1(멈춤 없음)"; else bad "⑤ GUI grid rows=1 아님(멈춤 회귀?)" "$(cat "$OUT/g5.grid" 2>/dev/null; tail -3 "$OUT/g5.out" 2>/dev/null)"; fi
  printf 'SELECT count(*) AS n FROM g;\n' > "$OUT/cntg.sql"; o=$("$NSQL" run -c "sqlite:$gdb" "$OUT/cntg.sql" 2>&1)
  printf '%s' "$o" | grep -qE '(^|[^0-9])1([^0-9]|$)' && ok "⑤ DB 실제 값 1행(자동 커밋 반영)" || bad "⑤ DB 값" "$o"
  gdb=$(gui_case g6 $'session.autocommit=off\n' ",@after:10000:run.commit")
  o=$("$NSQL" run -c "sqlite:$gdb" "$OUT/cntg.sql" 2>&1)
  printf '%s' "$o" | grep -qE '(^|[^0-9])1([^0-9]|$)' && ok "⑥ 수동 커밋 run.commit 뒤 DB 1행" || bad "⑥ 수동 커밋 뒤 DB 값" "$o"
  pgrep -f "NSQL_HOME=$H" >/dev/null 2>&1 && pkill -f "$H" 2>/dev/null || true
else skp "GUI 실행 파일 없음($EXE)"; fi

say "결과: PASS $pass · FAIL $fail · SKIP $skip · 보고 = $REPORT"
[ $fail = 0 ]
