#!/usr/bin/env bash
# Linux 시스템 테마 변경 감시 E2E(10-24 · 138차 · T-38 Linux 잔여 · `ui.theme_watch`): dconf 폴더 사건 → 앱이 재판정하는지.
#   ★ 사용자의 dconf·테마는 건드리지 않는다 — 격리 `XDG_CONFIG_HOME`(안에 빈 dconf/ 폴더)으로 앱을 띄우고, 그 폴더에서 GNOME 방식(임시 파일 → rename `user`)으로
#   파일 사건만 일으킨다. 판정 = Debug 앱 stderr의 `NSQL_TRACE_THEME` 줄 `[theme] dconf changed → re-probe`(감시 켬 = 1회 이상 · 끔 = 0회).
# 사용: scripts/linux-theme-watch-e2e.sh -o <출력폴더> [-e target/debug/nexa-sql]
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/timeout.sh"
. "$ROOT/scripts/lib/xvfb.sh"   # Linux = Xvfb 가상 디스플레이로 재실행(T-458 · D-435 · NSQL_XVFB=0이면 실제 화면)
OUT=""; EXE="$ROOT/target/debug/nexa-sql"
while [ $# -gt 0 ]; do case "$1" in -o) OUT=$2; shift 2;; -e) EXE=$2; shift 2;; *) echo "usage: $0 -o <out> [-e gui]"; exit 2;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir>"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/theme-watch-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && printf '%s\n' "$2" | head -6 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
export DISPLAY="${DISPLAY:-:0}"
run_case() { # $1 이름 · $2 ui.theme_watch 값 · $3 기대 줄 수(>=1 | 0)
  local name=$1 watch=$2 want=$3
  local H="$OUT/$name/home" X="$OUT/$name/xdg"; rm -rf "$OUT/$name"; mkdir -p "$H" "$X/dconf"
  printf 'ui.lang=en\ndemo.prompted=on\nui.theme=system\nui.theme_watch=%s\nwindow.main_size=900,600\n' "$watch" > "$H/settings.conf"
  printf 'x' > "$X/dconf/user"   # 처음부터 db 파일이 있는 꼴
  ( XDG_CONFIG_HOME="$X" NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_TRACE_THEME=1 timeout -s KILL 9 "$EXE" > "$OUT/$name/stdout" 2> "$OUT/$name/stderr" ) &
  local p=$!
  sleep 3.5
  # GNOME dconf-service 방식 = 임시 파일에 쓰고 rename(IN_MOVED_TO) · 두 번
  printf 'y' > "$X/dconf/user.tmp"; mv -f "$X/dconf/user.tmp" "$X/dconf/user"; sleep 1.2
  printf 'z' > "$X/dconf/user.tmp"; mv -f "$X/dconf/user.tmp" "$X/dconf/user"
  wait $p 2>/dev/null
  local n; n=$(grep -c '\[theme\] dconf changed' "$OUT/$name/stderr")
  grep -qE 'panicked' "$OUT/$name/stderr" && { bad "$name 패닉" "$(grep -A3 panicked "$OUT/$name/stderr" | head -4)"; return; }
  if [ "$want" = 0 ]; then [ "$n" = 0 ] && ok "$name 감시 끔 = 재판정 0회" || bad "$name 감시 끔인데 $n회" "$(grep theme "$OUT/$name/stderr")"
  else [ "$n" -ge 1 ] && ok "$name 감시 켬 = 재판정 ${n}회(사건 2 · 묶음 300 ms)" || bad "$name 감시 켬인데 0회" "$(tail -5 "$OUT/$name/stderr")"; fi
}
run_case on on 1
run_case off off 0
say "결과: PASS $pass · FAIL $fail · 보고 = $REPORT"
[ $fail = 0 ]
