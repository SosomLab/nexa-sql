#!/usr/bin/env bash
# win-reconnect-e2e.sh — 끊긴 접속의 **자동 재접속** 회귀(10-09 · 사용자 "전용 세션은 VPN 복귀 뒤 재접속이 안 된다" · 53 §8 · 107 §6).
#   Debug 전용 훅 `sess.kill`이 워커 세션을 실제로 떨어뜨린 뒤(재접속 원천은 유지) 다음 실행이 "다시 접속 중…" 로그와 함께
#   재접속해 결과를 내는지 본다. SQLite 격리 홈 · 키 주입 0 · 실서버 없음.
#   케이스: ① 공유 세션(기동 인자 프로필 · `Cmd::ConnectSpec` 원천) ② 전용 세션(편집기 `CONNECT sqlite:…` 문 = 종전 결함 자리 · 원천 =
#   러너 `connected_to`) — 둘 다 kill 뒤 `SELECT 7` 결과 1행 + 로그 "다시 접속" 1회.
# 사용: scripts/win-reconnect-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"
REPORT="$OUT/reconnect-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

# run_case <이름> <본문> <기동 인자(프로필 또는 빈 값)>
run_case() {
  local name=$1 body=$2 arg=$3 H="$OUT/$1-home"
  rm -rf "$H"; mkdir -p "$H"
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n' > "$H/settings.conf"
  NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  printf '%s\n' "$body" > "$OUT/$name.sql"
  local cmd="open:$(w "$OUT/$name.sql"),@after:1500:run.all,@after:3500:grid.dump:$(w "$OUT/$name.d1"),@after:4000:sess.kill,@after:5000:run.statement,@after:8000:grid.dump:$(w "$OUT/$name.d2"),@after:8200:log.dump:$(w "$OUT/$name.log")"
  [ -n "$arg" ] && cmd="@connected:$cmd"
  rm -f "$OUT/$name.d1" "$OUT/$name.d2" "$OUT/$name.log"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 14 "$EXE" $arg > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
}

check() {
  local name=$1
  grep -q "^rows=1 " "$OUT/$name.d1" 2>/dev/null && ok "$name 첫 실행 1행" || bad "$name 첫 실행 ≠ 1행" "$(head -2 "$OUT/$name.d1" 2>/dev/null)"
  grep -q "sess.kill: session dropped" "$OUT/$name.log" 2>/dev/null && ok "$name 세션 떨어뜨림 로그" || bad "$name kill 로그 없음" "$(tail -6 "$OUT/$name.log" 2>/dev/null)"
  grep -qi "reconnect\|다시 접속" "$OUT/$name.log" 2>/dev/null && ok "$name 다시 접속 로그" || bad "$name 다시 접속 로그 없음" "$(tail -8 "$OUT/$name.log" 2>/dev/null)"
  grep -q "^rows=1 " "$OUT/$name.d2" 2>/dev/null && ok "$name 재접속 뒤 실행 1행" || bad "$name 재접속 뒤 실행 ≠ 1행" "$(head -2 "$OUT/$name.d2" 2>/dev/null; tail -4 "$OUT/$name.log" 2>/dev/null)"
}

say "reconnect-e2e $(date '+%F %T')"
run_case shared "SELECT 7 AS v;" Local; check shared
H2="$OUT/private-home"
run_case private "CONNECT sqlite:$(m "$H2")/local.sqlite;
SELECT 7 AS v;" ""; check private
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
