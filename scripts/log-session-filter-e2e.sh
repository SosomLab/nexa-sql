#!/usr/bin/env bash
# log-session-filter-e2e.sh — T-124 실행 로그 창 **세션 필터** E2E(11-03 · D-446 · 3-OS 헤드리스 · 격리 홈 · 키 주입 0 · OS 창 0).
#   판정 = `log.stat:<파일>` 한 줄(`sessions_seen=N session_col=B entries=E session_filter=F visible=V sessions=[a|b]`) — 설정
#   `log.session_filter`로 필터를 켜면 그 라벨의 줄만 남는다(visible < entries · 0 < visible) · 없는 라벨 = visible 0 · 비면 전부(visible = entries).
#   라벨은 1차 실행의 `sessions=[…]`에서 읽어 2차에 그대로 넣으므로 라벨 형식을 추측하지 않는다(ⓞ″ headless-field-e2e.sh 시나리오 재사용).
#   사용: scripts/log-session-filter-e2e.sh [-o <산출 폴더 · 기본 target/log-sess-hl>] [-e <gui exe>] [-n <nsql exe>]
#   되풀이 = 같은 폴더 덮어쓰기(61 §2-4) · 산출은 tmp-clean 후보(target/<id>/).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"
EXE="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"; T="$ROOT/target/log-sess-hl"
[ -x "$EXE.exe" ] && EXE="$EXE.exe"; [ -x "$NSQL.exe" ] && NSQL="$NSQL.exe"
while getopts "o:e:n:" o; do case $o in o) T=$OPTARG;; e) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
H="$T/home"; D="$T/data"; O="$T/out"; rm -rf "$H" "$D" "$O"; mkdir -p "$H" "$D" "$O"
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
REPORT="$O/report.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }

export NSQL_HOME="$H"
# 시험 DB = nsql 자신으로 만든다(sqlite3 CLI가 없는 PC도 · 61 §3 Windows) — stdin 스크립트.
printf "CREATE TABLE hr_emp(id INTEGER, name TEXT);\nINSERT INTO hr_emp VALUES (1,'kim'),(2,'lee');\n" | "$NSQL" run -c "sqlite:$(w "$H/local.sqlite")" - >/dev/null 2>&1
printf "CREATE TABLE t2(x INTEGER);\nINSERT INTO t2 VALUES (1);\n" | "$NSQL" run -c "sqlite:$(w "$H/local2.sqlite")" - >/dev/null 2>&1
[ -s "$H/local.sqlite" ] && [ -s "$H/local2.sqlite" ] || { echo "FAIL  시험 DB 생성 실패(nsql run)"; exit 1; }
"$NSQL" conn add Local "sqlite:$(w "$H/local.sqlite")" -d sqlite --no-prompt >/dev/null 2>&1
"$NSQL" conn add Local2 "sqlite:$(w "$H/local2.sqlite")" -d sqlite --no-prompt >/dev/null 2>&1
printf 'SELECT * FROM hr_emp;\n' > "$D/q_emp.sql"
BASE=$'ui.lang=en\ngrid.result_tabbar_single=on\ndemo.prompted=on\nwindow.main_size=1200,800\n'
# id conf : 접속 Local → 파일 열기 → Local2 추가 접속 → 실행 → log.stat(접속 둘 · 줄 여럿) → 종료(KILL 10 s = 설계).
run() {
  local id=$1 conf=$2
  printf '%s%s' "$BASE" "$conf" > "$H/settings.conf"
  local cmd="open:$(w "$D/q_emp.sql"),@after:2000:connect:Local2,@after:5000:run.all,@after:6500:log.stat:$(w "$O/$id.stat"),@after:7000:file.exit"
  NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 10 "$EXE" Local > "$O/$id.out" 2> "$O/$id.err"
  echo "  $id rc=$?" >> "$REPORT"
}
field() { tr -d '\r' < "$1" | sed -n "s/.*$2=\([^ ]*\).*/\1/p" | head -1; }

say "== 1차: 필터 없음(기본) — 라벨 수집"
run a ""
if [ -s "$O/a.stat" ]; then
  ok "a.stat 생성: $(tr -d '\r' < "$O/a.stat")"
  SEEN=$(field "$O/a.stat" sessions_seen); ENT=$(field "$O/a.stat" entries); VIS=$(field "$O/a.stat" visible); F=$(field "$O/a.stat" session_filter)
  [ "${SEEN:-0}" -ge 2 ] && ok "a1 세션 라벨 둘 이상($SEEN)" || bad "a1 sessions_seen < 2" "$(cat "$O/a.stat")"
  [ "$F" = "-" ] && ok "a2 필터 없음(session_filter=-)" || bad "a2 session_filter=$F" ""
  [ -n "$ENT" ] && [ "$VIS" = "$ENT" ] && ok "a3 전부 보임(visible=$VIS = entries)" || bad "a3 visible($VIS) != entries($ENT)" ""
  LABELS=$(tr -d '\r' < "$O/a.stat" | sed -n 's/.*sessions=\[\(.*\)\].*/\1/p')
  L2=$(printf '%s' "$LABELS" | awk -F'|' '{print $2}')
else
  bad "a.stat 없음" "$(head -5 "$O/a.err")"; L2=""
fi

say "== 2차: 필터 = 둘째 라벨(${L2:-?})"
if [ -n "$L2" ]; then
  run b "log.session_filter=$L2"$'\n'
  if [ -s "$O/b.stat" ]; then
    ok "b.stat 생성: $(tr -d '\r' < "$O/b.stat")"
    F=$(field "$O/b.stat" session_filter); ENT=$(field "$O/b.stat" entries); VIS=$(field "$O/b.stat" visible)
    [ "$F" = "$L2" ] && ok "b1 설정이 필터로 적용(session_filter=$F)" || bad "b1 session_filter=$F ≠ $L2" ""
    [ -n "$VIS" ] && [ "$VIS" -gt 0 ] && [ "$VIS" -lt "${ENT:-0}" ] && ok "b2 그 세션 줄만(0 < visible=$VIS < entries=$ENT)" || bad "b2 visible=$VIS entries=$ENT" ""
  else
    bad "b.stat 없음" "$(head -5 "$O/b.err")"
  fi
else
  bad "둘째 라벨을 못 읽어 2차 생략" ""
fi

say "== 3차: 없는 라벨 → 0줄"
run c $'log.session_filter=no-such-session\n'
if [ -s "$O/c.stat" ]; then
  VIS=$(field "$O/c.stat" visible); F=$(field "$O/c.stat" session_filter)
  [ "$F" = "no-such-session" ] && [ "$VIS" = "0" ] && ok "c1 없는 라벨 = visible 0" || bad "c1 session_filter=$F visible=$VIS" "$(cat "$O/c.stat")"
else
  bad "c.stat 없음" "$(head -5 "$O/c.err")"
fi
for f in a b c; do grep -qiE 'panick|overflow' "$O/$f.err" 2>/dev/null && bad "$f stderr panic/overflow" "$(grep -iE 'panick|overflow' "$O/$f.err" | head -3)"; done
say "RESULT PASS $pass FAIL $fail"
[ "$fail" = 0 ]
