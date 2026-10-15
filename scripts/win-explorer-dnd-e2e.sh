#!/usr/bin/env bash
# win-explorer-dnd-e2e.sh — 객체 탐색기 노드 끌어 놓기(DnD) 자체 시험(10-15 사용자 "객체 탐색기에서 객체를 DnD · 이름 미리보기").
#   SQLite 격리 홈 · 키/마우스 **주입 0**(앱 안 `ui.down/move/up` = 실제 route 경로 · 61 §4) · 2단계:
#   ① `explorer.rowpt:<라벨>:<파일>`로 트리 행의 창 좌표를 받고 ② 같은 창 크기로 다시 띄워 그 좌표에서 누름 → 이동(문턱 4px 초과 ·
#   중간 점을 밟는다 — 점프는 경로를 안 밟는다 102 §12) → 편집기 위에서 놓음 → `editor.dump`에 이름이 들어갔는가.
#   케이스: A 편집기에 놓기 = 이름 삽입 · B 상태줄(놓을 곳 없음)에 놓기 = 삽입 없음 + 끌기 끝 · C 폴더 행(Tables)도 끌어 놓으면 라벨 삽입.
#   (좌표 파일은 끝 개행이 없다 → `read … || [ -n "$OX" ]` · 102 §12 같은 부류)
# 사용: scripts/win-explorer-dnd-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"
REPORT="$OUT/explorer-dnd-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n' > "$H/settings.conf"
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
printf 'CREATE TABLE sales_customer (id INTEGER PRIMARY KEY, cust_name TEXT);\n' > "$OUT/mk.sql"
NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt "$(m "$OUT/mk.sql")" >/dev/null 2>&1
printf -- '-- drop here\n' > "$OUT/doc.sql"
PRE="@connected:open:$(w "$OUT/doc.sql"),@after:2500:explorer.expand:main,@after:4000:explorer.expand:Tables"
gui() { # <이름> <명령> <초>
  local name=$1 cmd=$2 secs=$3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL "$secs" "$EXE" Local > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
}

say "explorer-dnd-e2e $(date '+%F %T')"
# ① 좌표.
rm -f "$OUT/pt_obj" "$OUT/pt_folder"
gui pt "$PRE,@after:6000:explorer.rowpt:sales_customer:$(w "$OUT/pt_obj"),@after:6100:explorer.rowpt:Tables:$(w "$OUT/pt_folder")" 9
{ read -r OX OY || [ -n "${OX:-}" ]; } < "$OUT/pt_obj" 2>/dev/null || { bad "행 좌표 없음(sales_customer)" "$(cat "$OUT/pt.stderr" 2>/dev/null | tail -3)"; say "RESULT PASS $pass FAIL $fail"; exit 1; }
{ read -r FX FY || [ -n "${FX:-}" ]; } < "$OUT/pt_folder" 2>/dev/null || FX=0
[ "$OX" != "none" ] && ok "① 행 좌표 sales_customer = $OX $OY · Tables = $FX $FY" || { bad "① 행 좌표 none"; say "RESULT PASS $pass FAIL $fail"; exit 1; }
EX=820; EY=140   # 편집기 안(1200×800 · 탐색기 오른쪽 · 탭 바 아래)
drag() { # <시작x> <시작y> <끝x> <끝y> → 명령 조각(문턱 넘김 → 중간 점 → 목표)
  local sx=$1 sy=$2 ex=$3 ey=$4
  echo "@after:6000:ui.down:$sx/$sy,@after:6200:ui.move:$((sx+8))/$((sy+6)),@after:6400:ui.move:$((sx+80))/$((sy+10)),@after:6600:ui.move:$(((sx+ex)/2))/$(((sy+ey)/2)),@after:6800:ui.move:$ex/$ey,@after:7000:ui.move:$((ex+2))/$((ey+1)),@after:7300:ui.up:$((ex+2))/$((ey+1))"
}
# A 편집기에 놓기.
gui a "$PRE,$(drag "$OX" "$OY" $EX $EY),@after:8500:editor.dump:$(w "$OUT/a.dump")" 11
if grep -q "sales_customer" "$OUT/a.dump" 2>/dev/null; then ok "A 편집기에 놓기 = 이름 삽입(sales_customer)"; else bad "A 삽입 없음" "$(cat "$OUT/a.dump" 2>/dev/null; tail -3 "$OUT/a.stderr")"; fi
# B 상태줄에 놓기 = 삽입 없음.
gui b "$PRE,$(drag "$OX" "$OY" 600 790),@after:8500:editor.dump:$(w "$OUT/b.dump")" 11
if grep -q "sales_customer" "$OUT/b.dump" 2>/dev/null; then bad "B 놓을 곳 없는데 삽입됨" "$(cat "$OUT/b.dump")"; else ok "B 상태줄에 놓기 = 삽입 없음"; fi
grep -q "^-- drop here" "$OUT/b.dump" 2>/dev/null && ok "B 문서 원문 유지" || bad "B 문서 덤프 이상" "$(cat "$OUT/b.dump" 2>/dev/null)"
# C 폴더 행(Tables) = 라벨 삽입(이름 복사 전 노드 규칙).
if [ "$FX" != "0" ] && [ "$FX" != "none" ]; then
  gui c "$PRE,$(drag "$FX" "$FY" $EX $EY),@after:8500:editor.dump:$(w "$OUT/c.dump")" 11
  if grep -q "Tables" "$OUT/c.dump" 2>/dev/null; then ok "C 폴더 행 끌어 놓기 = 라벨 삽입(Tables)"; else bad "C 라벨 삽입 없음" "$(cat "$OUT/c.dump" 2>/dev/null)"; fi
fi
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
