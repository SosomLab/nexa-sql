#!/usr/bin/env bash
# mac-startup.sh — `linux-startup.sh`의 맥 이식(입력 주입 없음 · docs/61 §4): N번 띄워 ① 창이 보일 때까지(`NSQL_TRACE_FRAMES=1` 첫 stderr)
#   ② settle초까지 CPU(ms) ③ footprint · RSS · 스레드 를 재고 중앙값. 빌드 직후 첫 실행은 버린다(26 §7-5).
# 사용: scripts/mac-startup.sh -H <home> -n 5 -s 3 [-a Local] [-e <exe>] [-t tag]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; . "$ROOT/scripts/mac-common.sh"
EXE="$ROOT/target/release/nexa-sql"; HOME_DIR=""; RUNS=5; SETTLE=3; ARGS=""; TAG=startup
while getopts "e:H:n:s:a:t:" o; do case $o in e) EXE=$OPTARG;; H) HOME_DIR=$OPTARG;; n) RUNS=$OPTARG;; s) SETTLE=$OPTARG;; a) ARGS=$OPTARG;; t) TAG=$OPTARG;; esac; done
[ -d "$HOME_DIR" ] || { echo "home dir missing: '$HOME_DIR'"; exit 2; }
export NSQL_HOME="$HOME_DIR" NSQL_NO_ACTIVATE=1 NSQL_TRACE_FRAMES=1
# ★ T-464(11-01): "window" = 첫 stderr 줄(`[startup]` · 창이 뜨기 전 앱 조립 끝)까지의 **바깥** 시계 — 대용치다. 감지 루프는 시계를 부르지 않고(호출 비용이
#   값에 더해진다 · 옛 python 시계 = 160~270 ms/회 → 350~550 ms 계단) 횟수로 15 s 상한 · 감지 뒤 한 번만 읽는다. 창이 실제로 보인 시각은 앱이 잰
#   `[startup:resumed] … window N (ms since main)`(09-24 2부)와 `[frames] … first paint … at +N ms`를 stderr에서 읽어 **app-win · paint** 열로 함께 낸다
#   (프로세스 spawn·dyld 제외 · OS·시계 무관 · 26 §7 비교는 이 둘로).
shown_l=(); cpu_l=(); foot_l=(); rss_l=(); appwin_l=(); paint_l=()
for r in $(seq 1 "$RUNS"); do
  : > "$HOME_DIR/$TAG.stderr"; T0=$(now_ms)
  # shellcheck disable=SC2086
  "$EXE" $ARGS >/dev/null 2>"$HOME_DIR/$TAG.stderr" &
  PID=$!; shown=-1; i=0
  while [ $i -lt 1500 ]; do
    if [ -s "$HOME_DIR/$TAG.stderr" ]; then shown=$(( $(now_ms) - T0 )); break; fi
    sleep 0.01; i=$((i+1))
  done
  left=$(( SETTLE*1000 - ($(now_ms) - T0) )); [ $left -gt 0 ] && sleep "$(echo "$left/1000" | bc -l)"
  cpu=$(cpu_ms $PID); rss=$(rss_mb $PID); thr=$(thr_n $PID); foot=$(foot_mb $PID)
  appwin=$(sed -n 's/.*\[startup:resumed\].* window \([0-9]*\).*/\1/p' "$HOME_DIR/$TAG.stderr" | head -1); appwin=${appwin:--1}
  paint=$(sed -n 's/.*first paint .* at +\([0-9]*\)ms.*/\1/p' "$HOME_DIR/$TAG.stderr" | head -1); paint=${paint:--1}
  printf '%s\trun%d: window %5d ms | app-win %4d | paint +%4d | cpu(%ds) %5d ms | footprint %6.1f MB | rss %6.1f MB | thr %d\n' "$TAG" "$r" "$shown" "$appwin" "$paint" "$SETTLE" "$cpu" "$foot" "$rss" "$thr"
  shown_l+=("$shown"); cpu_l+=("$cpu"); foot_l+=("$foot"); rss_l+=("$rss"); appwin_l+=("$appwin"); paint_l+=("$paint")
  kill $PID 2>/dev/null; wait $PID 2>/dev/null; sleep 0.6
done
printf '%s\tMEDIAN: window %s ms | app-win %s | paint +%s | cpu %s ms | footprint %s MB | rss %s MB\n' "$TAG" "$(med "${shown_l[@]}")" "$(med "${appwin_l[@]}")" "$(med "${paint_l[@]}")" "$(med "${cpu_l[@]}")" "$(med "${foot_l[@]}")" "$(med "${rss_l[@]}")"
