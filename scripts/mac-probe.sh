#!/usr/bin/env bash
# mac-probe.sh — `linux-probe.sh`의 맥 이식: 격리 홈 + 기동 명령으로 띄워 초 단위로 CPU(ms)·RSS·스레드, 마지막 두 표본은 footprint도.
# 사용: scripts/mac-probe.sh -H <home> -c "<기동 명령>" -s 12 -t tag [-a Local] [-e exe] [-i 간격]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; . "$ROOT/scripts/mac-common.sh"
EXE="$ROOT/target/release/nexa-sql"; HOME_DIR=""; CMD=""; SECS=20; TAG=probe; ARGS=""; IV=1
while getopts "e:H:c:s:t:a:i:" o; do case $o in e) EXE=$OPTARG;; H) HOME_DIR=$OPTARG;; c) CMD=$OPTARG;; s) SECS=$OPTARG;; t) TAG=$OPTARG;; a) ARGS=$OPTARG;; i) IV=$OPTARG;; esac; done
[ -d "$HOME_DIR" ] || { echo "home dir missing: '$HOME_DIR'"; exit 2; }
export NSQL_HOME="$HOME_DIR" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$CMD"
# shellcheck disable=SC2086
"$EXE" $ARGS >"$HOME_DIR/$TAG.stdout" 2>"$HOME_DIR/$TAG.stderr" &
PID=$!; prev=0; t=0
while [ "$(echo "$t < $SECS" | bc)" = 1 ]; do
  sleep "$IV"; t=$(echo "$t + $IV" | bc)
  kill -0 $PID 2>/dev/null || { echo "$TAG	process exited at t=${t}s"; break; }
  cpu=$(cpu_ms $PID); rss=$(rss_mb $PID); thr=$(thr_n $PID)
  foot="-"; if [ "$(echo "$t >= $SECS - 2*$IV" | bc)" = 1 ]; then foot=$(foot_mb $PID); fi
  printf '%s\tt=%5.1fs cpu+=%5d ms rss=%6.1f MB footprint=%s MB thr=%2d\n' "$TAG" "$t" $((cpu - prev)) "$rss" "$foot" "$thr"
  prev=$cpu
done
echo "$TAG	fd=$(fd_n $PID)"
kill $PID 2>/dev/null; wait $PID 2>/dev/null
echo "$TAG	stopped pid $PID"
