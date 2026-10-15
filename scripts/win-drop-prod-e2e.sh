#!/usr/bin/env bash
# win-drop-prod-e2e.sh — 운영(PRD) 접속에서 탐색기 ▸ 삭제… 가 실제로 DROP을 보내는가(docs/114 G6 · 10-15).
#   결함 = 삭제 창의 모달 2단 확인을 거친 DROP이 `run_text_in`의 운영 2단 확인에 걸려 첫 호출은 무장만 하는데 창은 "전송됨" 표시.
#   SQLite 격리 홈 · 프로필 유형 = `nsql conn env Local prd` · 키 주입 0 · 기동 명령 `explorer.menu/pick:drop` → `drop.confirm` → `drop.dump` ·
#   판정 = 표가 사라졌는가(`nsql cat tables`) + 덤프 단계. 대조 = 유형 none에서도 같은 흐름.
# 사용: scripts/win-drop-prod-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/drop-prod-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

run_case() { # <이름> <유형 none|prd>
  local name=$1 env=$2 H="$OUT/$1-home"
  rm -rf "$H"; mkdir -p "$H"
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\nrun.prod_confirm=on\n' > "$H/settings.conf"
  NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  NSQL_HOME="$H" "$NSQL" conn env Local "$env" >/dev/null 2>&1
  printf 'CREATE TABLE t_drop (id INTEGER PRIMARY KEY, v TEXT);\n' > "$OUT/$name.sql"
  NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt "$(m "$OUT/$name.sql")" >/dev/null 2>&1
  local cmd="@after:2500:explorer.expand:main,@after:4000:explorer.expand:Tables,@after:5500:explorer.menu:t_drop,@after:6100:explorer.pick:drop,@after:7500:drop.confirm,@after:12000:drop.dump:$(w "$OUT/$name.dump"),@after:12200:log.dump:$(w "$OUT/$name.log")"
  rm -f "$OUT/$name.dump"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 15 "$EXE" Local > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
  NSQL_HOME="$H" "$NSQL" cat -c Local tables > "$OUT/$name.tables" 2>&1
}

say "drop-prod-e2e $(date '+%F %T')"
# 워밍업(갓 링크된 exe 첫 기동 지연).
NSQL_HOME="$OUT" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@after:1000:ui.move:10/10" timeout -s KILL 3 "$EXE" >/dev/null 2>&1
run_case none none
grep -q "t_drop" "$OUT/none.tables" && bad "대조(none): 표가 남음" "$(cat "$OUT/none.dump" 2>/dev/null)" || ok "대조(none): DROP 전송 → 표 사라짐"
grep -qi "done\|sent\|전송" "$OUT/none.dump" 2>/dev/null && ok "대조(none): 삭제 창 단계 = 전송됨" || bad "대조(none): 창 단계 이상" "$(cat "$OUT/none.dump" 2>/dev/null)"
run_case prd prd
if grep -q "t_drop" "$OUT/prd.tables"; then
  bad "운영(prd): 창은 전송됨인데 표가 남음(= G6 재현)" "$(cat "$OUT/prd.dump" 2>/dev/null; grep -i "prod\|confirm\|운영" "$OUT/prd.log" 2>/dev/null | tail -3)"
else
  ok "운영(prd): DROP 전송 → 표 사라짐(2단 확인은 삭제 창이 대신)"
fi
grep -qi "done\|sent\|전송" "$OUT/prd.dump" 2>/dev/null && ok "운영(prd): 삭제 창 단계 = 전송됨" || bad "운영(prd): 창 단계 이상" "$(cat "$OUT/prd.dump" 2>/dev/null)"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
