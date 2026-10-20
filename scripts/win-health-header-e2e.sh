#!/usr/bin/env bash
# win-health-header-e2e.sh — 🔧 131차 win(10-19 · 사용자 실기 0.2.2): 끊긴 끝점의 세션을 **모두 끊고 다시 접속**하면 탐색기 헤더
#   "끊김 hh:mm"만 영영 남던 결함의 회귀. 뿌리 = 세션 0이 되면 `health_gc`가 레지스트리 기록을 지워 재접속 때 `Up`이 안 흘렀다 →
#   수정 = 접속 성공마다 `Up`을 늘 흘리고(`events.rs` 3곳) 조용한 복귀(`Notify::None`)에도 헤더만 걷는다(`header_clear_on_quiet_up`).
#   길 = `@connected:net.break:<ep>`(d1 = 칸 broken + 레지스트리 Broken) → `disc.all`(전부 해제) → `net.heal:<가짜 끝점>`(= gc 유발 ·
#   d2 = 레지스트리에 그 끝점 없음 + 칸 broken 그대로 = **결함 상태 재현**) → `connect:<프로필>`(d3 = 칸 broken 없음 · 세션 connected).
#   실서버 읽기 접속만(객체 생성 0) · 키 주입 0 · 격리 홈(프로필·기기 키는 실제 홈에서 **복사**).
# 사용: scripts/win-health-header-e2e.sh -o <출력폴더> -P <실제 NSQL_HOME> [-c Repository] [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; PROF=""; CONN="Repository"; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:P:c:g:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; c) CONN=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <real NSQL_HOME> [-c profile] [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"
REPORT="$OUT/health-header-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n' > "$H/settings.conf"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
# 끝점 = `nsql conn list`의 TARGET 열(`user@host:port/db`)에서 `host:port`.
EP="$(NSQL_HOME="$H" "$NSQL" conn list 2>/dev/null | awk -v c="$CONN" '$1==c{print $NF}' | sed 's/.*@//; s#/.*##')"
[ -n "$EP" ] || { say "FAIL profile $CONN not found in $PROF (nsql conn list)"; exit 1; }
say "health-header-e2e $(date '+%F %T') · profile=$CONN · ep=$EP"

D1="$OUT/d1.txt"; D2="$OUT/d2.txt"; D3="$OUT/d3.txt"; LOG="$OUT/log.txt"
rm -f "$D1" "$D2" "$D3" "$LOG"
# `@connected:`는 첫 명령(net.break)만 접속 뒤로 미루고 `@after:N`은 **기동 기준 절대 시각**이라(1차 실행 = 1 s 덤프가 접속 전) 실서버 접속(PG 2~3 s)보다
#   넉넉히 뒤에 둔다 — 6 s 뒤부터.
# gc 조건 = 그 끝점을 든 세션이 0 — `disc.all`만으로는 활성 탭 세션이 spec을 그대로 쥐어(connected=false) `health_gc`가 기록을 남긴다(2차 실행 d2) →
#   다른 서버(격리 홈의 SQLite `Local`)에 접속해 활성 세션을 갈아 끼운 뒤 가짜 끝점 `net.heal`(= 아무 건강 사건)로 gc를 돌린다.
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(cygpath -m "$H" 2>/dev/null || echo "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
CMD="@connected:net.break:$EP,@after:6000:health.dump:$(w "$D1"),@after:6500:disc.all,@after:7500:connect:Local,@after:9500:net.heal:nsqlt-fake:1,@after:10000:health.dump:$(w "$D2"),@after:10500:connect:$CONN,@after:17500:health.dump:$(w "$D3"),@after:17800:log.dump:$(w "$LOG")"
NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$CMD" timeout -s KILL 22 "$EXE" "$CONN" > "$OUT/stdout.txt" 2> "$OUT/stderr.txt"

# d1 = 끊김 투영(레지스트리 Broken · 칸 헤더 끊김 hh:mm · 세션 broken).
grep -q "^$EP|Broken" "$D1" 2>/dev/null && ok "d1 레지스트리 $EP Broken" || bad "d1 레지스트리 Broken 아님" "$(cat "$D1" 2>/dev/null)"
grep -qE "^pane $EP\|broken=[0-9]{2}:[0-9]{2}" "$D1" 2>/dev/null && ok "d1 칸 헤더 끊김 hh:mm" || bad "d1 칸 헤더 끊김 없음" "$(cat "$D1" 2>/dev/null)"
grep -qE "^sess [0-9]+\|$EP\|connected=true\|broken=true" "$D1" 2>/dev/null && ok "d1 세션 broken=true" || bad "d1 세션 broken 아님" "$(cat "$D1" 2>/dev/null)"
# d2 = 전부 해제 + gc 뒤 = 레지스트리에 끝점 없음 · 칸 헤더는 아직 끊김(= 종전 결함이 생기는 상태).
! grep -q "^$EP|" "$D2" 2>/dev/null && ok "d2 레지스트리에 $EP 없음(gc)" || bad "d2 레지스트리에 아직 있음" "$(cat "$D2" 2>/dev/null)"
# 칸은 접속이 0이 되면 열쇠만 풀려 남는다(`pane -|…` · 2차 패치부터 덤프에 찍힘) — 🔧 131차 2단 (b): 열쇠를 풀 때 끊김 표식도 비운다(`set_broken(None)`)
#   → 다음 접속(Local)이 이 칸을 재사용해도 "끊김 hh:mm"을 물려받지 않는다. d2 = 열쇠 없는 칸 또는 Local이 재사용한 칸 어느 쪽이든 broken=-.
if grep -qE "^pane -\|" "$D2" 2>/dev/null; then
  grep -qE "^pane -\|broken=-" "$D2" && ok "d2 열쇠 풀린 칸 끊김 표식 비움(pane -|broken=-)" || bad "d2 열쇠 풀린 칸에 끊김 잔존" "$(grep -E '^pane ' "$D2")"
else
  grep -qE "^pane \|broken=-" "$D2" 2>/dev/null && ok "d2 재사용 칸(Local) 끊김 표식 없음" || bad "d2 재사용 칸(Local)에 끊김 잔존 또는 칸 없음" "$(cat "$D2" 2>/dev/null)"
fi
! grep -qE "^sess [0-9]+\|$EP\|connected=true" "$D2" 2>/dev/null && ok "d2 끝점 세션 0" || bad "d2 아직 접속된 세션" "$(cat "$D2" 2>/dev/null)"
# d3 = 다시 접속 뒤 = 칸 헤더 걷힘 · 세션 connected · broken=false.
grep -qE "^sess [0-9]+\|$EP\|connected=true\|broken=false" "$D3" 2>/dev/null && ok "d3 재접속 세션 connected · broken=false" || bad "d3 재접속 안 됨" "$(cat "$D3" 2>/dev/null; tail -6 "$LOG" 2>/dev/null)"
grep -qE "^pane $EP\|broken=-" "$D3" 2>/dev/null && ok "d3 칸 헤더 끊김 걷힘(🔧 131차)" || bad "d3 칸 헤더 끊김 잔존(결함)" "$(cat "$D3" 2>/dev/null)"
# 열쇠가 풀린 끊김 칸을 **다른 서버**(여기서는 SQLite Local · 끝점 없음)가 재사용하면 "끊김 hh:mm"이 그 칸에 그대로 옮겨 붙는다(3차 실행 관찰) —
#   끝점이 없으니 `Up`이 영영 안 와 걷히지 않는다 → 칸 열쇠 교체 때 끊김 표시를 비워야 한다.
other="$(grep -E "^pane " "$D3" 2>/dev/null | grep -v "^pane $EP|" | grep -E "broken=[0-9]{2}:[0-9]{2}")"
[ -z "$other" ] && ok "d3 다른 칸(재사용 칸)에 끊김 잔존 없음" || bad "d3 다른 칸에 끊김 잔존(칸 재사용 때 안 비움)" "$other"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
