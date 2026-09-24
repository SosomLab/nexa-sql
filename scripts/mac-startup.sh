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
shown_l=(); cpu_l=(); foot_l=(); rss_l=()
for r in $(seq 1 "$RUNS"); do
  : > "$HOME_DIR/$TAG.stderr"; T0=$(now_ms)
  # shellcheck disable=SC2086
  "$EXE" $ARGS >/dev/null 2>"$HOME_DIR/$TAG.stderr" &
  PID=$!; shown=-1
  while [ $(( $(now_ms) - T0 )) -lt 15000 ]; do
    if [ -s "$HOME_DIR/$TAG.stderr" ]; then shown=$(( $(now_ms) - T0 )); break; fi
    sleep 0.01
  done
  left=$(( SETTLE*1000 - ($(now_ms) - T0) )); [ $left -gt 0 ] && sleep "$(echo "$left/1000" | bc -l)"
  cpu=$(cpu_ms $PID); rss=$(rss_mb $PID); thr=$(thr_n $PID); foot=$(foot_mb $PID)
  printf '%s\trun%d: window %5d ms | cpu(%ds) %5d ms | footprint %6.1f MB | rss %6.1f MB | thr %d\n' "$TAG" "$r" "$shown" "$SETTLE" "$cpu" "$foot" "$rss" "$thr"
  shown_l+=("$shown"); cpu_l+=("$cpu"); foot_l+=("$foot"); rss_l+=("$rss")
  kill $PID 2>/dev/null; wait $PID 2>/dev/null; sleep 0.6
done
printf '%s\tMEDIAN: window %s ms | cpu %s ms | footprint %s MB | rss %s MB\n' "$TAG" "$(med "${shown_l[@]}")" "$(med "${cpu_l[@]}")" "$(med "${foot_l[@]}")" "$(med "${rss_l[@]}")"
