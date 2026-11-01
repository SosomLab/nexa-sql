#!/bin/zsh
# headless-field-e2e.sh — 실기 후보를 헤드리스로 자동 덮는 E2E(11-02 143차 · 16번 · 격리 홈 · 키 주입 0 · OS 창 0 · 3-OS 헤드리스 공통 길).
#   ⓝ″ 결과 그리드 → .sql 내보내기 = `INSERT INTO <표>`(`grid.export:<파일>` · 표 이름 = 원본 질의 FROM) · ⓛ″ `license.gates=on` + xlsx = 라이선스 창(저장 안 됨 ·
#   대조 = 게이트 끔이면 저장) · ⓞ″ 접속 둘 → 로그 창 세션 열 자동(`log.stat:<파일>`) · ⓜ″ 헤드리스 = OS 창 0(wincount) + `capture:` PNG 서명.
#   사용: scripts/headless-field-e2e.sh [-o <산출 폴더 · 기본 target/field-hl>] [-e <exe>] [-n <nsql>]  · 되풀이 = 같은 폴더 덮어쓰기(61 §2-4 디스크 규칙).
#   맥 OS 창 수 = target/func-check-tools/wincount(linux-func-check.sh가 만든다 · 없으면 ⓜ″ m3만 건너뜀). 첫 실행 11-02 = 13/0.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; APP=$ROOT/target/debug/nexa-sql; CLI=$ROOT/target/debug/nsql; T=$ROOT/target/field-hl
while getopts "o:e:n:" o; do case $o in o) T=$OPTARG;; e) APP=$OPTARG;; n) CLI=$OPTARG;; esac; done
H=$T/home; D=$T/data; O=$T/out; rm -rf "$H" "$D" "$O"; mkdir -p "$H" "$D" "$O"
sqlite3 "$H/local.sqlite" "CREATE TABLE hr_emp(id INTEGER, name TEXT); INSERT INTO hr_emp VALUES (1,'kim'),(2,'lee');"
sqlite3 "$H/local2.sqlite" "CREATE TABLE t2(x INTEGER); INSERT INTO t2 VALUES (1);"
NSQL_HOME=$H $CLI conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
NSQL_HOME=$H $CLI conn add Local2 "sqlite:$H/local2.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
printf 'SELECT * FROM hr_emp;\n' > "$D/q_emp.sql"
BASE=$'ui.lang=en\ngrid.result_tabbar_single=on\ndemo.prompted=on\nwindow.main_size=1375,945\n'
run() { # id conf cmd arg secs
  local id=$1 conf=$2 cmd=$3 arg=$4 secs=$5
  printf '%s%s' "$BASE" "$conf" > "$H/settings.conf"
  NSQL_HOME=$H NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$APP" $arg >"$O/$id.out" 2>"$O/$id.err" &
  local p=$!; sleep $((secs-3)); [ -x "$ROOT/target/func-check-tools/wincount" ] && "$ROOT/target/func-check-tools/wincount" "$p" > "$O/$id.oswins" 2>/dev/null; sleep 3; kill -0 $p 2>/dev/null && kill $p
}
pass=0; fail=0; v() { if eval "$2"; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1"; fail=$((fail+1)); fi; }
echo "== ⓝ″ 결과 → .sql = INSERT INTO hr_emp"
run n "" "open:$D/q_emp.sql,@after:1500:run.all,@after:3500:grid.export:$O/emp.sql,@after:6500:windows.dump:$O/n_wins.txt,@after:7000:file.exit" Local 10
v "n1 emp.sql 생성" '[ -s "$O/emp.sql" ]'
v "n2 본문 = INSERT INTO hr_emp" 'grep -q "INSERT INTO hr_emp" "$O/emp.sql"'
v "n3 행 2" '[ "$(grep -c "INSERT INTO" "$O/emp.sql")" = 2 ]'
echo "== ⓛ″ 게이트 켬 + xlsx → 라이선스 창(저장 안 됨)"
run l $'license.gates=on\n' "open:$D/q_emp.sql,@after:1500:run.all,@after:3500:grid.export:$O/emp.xlsx,@after:5500:license.dump:$O/l_lic.txt,@after:6000:windows.dump:$O/l_wins.txt,@after:6500:file.exit" Local 10
v "l1 xlsx 미생성" '[ ! -e "$O/emp.xlsx" ]'
v "l2 창 2(라이선스 창)" 'grep -q "wins=2" "$O/l_wins.txt"'
v "l3 라이선스 창 제목" 'grep -qi "licen" "$O/l_wins.txt"'
echo "== ⓛ″ 대조: 게이트 끔 + xlsx → 저장"
run l0 "" "open:$D/q_emp.sql,@after:1500:run.all,@after:3500:grid.export:$O/emp0.xlsx,@after:6500:windows.dump:$O/l0_wins.txt,@after:7000:file.exit" Local 10
v "l0 xlsx 생성(게이트 끔)" '[ -s "$O/emp0.xlsx" ]'
v "l0 창 1" 'grep -q "wins=1" "$O/l0_wins.txt"'
echo "== ⓞ″ 접속 둘 → 로그 창 세션 열 자동"
run o "" "open:$D/q_emp.sql,@after:1500:log.stat:$O/o_stat1.txt,@after:2000:connect:Local2,@after:5000:run.all,@after:6500:log.stat:$O/o_stat2.txt,@after:7000:file.exit" Local 10
v "o1 접속 1 = 세션 열 끔" 'grep -q "sessions_seen=1 session_col=false" "$O/o_stat1.txt"'
v "o2 접속 2 = 세션 열 자동 켬" 'grep -q "sessions_seen=2 session_col=true" "$O/o_stat2.txt"'
echo "== ⓜ″ mac 헤드리스 = OS 창 0 + capture PNG"
run m "" "open:$D/q_emp.sql,@after:1500:run.all,@after:3500:capture:$O/m.png,@after:5000:windows.dump:$O/m_wins.txt,@after:5500:file.exit" Local 9
v "m1 PNG 서명" '[ "$(od -An -c -N 4 "$O/m.png" 2>/dev/null | tr -d " ")" = "211PNG" ]'
v "m2 앱 창 목록 1(헤드리스 내부)" 'grep -q "wins=1" "$O/m_wins.txt"'
if [ -s "$O/m.oswins" ]; then v "m3 OS 창 0(CGWindowList · wincount PID)" '[ "$(tr -d "[:space:]" < "$O/m.oswins")" = 0 ]'; else echo "  SKIP m3 (wincount 없음)"; fi
echo "== 합계: PASS $pass · FAIL $fail"; for f in "$O"/*.err; do grep -il 'panic' "$f"; done
