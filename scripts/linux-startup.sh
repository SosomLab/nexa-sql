#!/usr/bin/env bash
# linux-startup.sh — `win-startup-probe.ps1`의 Linux 이식(입력 주입 없음 · docs/61 §4): 프로세스를 N번 띄워 ① 창이 보일 때까지
#   (= `NSQL_TRACE_FRAMES=1`의 첫 stderr 줄 · 표면 생성 시점) ② settle초까지 쓴 CPU ③ 그때의 RssAnon(≈ Private) · RSS · 스레드 · fd 를 재고
#   중앙값을 낸다. 두 빌드를 비교할 때는 번갈아 돌린다(페이지 캐시의 영향을 고르게) · **빌드 직후 첫 실행은 버린다**(26 §7-5).
#
# 사용:  scripts/linux-startup.sh -H /tmp/nsql-home -n 5 -s 3 [-a Local] [-e <exe>] [-t tag]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="$ROOT/target/release/nexa-sql"; HOME_DIR=""; RUNS=5; SETTLE=3; ARGS=""; TAG=startup
while getopts "e:H:n:s:a:t:" o; do case $o in e) EXE=$OPTARG;; H) HOME_DIR=$OPTARG;; n) RUNS=$OPTARG;; s) SETTLE=$OPTARG;; a) ARGS=$OPTARG;; t) TAG=$OPTARG;; esac; done
[ -d "$HOME_DIR" ] || { echo "home dir missing: '$HOME_DIR'"; exit 2; }
CLK=$(getconf CLK_TCK)
export NSQL_HOME="$HOME_DIR" NSQL_NO_ACTIVATE=1 NSQL_TRACE_FRAMES=1
shown_l=(); cpu_l=(); anon_l=(); rss_l=()
for r in $(seq 1 "$RUNS"); do
  : > "$HOME_DIR/$TAG.stderr"; T0=$(date +%s%N)
  # shellcheck disable=SC2086
  "$EXE" $ARGS >/dev/null 2>"$HOME_DIR/$TAG.stderr" &
  PID=$!; shown=-1
  while [ $(( ($(date +%s%N) - T0) / 1000000 )) -lt 15000 ]; do
    if [ -s "$HOME_DIR/$TAG.stderr" ]; then shown=$(( ($(date +%s%N) - T0) / 1000000 )); break; fi
    sleep 0.01
  done
  left=$(( SETTLE*1000 - ($(date +%s%N) - T0)/1000000 )); [ $left -gt 0 ] && sleep "$(echo "$left/1000" | bc -l)"
  stat=$(cat /proc/$PID/stat); stat=${stat##*) }; set -- $stat; cpu=$(( (${12} + ${13}) * 1000 / CLK ))
  anon=$(awk '/^RssAnon/{print $2}' /proc/$PID/status); rss=$(awk '/^VmRSS/{print $2}' /proc/$PID/status)
  thr=$(awk '/^Threads/{print $2}' /proc/$PID/status); fd=$(ls /proc/$PID/fd | wc -l)
  printf '%s\trun%d: window %5d ms | cpu(%ds) %5d ms | anon %6.2f MB | rss %6.1f MB | thr %d | fd %d\n' "$TAG" "$r" "$shown" "$SETTLE" "$cpu" "$(echo "$anon/1024"|bc -l)" "$(echo "$rss/1024"|bc -l)" "$thr" "$fd"
  shown_l+=("$shown"); cpu_l+=("$cpu"); anon_l+=("$anon"); rss_l+=("$rss")
  kill $PID 2>/dev/null; wait $PID 2>/dev/null; sleep 0.6
done
med() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
printf '%s\tMEDIAN: window %s ms | cpu %s ms | anon %.2f MB | rss %.1f MB\n' "$TAG" "$(med "${shown_l[@]}")" "$(med "${cpu_l[@]}")" "$(echo "$(med "${anon_l[@]}")/1024"|bc -l)" "$(echo "$(med "${rss_l[@]}")/1024"|bc -l)"
