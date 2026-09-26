#!/usr/bin/env bash
# mac-bulk-e2e.sh — 대량 적재(`nsql import`) E2E(docs/89 · T-236 · 09-26). CLI만(키 주입 0) · 격리 홈 · SQLite ①~③ + 실서버(선택) 임시 표 NSQLT_BULK.
#   ① 1,000행 csv 적재 → 건수·값 ② 중복 PK로 500번째 행 실패 → "row 500 (line 501)" 지목 · 그 앞 499행 커밋 ③ tsv + --no-header --cols + --map
#   -P <실제 설정 폴더> -d 프로필:방언,… = 실서버(임시 표 생성 → 5,000행 적재 → 건수 → DROP · 61 §2-4 ⑤) · rows/s 출력(89 실측).
# 사용: scripts/mac-bulk-e2e.sh [-n target/debug/nsql] [-H <home>] [-P <실제 설정 폴더>] [-d BISCM:oracle,Repository:postgres,M4PLAN:mssql]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/debug/nsql"; APP="$ROOT/target/debug/nexa-sql"; H="${TMPDIR:-/tmp}/nsql-bulk-e2e"; PROF=""; DBMS="${NSQL_E2E_DBMS:-}"
while getopts "n:e:H:P:d:" o; do case $o in n) NSQL=$OPTARG;; e) APP=$OPTARG;; H) H=$OPTARG;; P) PROF=$OPTARG;; d) DBMS=$OPTARG;; esac; done
D="$H/data"; O="$H/out"; rm -rf "$H"; mkdir -p "$D" "$O"
if [ -n "$PROF" ] && [ -d "$PROF/profiles" ]; then cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null; fi
fail=0; pass=0
ok() { echo "PASS  $1"; pass=$((pass+1)); }
bad() { echo "FAIL  $1"; fail=$((fail+1)); }
cli() { NSQL_HOME="$H" "$NSQL" run -c "$1" "$2" --no-prompt 2>&1; }
imp() { NSQL_HOME="$H" "$NSQL" import -c "$@" --no-prompt 2>&1; }
expect_grep() { if echo "$2" | grep -q -- "$3"; then ok "$1"; else bad "$1 (기대 '$3')"; echo "$2" | head -6 | sed 's/^/      /'; fi; }
run_gui() { # <초> <인자> <기동 명령> — GUI Import 창(89 §3-3 · 키 주입 0 · NSQL_NO_ACTIVATE)
  local secs=$1 arg=$2 cmd=$3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$APP" "$arg" >/dev/null 2>>"$O/stderr.txt" &
  local p=$!; sleep "$secs"; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
}
# ── SQLite
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
python3 - "$D" <<'PY'
import sys, os
d=sys.argv[1]
with open(os.path.join(d,'ok.csv'),'w') as f:
    f.write('id,name,amt,dt\n')
    for i in range(1,1001): f.write(f'{i},"n, {i}",{i*1.5},2026-09-{(i%28)+1:02d} 10:00:00\n')
with open(os.path.join(d,'dup.csv'),'w') as f:
    f.write('id,name,amt,dt\n')
    for i in range(1,1001):
        pk = 1 if i == 500 else 2000+i   # 500번째 행 = 이미 있는 1과 충돌
        f.write(f'{pk},x,1,2026-09-01 00:00:00\n')
with open(os.path.join(d,'nohdr.tsv'),'w') as f:
    for i in range(1,11): f.write(f'{5000+i}\tt{i}\t{i}\t2026-01-0{(i%9)+1} 09:00:00\n')
with open(os.path.join(d,'mapped.csv'),'w') as f:
    f.write('code,label\n'); f.write('7001,seven\n7002,\n')
PY
cat > "$D/setup.sql" <<'SQL'
DROP TABLE IF EXISTS bulk_t;
CREATE TABLE bulk_t (id INTEGER PRIMARY KEY, name TEXT, amt REAL, dt TEXT);
SQL
printf 'SELECT COUNT(*) AS n, MAX(id) AS mx, SUM(amt) AS s FROM bulk_t;\n' > "$D/chk.sql"
printf "SELECT name FROM bulk_t WHERE id = 7;\n" > "$D/chk7.sql"
cli Local "$D/setup.sql" >/dev/null
echo "=== SQLite ① 1,000행 csv(따옴표·쉼표 포함) 적재"
o=$(imp Local -t bulk_t "$D/ok.csv"); v=$(cli Local "$D/chk.sql")
expect_grep "적재 보고 1000행" "$o" "1000 rows imported"
expect_grep "서버 건수 1000 · 최대 id 1000" "$v" "1000  1000"
expect_grep "따옴표 필드 그대로(n, 7)" "$(cli Local "$D/chk7.sql")" "n, 7"
echo "=== SQLite ② 500번째 행 중복 PK = 행·줄 지목 · 앞 499행 커밋"
o=$(imp Local -t bulk_t --batch 500 "$D/dup.csv"); v=$(cli Local "$D/chk.sql")
expect_grep "오류 지목 = row 500 (line 501)" "$o" "row 500 (line 501)"
expect_grep "앞 499행 커밋 보고" "$o" "499 rows committed"
expect_grep "서버 건수 1499" "$v" "1499"
echo "=== SQLite ③ tsv + --no-header + --map"
o=$(imp Local -t bulk_t -f tsv --no-header "$D/nohdr.tsv"); expect_grep "tsv 헤더 없음 10행" "$o" "10 rows imported"
o=$(imp Local -t bulk_t --map code=id,label=name --empty-null on "$D/mapped.csv"); expect_grep "--map 2행" "$o" "2 rows imported"
printf "SELECT name IS NULL AS nn FROM bulk_t WHERE id = 7002;\n" > "$D/chknull.sql"; expect_grep "빈 필드 = NULL" "$(cli Local "$D/chknull.sql")" "^1$\|^ *1$"
o=$(imp Local -t bulk_t --map code=id,label=nope "$D/mapped.csv"); expect_grep "없는 열 = 안내" "$o" "not found"
echo "=== SQLite ④ JSON Lines(키 → 열 · 빠진 키 = NULL · 유니코드)"
printf '{"id": 8001, "name": "j\\u0031", "amt": 2.5, "dt": "2026-02-02 02:02:02"}\n{"id": 8002, "amt": null}\n' > "$D/rows.jsonl"
o=$(imp Local -t bulk_t "$D/rows.jsonl"); expect_grep "jsonl 2행" "$o" "2 rows imported"
printf "SELECT name, amt IS NULL AS an FROM bulk_t WHERE id IN (8001, 8002) ORDER BY id;\n" > "$D/chkj.sql"; v=$(cli Local "$D/chkj.sql")
expect_grep "jsonl 유니코드 이스케이프 j1" "$v" "j1"; expect_grep "jsonl 빠진 키/null = NULL" "$v" " 1$"
echo "=== SQLite ⑤ GUI Import 창(import.open → start → dump · 성공 · 실패 지목 · 89 §3-3)"
if [ -x "$APP" ]; then
  python3 - "$D" <<'PYG'
import sys, os
d=sys.argv[1]
with open(os.path.join(d,'gui.csv'),'w') as f:
    f.write('id,name,amt,dt\n')
    for i in range(1,301): f.write(f'{9000+i},g{i},{i},2026-03-01 00:00:00\n')
with open(os.path.join(d,'guidup.csv'),'w') as f:
    f.write('id,name,amt,dt\n')
    for i in range(1,21): f.write(f'{(9001 if i==15 else 9500+i)},d{i},1,2026-03-01 00:00:00\n')
PYG
  run_gui 14 Local "import.open:bulk_t;$D/gui.csv,@after:2500:import.start,@after:8000:import.dump:$O/imp1.txt"
  d1=$(cat "$O/imp1.txt" 2>/dev/null)
  expect_grep "창 상태 = 끝(running=false) · 성공 300행" "$d1" "running=false.*report=ok rows=300"
  printf 'SELECT COUNT(*) AS n FROM bulk_t WHERE id BETWEEN 9001 AND 9300;\n' > "$D/chkgui.sql"
  expect_grep "서버 건수 300" "$(cli Local "$D/chkgui.sql")" "300"
  run_gui 14 Local "import.open:bulk_t;$D/guidup.csv,@after:2500:import.start,@after:8000:import.dump:$O/imp2.txt"
  d2=$(cat "$O/imp2.txt" 2>/dev/null)
  expect_grep "중복 PK = 행 15(줄 16) 지목 · 앞 14행 커밋" "$d2" "report=failed row=15 line=16 rows=14"
  expect_grep "결과 줄 = 오류 표시(err:)" "$d2" "result=Some(\"err:"
else
  echo "SKIP  GUI 바이너리 없음($APP)"
fi
# ── 실서버
bulk_suite() {
  local P=$1 dl=$2 ddl
  case $dl in
    oracle)   ddl='CREATE TABLE NSQLT_BULK (ID NUMBER PRIMARY KEY, NAME VARCHAR2(30), AMT NUMBER(12,2), DT DATE)';;
    postgres) ddl='CREATE TABLE NSQLT_BULK (ID integer PRIMARY KEY, NAME varchar(30), AMT numeric(12,2), DT timestamp)';;
    mssql)    ddl='CREATE TABLE NSQLT_BULK (ID int PRIMARY KEY, NAME nvarchar(30), AMT decimal(12,2), DT datetime2(0))';;
  esac
  printf 'DROP TABLE NSQLT_BULK;\n' > "$D/${P}_drop.sql"; printf '%s;\n' "$ddl" > "$D/${P}_ddl.sql"
  printf 'SELECT COUNT(*) AS N, MAX(ID) AS MX FROM NSQLT_BULK;\n' > "$D/${P}_chk.sql"
  python3 -c "
with open('$D/${P}_5k.csv','w') as f:
    f.write('ID,NAME,AMT,DT\n')
    for i in range(1,5001): f.write(f'{i},name {i},{i*1.25:.2f},2026-09-{(i%28)+1:02d} 12:34:56\n')
"
  cli "$P" "$D/${P}_drop.sql" >/dev/null 2>&1; cli "$P" "$D/${P}_ddl.sql" >/dev/null 2>&1
  echo "=== $P($dl) 5,000행 적재"
  local o v; o=$(imp "$P" -t NSQLT_BULK "$D/${P}_5k.csv"); v=$(cli "$P" "$D/${P}_chk.sql")
  expect_grep "$P 적재 보고 5000행" "$o" "5000 rows imported"; echo "      $(echo "$o" | grep 'rows imported')"
  case $dl in mssql) expect_grep "$P 경로 = TDS bulk" "$o" "driver:tdsbulk";; postgres) expect_grep "$P 경로 = COPY" "$o" "driver:copyin";; oracle) expect_grep "$P 경로 = 배열 DML" "$o" "driver:arraydml";; esac
  expect_grep "$P 서버 건수 5000" "$v" "5000"
  if [ "$dl" = postgres ]; then
    NSQL_HOME="$H" "$NSQL" export -c "$P" -t NSQLT_BULK -f csv --fast -o "$D/${P}_out.csv" --no-prompt >/dev/null 2>&1
    expect_grep "$P export --fast(COPY TO) = 헤더 + 5,000줄" "$(wc -l < "$D/${P}_out.csv" | tr -d ' ')" "^5001$"
    expect_grep "$P export --fast 헤더" "$(head -1 "$D/${P}_out.csv")" "id,name,amt,dt"
  fi
  cli "$P" "$D/${P}_drop.sql" >/dev/null 2>&1
}
if [ -n "$DBMS" ]; then IFS=',' read -ra PAIRS <<< "$DBMS"; for pr in "${PAIRS[@]}"; do bulk_suite "${pr%%:*}" "${pr##*:}"; done; fi
echo "=== 결과: PASS $pass · FAIL $fail"
[ "$fail" -eq 0 ]
